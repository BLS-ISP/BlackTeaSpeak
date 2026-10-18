use super::*;
use std::collections::BTreeSet;
use std::io;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use tungstenite::error::Error as WebSocketError;
use tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use tungstenite::{Message, accept};

use crate::file_transfer::FileTransferRegistry;
use crate::models::{
    WHISPER_TARGET_CHANNEL, WHISPER_TARGET_CLIENT, WHISPER_TARGET_SELF,
    WHISPER_TARGET_SERVER_GROUP,
};
use crate::runtime::{BaselineRuntime, OnlineClientSnapshot};
use crate::transport::SessionPresence;

pub(crate) fn blackteaweb_disconnect_kind(
    close_frame_received: bool,
    ping_timeout_triggered: bool,
) -> BlackTeaWebDisconnectKind {
    if ping_timeout_triggered || !close_frame_received {
        BlackTeaWebDisconnectKind::TimedOut
    } else {
        BlackTeaWebDisconnectKind::LeftServer
    }
}

pub(crate) fn blackteaweb_disconnect_reason(kind: BlackTeaWebDisconnectKind) -> (u32, &'static str) {
    match kind {
        BlackTeaWebDisconnectKind::LeftServer => (8, "left server"),
        BlackTeaWebDisconnectKind::TimedOut => (3, ""),
    }
}

