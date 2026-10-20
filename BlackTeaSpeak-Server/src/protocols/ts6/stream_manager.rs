use super::wire_formatting::*;
use crate::runtime::ActiveMediaBroadcast;
use crate::teaspeak_transport::UdpSession;
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::net::UdpSocket;

static NEXT_TS6_STREAM_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct Ts6StreamEntry {
    pub id: String,
    pub server_id: u32,
    pub channel_id: u32,
    pub clid: u64,
    pub name: String,
    /// `0` = Desktop / Screen Share, `1` = Camera / Video (or TS6 stream type enum)
    pub stream_type: u32,
    pub bitrate: u64,
    pub accessibility: u32,
    /// `1` = P2P WebRTC (`STREAM_P2P`), `2` = SFU
    pub mode: u32,
    pub viewer_limit: u32,
    pub audio: u32,
    pub sfu_endpoint: String,
    pub sfu_user_id: String,
    pub attendees: BTreeSet<u64>,
}

impl Ts6StreamEntry {
    pub fn render_notifystreamstarted(&self, return_code: Option<&str>) -> String {
        let mut cmd = format!(
            "notifystreamstarted clid={} id={} name={} type={} access={} accessibility={} mode={} bitrate={} viewer_limit={} audio={} sfu_endpoint={} sfu_user_id={}",
            self.clid,
            ts6_escape(&self.id),
            ts6_escape(&self.name),
            self.stream_type,
            self.accessibility,
            self.accessibility,
            self.mode,
            self.bitrate,
            self.viewer_limit,
            self.audio,
            ts6_escape(&self.sfu_endpoint),
            ts6_escape(&self.sfu_user_id),
        );
        if let Some(rc) = return_code {
            cmd.push_str(&format!(" return_code={}", ts6_escape(rc)));
        }
        cmd
    }

    pub fn render_notifystreamupdated(&self, return_code: Option<&str>) -> String {
        let mut cmd = format!(
            "notifystreamupdated clid={} id={} name={} type={} access={} accessibility={} mode={} bitrate={} viewer_limit={} audio={}",
            self.clid,
            ts6_escape(&self.id),
            ts6_escape(&self.name),
            self.stream_type,
            self.accessibility,
            self.accessibility,
            self.mode,
            self.bitrate,
            self.viewer_limit,
            self.audio,
        );
        if let Some(rc) = return_code {
            cmd.push_str(&format!(" return_code={}", ts6_escape(rc)));
        }
        cmd
    }

    pub fn render_notifystreaminfo_row(&self, return_code: Option<&str>) -> String {
        let mut row = format!(
            "id={} clid={} name={} type={} accessibility={} access={} mode={} viewer={} bitrate={} viewer_limit={} audio={} sfu_endpoint={} sfu_user_id={}",
            ts6_escape(&self.id),
            self.clid,
            ts6_escape(&self.name),
            self.stream_type,
            self.accessibility,
            self.accessibility,
            self.mode,
            self.attendees.len(),
            self.bitrate,
            self.viewer_limit,
            self.audio,
            ts6_escape(&self.sfu_endpoint),
            ts6_escape(&self.sfu_user_id),
        );
        if let Some(rc) = return_code {
            row.push_str(&format!(" return_code={}", ts6_escape(rc)));
        }
        row
    }
}

#[derive(Default)]
pub struct Ts6StreamRegistry {
    /// Keyed by stream `id`
    pub streams: BTreeMap<String, Ts6StreamEntry>,
}

pub static TS6_STREAM_REGISTRY: LazyLock<Mutex<Ts6StreamRegistry>> =
    LazyLock::new(|| Mutex::new(Ts6StreamRegistry::default()));

/// Returns true if the given client currently has at least one active TS6 stream on `server_id`.
pub fn is_client_streaming_ts6(server_id: u32, clid: u64) -> bool {
    let reg = TS6_STREAM_REGISTRY.lock().unwrap();
    reg.streams
        .values()
        .any(|s| s.server_id == server_id && s.clid == clid)
}

/// Returns all active TS6 streams on `server_id` so a newly joined TS6 client can receive initial stream state.
pub fn active_ts6_streams_for_server(server_id: u32) -> Vec<Ts6StreamEntry> {
    let reg = TS6_STREAM_REGISTRY.lock().unwrap();
    reg.streams
        .values()
        .filter(|s| s.server_id == server_id)
        .cloned()
        .collect()
}

