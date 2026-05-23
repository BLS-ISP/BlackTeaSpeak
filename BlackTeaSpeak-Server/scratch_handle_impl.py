import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

new_fn = """use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn handle_wtransport_client(
    incoming: wtransport::endpoint::IncomingSession,
    runtime: Arc<Mutex<BaselineRuntime>>,
    file_transfers: Arc<FileTransferRegistry>,
    sessions: SharedBlackTeaWebSessions,
    query_bridge: Option<QueryNotificationBridge>,
    connection_id: u64,
) -> Result<()> {
    let session_request = incoming.await?;
    let wtransport_session = session_request.accept().await?;
    
    let (mut send, recv) = wtransport_session.accept_bi().await?;
    let mut recv_reader = BufReader::new(recv);
    
    let connection_ip = wtransport_session.remote_address().to_string();
    
    if blackteaweb_trace_enabled() {
        eprintln!("[webtransport:{connection_id}] accepted {connection_ip}");
    }
    
    let mut session = BlackTeaWebSessionHandler::new_with_connection_ip(connection_id, connection_ip);
    session.set_file_transfers(file_transfers);
    session.set_sessions(Arc::clone(&sessions));
    let pending_frames = Arc::new(Mutex::new(Vec::new()));
    let mut close_frame_received = false;
    let mut ping_timeout_triggered = false;
    let mut last_activity = tokio::time::Instant::now();

    let mut line_buf = String::new();

    loop {
        for frame in drain_pending_frames(&pending_frames)? {
            let mut data = frame;
            data.push('\\n');
            send.write_all(data.as_bytes()).await.context("failed to flush queued WebTransport frame")?;
        }
        
        line_buf.clear();
        let read_result = tokio::time::timeout(
            Duration::from_millis(250),
            recv_reader.read_line(&mut line_buf)
        ).await;

        match read_result {
            Ok(Ok(0)) => {
                close_frame_received = true;
                break;
            }
            Ok(Ok(_)) => {
                last_activity = tokio::time::Instant::now();
                let text = line_buf.trim_end();
                if text.is_empty() {
                    send.write_all(b"\\n").await?;
                    continue;
                }
                
                trace_blackteaweb_frame(connection_id, "in", text);
                let before_presence = session.presence();
                let mut outbound = {
                    let mut rt = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    session.handle_text_frame(text, &mut rt)?
                };
                let after_presence = session.presence();

                if let Some(after_presence) = after_presence.as_ref() {
                    let visible_channel_ids = {
                        let rt = runtime
                            .lock()
                            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                        session.visible_channel_ids(&rt)
                    };
                    let existing_peers = list_session_presences(
                        &sessions,
                        after_presence.server_id,
                        Some(after_presence.client_id),
                    )?
                    .into_iter()
                    .filter(|presence| visible_channel_ids.contains(&presence.channel_id))
                    .collect::<Vec<_>>();
                    if before_presence.is_none() && !existing_peers.is_empty() {
                        insert_frames_before_error(
                            &mut outbound,
                            vec![command_frame(
                                "notifycliententerview",
                                existing_peers
                                    .iter()
                                    .map(|presence| presence_enter_view_row(presence, None, 2))
                                    .collect(),
                            )?],
                        );
                    }
                    register_or_update_session(
                        &sessions,
                        after_presence.clone(),
                        session
                            .self_client_database_id()
                            .expect("connected BlackTeaWeb session should expose a database id"),
                        visible_channel_ids,
                        Arc::clone(&pending_frames),
                    )?;
                }
                session.sync_rtc_presence()?;

                let mut direct_frames = Vec::new();
                let peer_frames = derive_peer_frames(&before_presence, &after_presence)?;
                if let Some(frame) = derive_direct_frame(&before_presence, &after_presence)? {
                    direct_frames.push(frame);
                }
                if !peer_frames.is_empty() {
                    broadcast_frames_for_presence_change(&sessions, &peer_frames)?;
                }
                let pending_permission_refreshes = session.drain_pending_permission_refreshes();
                if !pending_permission_refreshes.is_empty() {
                    let rt = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    broadcast_permission_refreshes(
                        &sessions,
                        &rt,
                        &pending_permission_refreshes,
                    )?;
                }
                let pending_broadcasts = session.drain_pending_broadcasts();
                if !pending_broadcasts.is_empty() {
                    broadcast_queued_frames(&sessions, &pending_broadcasts)?;
                }
                let mut query_notifications =
                    derive_query_notifications_from_presence(&before_presence, &after_presence);
                query_notifications.extend(session.drain_pending_query_notifications());
                if !query_notifications.is_empty() {
                    if let Ok(rt) = runtime.lock() {
                        for notif in &query_notifications {
                            rt.broadcast_event(session.presence().unwrap().server_id, notif);
                        }
                    }
                }
                if !direct_frames.is_empty() {
                    insert_frames_before_error(&mut outbound, direct_frames);
                }
                outbound.extend(drain_pending_frames(&pending_frames)?);

                for message in outbound {
                    trace_blackteaweb_frame(connection_id, "out", &message);
                    let mut data = message;
                    data.push('\\n');
                    send.write_all(data.as_bytes())
                        .await
                        .context("failed to write WebTransport frame")?;
                }
            }
            Ok(Err(error)) => {
                eprintln!("[webtransport:{connection_id}] read error: {error}");
                break;
            }
            Err(_) => {
                if last_activity.elapsed() >= TEAWEB_IDLE_TIMEOUT {
                    ping_timeout_triggered = true;
                    break;
                }
            }
        }
    }

    let disconnect_kind = blackteaweb_disconnect_kind(close_frame_received, ping_timeout_triggered);
    let (disconnect_reason_id, disconnect_reason_message) =
        blackteaweb_disconnect_reason(disconnect_kind);

    let disconnect_presence = session.presence();
    unregister_session(&sessions, session.client_id)?;
    session.remove_from_rtc()?;
    {
        let mut rt = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        if let Some(disconnect_presence) = disconnect_presence
                .as_ref()
                .map(session_presence_from_blackteaweb_presence)
        {
            let notif = TransportNotification::ClientLeftView {
                presence: disconnect_presence.clone(),
                to_channel_id: None,
                reason_id: disconnect_reason_id,
                reason_message: disconnect_reason_message.to_string(),
                invoker_id: 0,
                invoker_name: String::new(),
                invoker_uid: String::new(),
                ban_time: None,
            };
            rt.broadcast_event(disconnect_presence.server_id, &notif);
        }
        rt.remove_session_client(session.client_id);
    }
    let disconnect_cleanup_notifications = {
        let mut rt = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        match disconnect_presence.as_ref() {
            Some(presence) => query_bridge_cleanup_notifications(
                presence.server_id,
                rt.cleanup_temporary_channels(presence.server_id, &[presence.channel_id]),
                0,
                "",
                "",
            ),
            None => Vec::new(),
        }
    };
    if !disconnect_cleanup_notifications.is_empty() {
        if let Ok(rt) = runtime.lock() {
            for notif in &disconnect_cleanup_notifications {
                rt.broadcast_event(session.presence().unwrap().server_id, notif);
            }
        }
        let rt = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        let cleanup_frames = visibility_aware_transport_broadcasts(
            &sessions,
            &rt,
            Some(session.client_id),
            &disconnect_cleanup_notifications,
        )?;
        broadcast_queued_frames(&sessions, &cleanup_frames)?;
    }
    if let Some(disconnect_presence) = disconnect_presence {
        broadcast_frames_for_presence_change(
            &sessions,
            &[PresenceBroadcast::PeerLeft {
                server_id: disconnect_presence.server_id,
                exclude_client_id: Some(disconnect_presence.client_id),
                presence: disconnect_presence,
                to_channel_id: None,
                reason_id: disconnect_reason_id,
                reason_message: disconnect_reason_message.to_string(),
            }],
        )?;
    }

    Ok(())
}
"""

content = re.sub(r"async fn handle_wtransport_client\(.*?\}", new_fn, content, flags=re.DOTALL)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