pub(crate) fn handle_client(
    stream: TcpStream,
    runtime: Arc<Mutex<BaselineRuntime>>,
    tls_config: Arc<ServerConfig>,
    file_transfers: Arc<FileTransferRegistry>,
    sessions: SharedBlackTeaWebSessions,
    connection_id: u64,
) -> Result<()> {
    let connection_ip = stream
        .peer_addr()
        .map(|peer_addr| peer_addr.ip().to_string())
        .unwrap_or_default();
    if blackteaweb_trace_enabled() {
        match stream.peer_addr() {
            Ok(peer_addr) => tracing::debug!("[blackteaweb:{connection_id}] accepted {peer_addr}"),
            Err(error) => tracing::debug!(
                "[blackteaweb:{connection_id}] accepted client with unknown peer address: {error:#}"
            ),
        }
    }
    stream
        .set_nonblocking(false)
        .context("failed to switch BlackTeaWeb client socket to blocking mode")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .context("failed to set BlackTeaWeb client read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .context("failed to set BlackTeaWeb client write timeout")?;

    let tls_stream = StreamOwned::new(
        ServerConnection::new(Arc::clone(&tls_config))
            .context("failed to create TLS server connection")?,
        stream,
    );
    let mut websocket =
        accept(tls_stream).context("failed to accept BlackTeaWeb websocket connection")?;
    let mut session = BlackTeaWebSessionHandler::new_with_connection_ip(connection_id, connection_ip);
    session.set_file_transfers(file_transfers);
    session.set_sessions(Arc::clone(&sessions));
    let pending_frames = Arc::new(Mutex::new(Vec::new()));
    let mut close_frame_received = false;
    let mut ping_timeout_triggered = false;
    let mut last_activity = Instant::now();

    loop {
        flush_pending_frames(&mut websocket, &pending_frames)?;
        match websocket.read() {
            Ok(Message::Text(text)) => {
                last_activity = Instant::now();
                trace_blackteaweb_frame(connection_id, "in", text.as_ref());
                let before_presence = session.presence();
                let mut outbound = {
                    let mut runtime = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    runtime.mark_client_seen(session.client_id);
                    session.handle_text_frame(text.as_ref(), &mut runtime)?
                };
                let after_presence = session.presence();

                if let Some(after_presence) = after_presence.as_ref() {
                    let visible_channel_ids = {
                        let runtime = runtime
                            .lock()
                            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                        session.visible_channel_ids(&runtime)
                    };
                    register_or_update_session(
                        &sessions,
                        after_presence.clone(),
                        session
                            .self_client_database_id()
                            .unwrap_or(session.client_id + 1000),
                        visible_channel_ids,
                        Arc::clone(&pending_frames),
                    )?;
                }
                session.sync_rtc_presence()?;

                let pending_permission_refreshes = session.drain_pending_permission_refreshes();
                if !pending_permission_refreshes.is_empty() {
                    let runtime = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    broadcast_permission_refreshes(
                        &sessions,
                        &runtime,
                        &pending_permission_refreshes,
                    )?;
                }
                let pending_broadcasts = session.drain_pending_broadcasts();
                if !pending_broadcasts.is_empty() {
                    broadcast_queued_frames(&sessions, &pending_broadcasts)?;
                }
                let mut query_notifications = if before_presence.is_none() {
                    derive_query_notifications_from_presence(&before_presence, &after_presence)
                } else {
                    Vec::new()
                };
                query_notifications.extend(session.drain_pending_query_notifications());
                if !query_notifications.is_empty() {
                    if let Ok(rt) = runtime.lock() {
                        if let Some(pres) = session.presence() {
                            for notif in &query_notifications {
                                rt.broadcast_event(pres.server_id, notif);
                            }
                        }
                    }
                }
                let pending = drain_pending_frames(&pending_frames)?;
                insert_frames_before_error(&mut outbound, pending);

                for message in outbound {
                    trace_blackteaweb_frame(connection_id, "out", &message);
                    websocket
                        .send(Message::Text(message.into()))
                        .context("failed to write BlackTeaWeb websocket frame")?;
                }
            }
            Ok(Message::Ping(payload)) => {
                last_activity = Instant::now();
                if blackteaweb_trace_enabled() {
                    tracing::debug!("[blackteaweb:{connection_id}] ping {} bytes", payload.len());
                }
                if let Ok(mut rt) = runtime.lock() {
                    rt.mark_client_seen(session.client_id);
                }
                websocket
                    .send(Message::Pong(payload))
                    .context("failed to answer websocket ping")?;
            }
            Ok(Message::Close(frame)) => {
                close_frame_received = true;
                if blackteaweb_trace_enabled() {
                    tracing::debug!("[blackteaweb:{connection_id}] close frame: {frame:?}");
                }
                break;
            }
            Ok(_) => {
                last_activity = Instant::now();
            }
            Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => {
                if blackteaweb_trace_enabled() {
                    tracing::debug!("[blackteaweb:{connection_id}] websocket closed");
                }
                break;
            }
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                if last_activity.elapsed() >= TEAWEB_IDLE_TIMEOUT {
                    ping_timeout_triggered = true;
                    let _ = websocket.send(Message::Close(Some(CloseFrame {
                        code: CloseCode::Normal,
                        reason: TEAWEB_TIMEOUT_CLOSE_REASON.into(),
                    })));
                    break;
                }
                continue;
            }
            Err(error) => return Err(error).context("BlackTeaWeb websocket processing failed"),
        }
    }

    let disconnect_kind = blackteaweb_disconnect_kind(close_frame_received, ping_timeout_triggered);
    let (disconnect_reason_id, disconnect_reason_message) =
        blackteaweb_disconnect_reason(disconnect_kind);

    let disconnect_presence = session.presence();
    unregister_session(&sessions, session.client_id)?;
    session.remove_from_rtc()?;
    {
        let mut runtime = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        runtime.remove_session_client(session.client_id, disconnect_reason_id, disconnect_reason_message.to_string());
    }
    let disconnect_cleanup_notifications = {
        let mut runtime = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        match disconnect_presence.as_ref() {
            Some(presence) => _cleanup_notifications(
                presence.server_id,
                runtime.cleanup_temporary_channels(presence.server_id, &[presence.channel_id]),
                0,
                "",
                "",
            ),
            None => Vec::new(),
        }
    };
    if !disconnect_cleanup_notifications.is_empty() {
        if let Ok(rt) = runtime.lock() {
            if let Some(pres) = disconnect_presence.as_ref() {
                for notif in &disconnect_cleanup_notifications {
                    rt.broadcast_event(pres.server_id, notif);
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn blackteaweb_trace_enabled() -> bool {
    matches!(
        std::env::var("TEASPEAK_TEAWEB_TRACE").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "True" | "yes" | "on")
    )
}

pub(crate) fn trace_blackteaweb_frame(connection_id: u64, direction: &str, payload: &str) {
    if blackteaweb_trace_enabled() {
        tracing::debug!("[blackteaweb:{connection_id}] {direction} {payload}");
    }
}

pub(crate) fn list_session_presences(
    sessions: &SharedBlackTeaWebSessions,
    server_id: u32,
    exclude_client_id: Option<u64>,
) -> Result<Vec<BlackTeaWebPresence>> {
    let sessions = sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
    let mut presences = sessions
        .values()
        .filter(|session| {
            session.presence.server_id == server_id
                && exclude_client_id != Some(session.presence.client_id)
        })
        .map(|session| session.presence.clone())
        .collect::<Vec<_>>();
    presences.sort_by(|left, right| {
        left.channel_id
            .cmp(&right.channel_id)
            .then_with(|| left.client_id.cmp(&right.client_id))
    });
    Ok(presences)
}

pub(crate) fn register_or_update_session(
    sessions: &SharedBlackTeaWebSessions,
    presence: BlackTeaWebPresence,
    client_database_id: u64,
    visible_channel_ids: BTreeSet<u32>,
    pending_frames: SharedPendingFrames,
) -> Result<()> {
    let mut lock = sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
    
    let existing_wtransport = lock.get(&presence.client_id).and_then(|s| s.wtransport_session.clone());
    
    lock.insert(
        presence.client_id,
        RegisteredBlackTeaWebSession {
            presence,
            client_database_id,
            visible_channel_ids,
            pending_frames,
            wtransport_session: existing_wtransport,
        },
    );
    Ok(())
}

pub(crate) fn unregister_session(sessions: &SharedBlackTeaWebSessions, client_id: u64) -> Result<()> {
    sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?
        .remove(&client_id);
    Ok(())
}

pub(crate) fn blackteaweb_presence_from_snapshot(snapshot: OnlineClientSnapshot) -> BlackTeaWebPresence {
    let mut client_state = row_map([
        ("client_nickname", snapshot.nickname),
        ("client_unique_identifier", snapshot.unique_identifier),
        ("client_type", snapshot.client_type.to_string()),
        ("client_type_exact", snapshot.client_type_exact.to_string()),
        ("client_database_id", snapshot.database_id.to_string()),
        (
            "client_servergroups",
            snapshot
                .server_groups
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        ("client_version", snapshot.version),
        ("client_platform", snapshot.platform),
        ("client_country", snapshot.country),
        (
            "client_away",
            if snapshot.away {
                String::from("1")
            } else {
                String::from("0")
            },
        ),
        ("client_away_message", snapshot.away_message),
        (
            "client_input_muted",
            if snapshot.input_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        ),
        (
            "client_output_muted",
            if snapshot.output_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        ),
        ("connection_client_ip", snapshot.connection_ip),
    ]);
    for key in [
        "player_state",
        "player_volume",
        "client_playlist_id",
        "client_disabled",
        "client_flag_notify_song_change",
        "client_bot_type",
        "client_uptime_mode",
        "client_description",
        "client_flag_avatar",
    ] {
        if let Some(value) = snapshot.extra_properties.get(key) {
            client_state.insert(String::from(key), value.clone());
        }
    }
    BlackTeaWebPresence {
        client_id: snapshot.id,
        server_id: snapshot.server_id,
        channel_id: snapshot.channel_id,
        client_state,
    }
}

pub(crate) fn blackteaweb_presence_from_transport_presence(presence: &SessionPresence) -> BlackTeaWebPresence {
    BlackTeaWebPresence {
        client_id: presence.client_id,
        server_id: presence.server_id,
        channel_id: presence.channel_id,
        client_state: row_map([
            ("client_nickname", presence.login_name.clone()),
            (
                "client_unique_identifier",
                presence.unique_identifier.clone(),
            ),
            ("client_type", presence.client_type.to_string()),
            ("client_type_exact", presence.client_type.to_string()),
            ("client_database_id", String::from("0")),
            ("client_servergroups", String::new()),
            ("client_version", String::new()),
            ("client_platform", String::new()),
            ("client_country", String::new()),
            ("connection_client_ip", String::new()),
        ]),
    }
}

pub(crate) fn session_presence_from_blackteaweb_presence(presence: &BlackTeaWebPresence) -> SessionPresence {
    SessionPresence {
        client_id: presence.client_id,
        login_name: presence
            .client_state
            .get("client_nickname")
            .cloned()
            .unwrap_or_else(|| String::from("BlackTeaWeb User")),
        unique_identifier: presence
            .client_state
            .get("client_unique_identifier")
            .cloned()
            .unwrap_or_else(|| format!("compat-web-{}", presence.client_id)),
        client_type: presence
            .client_state
            .get("client_type")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0),
        server_id: presence.server_id,
        channel_id: presence.channel_id,
    }
}

pub(crate) fn parse_whisper_target_selection(rows: &[CommandRow]) -> Result<(u32, crate::models::WhisperTargetSelection)> {
    let Some(first_row) = rows.first() else {
        return Err(anyhow!("missing whispersessioninitialize payload"));
    };
    let Some(ssrc) = first_row
        .get("ssrc")
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return Err(anyhow!("missing whisper ssrc"));
    };

    let mut selection = crate::models::WhisperTargetSelection::default();
    for row in rows {
        let row_ssrc = row
            .get("ssrc")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(ssrc);
        if row_ssrc != ssrc {
            return Err(anyhow!("whisper ssrc must stay consistent"));
        }

        let target_type = row
            .get("type")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or_default();
        let target = row
            .get("target")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_default();
        let id = row
            .get("id")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_default();

        match target_type {
            WHISPER_TARGET_SELF => {
                if !(target == 0 && id == 0) {
                    return Err(anyhow!("invalid echo whisper target"));
                }
                selection.echo_self = true;
            }
            WHISPER_TARGET_CHANNEL => {
                let channel_id = u32::try_from(target)
                    .map_err(|_| anyhow!("invalid whisper channel target"))?;
                if channel_id == 0 {
                    return Err(anyhow!("invalid whisper channel target"));
                }
                selection.channel_ids.insert(channel_id);
            }
            WHISPER_TARGET_CLIENT => {
                if target == 0 {
                    return Err(anyhow!("invalid whisper client target"));
                }
                selection.client_ids.insert(target);
            }
            WHISPER_TARGET_SERVER_GROUP => {
                let group_id = u32::try_from(target)
                    .map_err(|_| anyhow!("invalid whisper server-group target"))?;
                if group_id == 0 {
                    return Err(anyhow!("invalid whisper server-group target"));
                }
                selection.server_group_ids.insert(group_id);
            }
            other => return Err(anyhow!("unsupported whisper target {other}")),
        }
    }

    if selection.is_empty() {
        return Err(anyhow!("missing whisper target"));
    }

    Ok((ssrc, selection))
}

pub(crate) fn flush_pending_frames(
    websocket: &mut tungstenite::WebSocket<StreamOwned<ServerConnection, TcpStream>>,
    pending_frames: &SharedPendingFrames,
) -> Result<()> {
    for frame in drain_pending_frames(pending_frames)? {
        websocket
            .send(Message::Text(frame.into()))
            .context("failed to flush queued BlackTeaWeb websocket frame")?;
    }
    Ok(())
}

pub(crate) fn drain_pending_frames(pending_frames: &SharedPendingFrames) -> Result<Vec<String>> {
    let mut pending_frames = pending_frames
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?;
    Ok(pending_frames.drain(..).collect())
}

pub(crate) fn insert_frames_before_error(outbound: &mut Vec<String>, mut extra_frames: Vec<String>) {
    if extra_frames.is_empty() {
        return;
    }

    let insert_at = outbound
        .iter()
        .rposition(|frame| frame.contains("\"command\":\"error\""))
        .unwrap_or(outbound.len());
    outbound.splice(insert_at..insert_at, extra_frames.drain(..));
}