/// Syncs the client's `client_is_streaming` and `ActiveMediaBroadcast` state into `BaselineRuntime`
/// and broadcasts `ClientUpdated` + `VideoBroadcastChanged` notifications.
fn sync_runtime_stream_state(
    runtime: &Arc<Mutex<crate::runtime::BaselineRuntime>>,
    server_id: u32,
    clid: u64,
) {
    let client_streams: Vec<Ts6StreamEntry> = {
        let reg = TS6_STREAM_REGISTRY.lock().unwrap();
        reg.streams
            .values()
            .filter(|s| s.server_id == server_id && s.clid == clid)
            .cloned()
            .collect()
    };

    let is_streaming = !client_streams.is_empty();
    let has_video = client_streams.iter().any(|s| s.stream_type == 1);
    let has_screen = client_streams.iter().any(|s| s.stream_type != 1);

    let mut rt = runtime.lock().unwrap();
    let before_snap = rt.online_client_snapshot(server_id, clid);
    let channel_id = rt
        .store
        .online_clients
        .get(&clid)
        .map(|c| c.channel_id)
        .unwrap_or(1);

    if let Some(online) = rt.store.online_clients.get_mut(&clid) {
        online.extra_properties.insert(
            String::from("client_is_streaming"),
            if is_streaming {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        online.extra_properties.insert(
            String::from("client_flag_video_broadcasting"),
            if has_video {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        online.extra_properties.insert(
            String::from("client_flag_screen_broadcasting"),
            if has_screen {
                String::from("1")
            } else {
                String::from("0")
            },
        );
    }

    // Keep `active_broadcasts` in sync so BTEA Desktop & TeaSpeak clients also see TS6 streams
    if has_video {
        rt.store.active_broadcasts.insert(
            (server_id, clid, 0),
            ActiveMediaBroadcast {
                server_id,
                channel_id,
                broadcaster_client_id: clid,
                broadcast_type: 0,
                ssrc: (clid as u32).wrapping_mul(10).wrapping_add(1),
                joined_client_ids: BTreeSet::new(),
            },
        );
    } else {
        rt.store.active_broadcasts.remove(&(server_id, clid, 0));
    }

    if has_screen {
        rt.store.active_broadcasts.insert(
            (server_id, clid, 1),
            ActiveMediaBroadcast {
                server_id,
                channel_id,
                broadcaster_client_id: clid,
                broadcast_type: 1,
                ssrc: (clid as u32).wrapping_mul(10).wrapping_add(2),
                joined_client_ids: BTreeSet::new(),
            },
        );
    } else {
        rt.store.active_broadcasts.remove(&(server_id, clid, 1));
    }

    let after_snap = rt.online_client_snapshot(server_id, clid);
    if let (Some(before), Some(after)) = (before_snap, after_snap)
        && crate::transport::server::client_update_fields(&before, &after).len() > 1
    {
        let notif = crate::transport::TransportNotification::ClientUpdated {
            server_id,
            before,
            after,
        };
        rt.broadcast_event(server_id, &notif);
    }

    let bcast_notif = crate::transport::TransportNotification::VideoBroadcastChanged {
        server_id,
        channel_id,
    };
    rt.broadcast_event(server_id, &bcast_notif);
}

/// Cleans up all streams owned by or viewed by `clid` when a TS6 client disconnects or switches channels.
pub fn cleanup_client_streams(
    server_id: u32,
    clid: u64,
    runtime: &Arc<Mutex<crate::runtime::BaselineRuntime>>,
) {
    let mut stopped_streams = Vec::new();
    let mut left_streams = Vec::new();

    {
        let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
        let owned_ids: Vec<String> = reg
            .streams
            .values()
            .filter(|s| s.server_id == server_id && s.clid == clid)
            .map(|s| s.id.clone())
            .collect();

        for id in owned_ids {
            if let Some(removed) = reg.streams.remove(&id) {
                stopped_streams.push(removed);
            }
        }

        for stream in reg.streams.values_mut() {
            if stream.server_id == server_id && stream.attendees.remove(&clid) {
                left_streams.push(stream.id.clone());
            }
        }
    }

    if !stopped_streams.is_empty() {
        sync_runtime_stream_state(runtime, server_id, clid);
    }

    let rt = runtime.lock().unwrap();
    for stopped in stopped_streams {
        let cmd = format!(
            "notifystreamstopped clid={} id={} reason=0",
            clid,
            ts6_escape(&stopped.id)
        );
        rt.broadcast_event(
            server_id,
            &crate::transport::TransportNotification::Ts6StreamNotify {
                server_id,
                target_client_id: None,
                exclude_client_id: Some(clid),
                command: cmd,
            },
        );
    }

    for stream_id in left_streams {
        let cmd = format!(
            "notifystreamclientleft clid={} id={} reason=0",
            clid,
            ts6_escape(&stream_id)
        );
        rt.broadcast_event(
            server_id,
            &crate::transport::TransportNotification::Ts6StreamNotify {
                server_id,
                target_client_id: None,
                exclude_client_id: Some(clid),
                command: cmd,
            },
        );
    }
}

/// Handles all TeamSpeak 6 Video & Desktop/Screen-Sharing commands (`setupstream`, `updatestream`,
/// `stopstream`, `requeststreaminfo`, `joinstreamrequest`, `respondjoinstreamrequest`,
/// `streamsignaling`, `removeclientfromstream`, `requestsfuaccessinfo`).
pub async fn handle_ts6_stream_command(
    payload_str: &str,
    session: &mut UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
    runtime: &Arc<Mutex<crate::runtime::BaselineRuntime>>,
    server_id: u32,
) -> bool {
    let clid = session.client_id;

    // 1. setupstream: Client starts a new video or screen-share stream
    if payload_str.starts_with("setupstream") {
        let rc = get_ts6_return_code(payload_str);
        let name = parse_ts6_argument(payload_str, "name").unwrap_or_else(|| String::from("Stream"));
        let stream_type = parse_ts6_argument_u32(payload_str, "type").unwrap_or(0);
        let bitrate = parse_ts6_argument_u64(payload_str, "bitrate").unwrap_or(2_500_000);
        let accessibility = parse_ts6_argument_u32(payload_str, "accessibility")
            .or_else(|| parse_ts6_argument_u32(payload_str, "access"))
            .unwrap_or(0);
        // TS6 supports mode=1 (P2P WebRTC) and mode=2 (SFU). Since we advertise STREAM_P2P,
        // default to mode=1 (P2P) so clients establish direct WebRTC media via `streamsignaling`.
        let requested_mode = parse_ts6_argument_u32(payload_str, "mode").unwrap_or(1);
        let mode = if requested_mode == 0 { 1 } else { requested_mode };
        let viewer_limit = parse_ts6_argument_u32(payload_str, "viewer_limit").unwrap_or(10);
        let audio = parse_ts6_argument_u32(payload_str, "audio").unwrap_or(1);

        let channel_id = {
            let rt = runtime.lock().unwrap();
            rt.store
                .online_clients
                .get(&clid)
                .map(|c| c.channel_id)
                .unwrap_or(1)
        };

        let seq = NEXT_TS6_STREAM_SEQ.fetch_add(1, Ordering::SeqCst);
        let stream_id = format!("ts6-stream-{}-{}", clid, seq);

        let entry = Ts6StreamEntry {
            id: stream_id.clone(),
            server_id,
            channel_id,
            clid,
            name,
            stream_type,
            bitrate,
            accessibility,
            mode,
            viewer_limit,
            audio,
            sfu_endpoint: String::new(),
            sfu_user_id: String::new(),
            attendees: BTreeSet::new(),
        };

        {
            let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
            reg.streams.insert(stream_id.clone(), entry.clone());
        }

        // Send notifystreamstarted to initiator (with return_code if present)
        let self_notify = entry.render_notifystreamstarted(rc.as_deref());
        send_ts6_command(&self_notify, session, addr, socket, iv_struct).await;

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }

        // Broadcast notifystreamstarted to all other TS6 clients on the server
        let peer_notify = entry.render_notifystreamstarted(None);
        {
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: None,
                    exclude_client_id: Some(clid),
                    command: peer_notify,
                },
            );
        }

        // Update runtime `client_is_streaming` and `active_broadcasts`
        sync_runtime_stream_state(runtime, server_id, clid);
        return true;
    }

    // 2. updatestream: Streamer updates stream metadata/parameters
    if payload_str.starts_with("updatestream") {
        let rc = get_ts6_return_code(payload_str);
        let stream_id = parse_ts6_argument(payload_str, "id").unwrap_or_default();

        let updated_entry = {
            let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
            let target = if !stream_id.is_empty() {
                reg.streams.get_mut(&stream_id)
            } else {
                reg.streams
                    .values_mut()
                    .find(|s| s.server_id == server_id && s.clid == clid)
            };
            if let Some(entry) = target {
                if let Some(name) = parse_ts6_argument(payload_str, "name") {
                    entry.name = name;
                }
                if let Some(st) = parse_ts6_argument_u32(payload_str, "type") {
                    entry.stream_type = st;
                }
                if let Some(br) = parse_ts6_argument_u64(payload_str, "bitrate") {
                    entry.bitrate = br;
                }
                if let Some(acc) = parse_ts6_argument_u32(payload_str, "accessibility")
                    .or_else(|| parse_ts6_argument_u32(payload_str, "access"))
                {
                    entry.accessibility = acc;
                }
                if let Some(m) = parse_ts6_argument_u32(payload_str, "mode") {
                    entry.mode = m;
                }
                if let Some(vl) = parse_ts6_argument_u32(payload_str, "viewer_limit") {
                    entry.viewer_limit = vl;
                }
                if let Some(au) = parse_ts6_argument_u32(payload_str, "audio") {
                    entry.audio = au;
                }
                Some(entry.clone())
            } else {
                None
            }
        };

        if let Some(entry) = updated_entry {
            let self_notify = entry.render_notifystreamupdated(rc.as_deref());
            send_ts6_command(&self_notify, session, addr, socket, iv_struct).await;

            if let Some(rc_val) = &rc {
                let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
                send_ts6_command(&ack, session, addr, socket, iv_struct).await;
            }

            let peer_notify = entry.render_notifystreamupdated(None);
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: None,
                    exclude_client_id: Some(clid),
                    command: peer_notify,
                },
            );
        } else if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 3. stopstream: Streamer stops an active stream
    if payload_str.starts_with("stopstream") {
        let rc = get_ts6_return_code(payload_str);
        let stream_id = parse_ts6_argument(payload_str, "id").unwrap_or_default();
        let reason = parse_ts6_argument_u32(payload_str, "reason").unwrap_or(0);

        let removed_entries: Vec<Ts6StreamEntry> = {
            let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
            if !stream_id.is_empty() {
                reg.streams.remove(&stream_id).into_iter().collect()
            } else {
                let ids: Vec<String> = reg
                    .streams
                    .values()
                    .filter(|s| s.server_id == server_id && s.clid == clid)
                    .map(|s| s.id.clone())
                    .collect();
                ids.into_iter()
                    .filter_map(|id| reg.streams.remove(&id))
                    .collect()
            }
        };

        for entry in &removed_entries {
            let mut self_cmd = format!(
                "notifystreamstopped clid={} id={} reason={}",
                entry.clid,
                ts6_escape(&entry.id),
                reason
            );
            if let Some(rc_val) = &rc {
                self_cmd.push_str(&format!(" return_code={}", ts6_escape(rc_val)));
            }
            send_ts6_command(&self_cmd, session, addr, socket, iv_struct).await;

            let peer_cmd = format!(
                "notifystreamstopped clid={} id={} reason={}",
                entry.clid,
                ts6_escape(&entry.id),
                reason
            );
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: None,
                    exclude_client_id: Some(clid),
                    command: peer_cmd,
                },
            );
        }

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }

        sync_runtime_stream_state(runtime, server_id, clid);
        return true;
    }

    // 4. requeststreaminfo: Client queries stream metadata & attendees for a streamer (or server)
    if payload_str.starts_with("requeststreaminfo") {
        let rc = get_ts6_return_code(payload_str);
        let target_clid = parse_ts6_argument_u64(payload_str, "clid").unwrap_or(0);

        let matching_streams: Vec<Ts6StreamEntry> = {
            let reg = TS6_STREAM_REGISTRY.lock().unwrap();
            reg.streams
                .values()
                .filter(|s| {
                    s.server_id == server_id && (target_clid == 0 || s.clid == target_clid)
                })
                .cloned()
                .collect()
        };

        if !matching_streams.is_empty() {
            let mut rows = Vec::with_capacity(matching_streams.len());
            for (idx, stream) in matching_streams.iter().enumerate() {
                rows.push(stream.render_notifystreaminfo_row(if idx == 0 {
                    rc.as_deref()
                } else {
                    None
                }));
            }
            let info_cmd = format!("notifystreaminfo {}", rows.join("|"));
            send_ts6_command(&info_cmd, session, addr, socket, iv_struct).await;

            for stream in &matching_streams {
                if !stream.attendees.is_empty() {
                    let attendee_rows: Vec<String> = stream
                        .attendees
                        .iter()
                        .map(|a_clid| format!("id={} clid={}", ts6_escape(&stream.id), a_clid))
                        .collect();
                    let attendees_cmd = format!("notifystreamattendees {}", attendee_rows.join("|"));
                    send_ts6_command(&attendees_cmd, session, addr, socket, iv_struct).await;
                }
            }
        }

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 5. joinstreamrequest: Viewer requests to join (or cancel joining) a streamer's P2P stream
    if payload_str.starts_with("joinstreamrequest") {
        let rc = get_ts6_return_code(payload_str);
        let stream_id = parse_ts6_argument(payload_str, "id").unwrap_or_default();
        let target_streamer_clid = parse_ts6_argument_u64(payload_str, "clid").unwrap_or_else(|| {
            let reg = TS6_STREAM_REGISTRY.lock().unwrap();
            reg.streams.get(&stream_id).map(|s| s.clid).unwrap_or(0)
        });
        let msg = parse_ts6_argument(payload_str, "msg").unwrap_or_default();
        let is_remove = parse_ts6_argument_u32(payload_str, "is_remove").unwrap_or(0);

        if is_remove != 0 {
            let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
            if let Some(stream) = reg.streams.get_mut(&stream_id) {
                stream.attendees.remove(&clid);
            }
        }

        if target_streamer_clid != 0 {
            let forward_cmd = format!(
                "notifyjoinstreamrequest clid={} id={} msg={} is_remove={}",
                clid,
                ts6_escape(&stream_id),
                ts6_escape(&msg),
                is_remove
            );
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: Some(target_streamer_clid),
                    exclude_client_id: None,
                    command: forward_cmd,
                },
            );
        }

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 6. respondjoinstreamrequest: Streamer accepts/rejects viewer's join request and sends WebRTC SDP offer
    if payload_str.starts_with("respondjoinstreamrequest") {
        let rc = get_ts6_return_code(payload_str);
        let stream_id = parse_ts6_argument(payload_str, "id").unwrap_or_default();
        let viewer_clid = parse_ts6_argument_u64(payload_str, "clid").unwrap_or(0);
        let msg = parse_ts6_argument(payload_str, "msg").unwrap_or_default();
        let offer = parse_ts6_argument(payload_str, "offer").unwrap_or_default();
        let decision = parse_ts6_argument_u32(payload_str, "decision").unwrap_or(1);

        if decision != 0 && viewer_clid != 0 {
            let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
            if let Some(stream) = reg.streams.get_mut(&stream_id) {
                stream.attendees.insert(viewer_clid);
            }
        }

        if viewer_clid != 0 {
            let respond_cmd = format!(
                "notifyrespondjoinstreamrequest clid={} id={} msg={} offer={} decision={}",
                clid,
                ts6_escape(&stream_id),
                ts6_escape(&msg),
                ts6_escape(&offer),
                decision
            );
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: Some(viewer_clid),
                    exclude_client_id: None,
                    command: respond_cmd,
                },
            );

            if decision != 0 {
                let joined_cmd = format!(
                    "notifystreamclientjoined clid={} id={}",
                    viewer_clid,
                    ts6_escape(&stream_id)
                );
                send_ts6_command(&joined_cmd, session, addr, socket, iv_struct).await;
                rt.broadcast_event(
                    server_id,
                    &crate::transport::TransportNotification::Ts6StreamNotify {
                        server_id,
                        target_client_id: None,
                        exclude_client_id: Some(clid),
                        command: joined_cmd,
                    },
                );
            }
        }

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 7. streamsignaling: Exchange WebRTC SDP answers & ICE candidates (`json=`) between streamer and viewer
    if payload_str.starts_with("streamsignaling") {
        let rc = get_ts6_return_code(payload_str);
        let stream_id = parse_ts6_argument(payload_str, "id").unwrap_or_default();
        let target_clid = parse_ts6_argument_u64(payload_str, "clid").unwrap_or(0);
        let json_payload = parse_ts6_argument(payload_str, "json").unwrap_or_default();

        if target_clid != 0 {
            let sig_cmd = format!(
                "notifystreamsignaling clid={} id={} json={}",
                clid,
                ts6_escape(&stream_id),
                ts6_escape(&json_payload)
            );
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: Some(target_clid),
                    exclude_client_id: None,
                    command: sig_cmd,
                },
            );
        }

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 8. removeclientfromstream: Viewer leaves stream (`id=... reason=...`) OR streamer kicks viewer (`id=... clid=... reason=...`)
    if payload_str.starts_with("removeclientfromstream") {
        let rc = get_ts6_return_code(payload_str);
        let stream_id = parse_ts6_argument(payload_str, "id").unwrap_or_default();
        let target_clid = parse_ts6_argument_u64(payload_str, "clid")
            .filter(|&id| id != 0)
            .unwrap_or(clid);
        let reason = parse_ts6_argument_u32(payload_str, "reason").unwrap_or(0);

        {
            let mut reg = TS6_STREAM_REGISTRY.lock().unwrap();
            if let Some(stream) = reg.streams.get_mut(&stream_id) {
                stream.attendees.remove(&target_clid);
            }
        }

        let left_cmd = format!(
            "notifystreamclientleft clid={} id={} reason={}",
            target_clid,
            ts6_escape(&stream_id),
            reason
        );
        send_ts6_command(&left_cmd, session, addr, socket, iv_struct).await;

        {
            let rt = runtime.lock().unwrap();
            rt.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::Ts6StreamNotify {
                    server_id,
                    target_client_id: None,
                    exclude_client_id: Some(clid),
                    command: left_cmd,
                },
            );
        }

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 9. requestsfuaccessinfo: Client requests SFU credentials
    if payload_str.starts_with("requestsfuaccessinfo") {
        let rc = get_ts6_return_code(payload_str);
        let mut sfu_cmd = format!(
            "notifysfuaccessinfo sfu_token=bts-sfu-token-{} sfu_user=bts-user-{}",
            clid, clid
        );
        if let Some(rc_val) = &rc {
            sfu_cmd.push_str(&format!(" return_code={}", ts6_escape(rc_val)));
        }
        send_ts6_command(&sfu_cmd, session, addr, socket, iv_struct).await;

        if let Some(rc_val) = &rc {
            let ack = format!("error id=0 msg=ok return_code={}", ts6_escape(rc_val));
            send_ts6_command(&ack, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ts6_argument_distinguishes_id_from_clid() {
        let cmd = "joinstreamrequest clid=102 id=ts6-stream-102-1 msg=hello\\sworld is_remove=0 return_code=1:9";
        assert_eq!(parse_ts6_argument_u64(cmd, "clid"), Some(102));
        assert_eq!(
            parse_ts6_argument(cmd, "id").as_deref(),
            Some("ts6-stream-102-1")
        );
        assert_eq!(parse_ts6_argument(cmd, "msg").as_deref(), Some("hello world"));
        assert_eq!(parse_ts6_argument_u32(cmd, "is_remove"), Some(0));
        assert_eq!(get_ts6_return_code(cmd).as_deref(), Some("1:9"));
    }

    #[test]
    fn ts6_stream_notifications_render_both_access_and_accessibility_keys() {
        let mut attendees = BTreeSet::new();
        attendees.insert(105);
        let entry = Ts6StreamEntry {
            id: String::from("ts6-stream-101-1"),
            server_id: 1,
            channel_id: 1,
            clid: 101,
            name: String::from("Desktop Share"),
            stream_type: 0,
            bitrate: 4_000_000,
            accessibility: 0,
            mode: 1,
            viewer_limit: 10,
            audio: 1,
            sfu_endpoint: String::new(),
            sfu_user_id: String::new(),
            attendees,
        };

        let started = entry.render_notifystreamstarted(Some("1:42"));
        assert!(started.starts_with("notifystreamstarted "));
        assert!(started.contains("clid=101"));
        assert!(started.contains("id=ts6-stream-101-1"));
        assert!(started.contains("name=Desktop\\sShare"));
        assert!(started.contains("access=0"));
        assert!(started.contains("accessibility=0"));
        assert!(started.contains("mode=1"));
        assert!(started.contains("return_code=1:42"));

        let info_row = entry.render_notifystreaminfo_row(Some("1:43"));
        assert!(info_row.contains("viewer=1"));
        assert!(info_row.contains("accessibility=0"));
        assert!(info_row.contains("access=0"));
        assert!(info_row.contains("return_code=1:43"));
    }
}

