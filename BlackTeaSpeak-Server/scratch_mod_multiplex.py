import re

# 1. Modify src/runtime.rs
filepath_runtime = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\runtime.rs"
with open(filepath_runtime, "r", encoding="utf-8") as f:
    content_rt = f.read()

content_rt = content_rt.replace(
    "pub webtransport_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, Vec<u8>)>>,",
    "pub webtransport_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, u8, Vec<u8>)>>,"
)
content_rt = content_rt.replace(
    "pub desktop_btea_media_tx: Option<tokio::sync::broadcast::Sender<(u32, u64, Vec<u8>)>>,",
    "pub desktop_btea_media_tx: Option<tokio::sync::broadcast::Sender<(u32, u64, u8, Vec<u8>)>>,"
)

old_route_desktop = """    pub fn route_btea_media_to_desktop(&self, server_id: u32, sender_client_id: u64, payload: &[u8]) {
        if let Some(tx) = &self.desktop_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, payload.to_vec()));
        }
    }"""
new_route_desktop = """    pub fn route_btea_media_to_desktop(&self, server_id: u32, sender_client_id: u64, packet_type: u8, payload: &[u8]) {
        if let Some(tx) = &self.desktop_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, packet_type, payload.to_vec()));
        }
    }"""
content_rt = content_rt.replace(old_route_desktop, new_route_desktop)

old_route_web = """    pub fn route_btea_media_to_webtransport(&self, server_id: u32, sender_client_id: u64, payload: &[u8]) {
        if let Some(tx) = &self.webtransport_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, payload.to_vec()));
        }
    }"""
new_route_web = """    pub fn route_btea_media_to_webtransport(&self, server_id: u32, sender_client_id: u64, packet_type: u8, payload: &[u8]) {
        if let Some(tx) = &self.webtransport_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, packet_type, payload.to_vec()));
        }
    }"""
content_rt = content_rt.replace(old_route_web, new_route_web)

with open(filepath_runtime, "w", encoding="utf-8") as f:
    f.write(content_rt)


# 2. Modify src/desktop_transport.rs
filepath_desktop = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\desktop_transport.rs"
with open(filepath_desktop, "r", encoding="utf-8") as f:
    content_desktop = f.read()

content_desktop = content_desktop.replace(
    "media_rx: tokio::sync::broadcast::Receiver<(u32, u64, Vec<u8>)>",
    "media_rx: tokio::sync::broadcast::Receiver<(u32, u64, u8, Vec<u8>)>"
)

old_run_loop_match = """                tokio::select! {
                    Ok((recv_server_id, sender_client_id, payload)) = media_rx.recv() => {
                        if recv_server_id == self.server_id {
                            Self::broadcast_media(
                                &socket,
                                &mut addr_to_session,
                                &self.shared_secrets,
                                &self.runtime,
                                self.server_id,
                                sender_client_id,
                                "0.0.0.0:0".parse().unwrap(), // Web clients don't have a UDP socket sender address
                                payload,
                                0x0B, // WebTransport routes standard voice
                            ).await;
                        }
                    }"""
new_run_loop_match = """                tokio::select! {
                    Ok((recv_server_id, sender_client_id, packet_type, payload)) = media_rx.recv() => {
                        if recv_server_id == self.server_id {
                            Self::broadcast_media(
                                &socket,
                                &mut addr_to_session,
                                &self.shared_secrets,
                                &self.runtime,
                                self.server_id,
                                sender_client_id,
                                "0.0.0.0:0".parse().unwrap(), // Web clients don't have a UDP socket sender address
                                payload,
                                packet_type,
                            ).await;
                        }
                    }"""
content_desktop = content_desktop.replace(old_run_loop_match, new_run_loop_match)

old_route_call = """        if packet_type == 0x0B {
            let rt_guard = runtime.lock().unwrap();
            rt_guard.route_btea_media_to_webtransport(server_id, sender_client_id, &payload);
        }"""
new_route_call = """        if packet_type == 0x0A || packet_type == 0x0B {
            let rt_guard = runtime.lock().unwrap();
            rt_guard.route_btea_media_to_webtransport(server_id, sender_client_id, packet_type, &payload);
        }"""
content_desktop = content_desktop.replace(old_route_call, new_route_call)

with open(filepath_desktop, "w", encoding="utf-8") as f:
    f.write(content_desktop)


# 3. Modify src/web_transport.rs
filepath_wt = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath_wt, "r", encoding="utf-8") as f:
    content_wt = f.read()

content_wt = content_wt.replace(
    "media_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(u32, u64, Vec<u8>)>>",
    "media_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(u32, u64, u8, Vec<u8>)>>"
)

old_broadcast = """            tokio::spawn(async move {
                while let Some((server_id, sender_client_id, payload)) = media_rx.recv().await {
                    let sender_channel_id = {"""
new_broadcast = """            tokio::spawn(async move {
                while let Some((server_id, sender_client_id, packet_type, payload)) = media_rx.recv().await {
                    let sender_channel_id = {"""
content_wt = content_wt.replace(old_broadcast, new_broadcast)

old_send = """                    for conn in targets {
                        let _ = conn.send_datagram(payload.clone());
                    }"""
new_send = """                    let mut wt_payload = Vec::with_capacity(1 + payload.len());
                    wt_payload.push(packet_type);
                    wt_payload.extend_from_slice(&payload);
                    for conn in targets {
                        let _ = conn.send_datagram(wt_payload.clone());
                    }"""
content_wt = content_wt.replace(old_send, new_send)

old_rx = """            match datagram_session.receive_datagram().await {
                Ok(datagram) => {
                    let payload = datagram.payload().to_vec();
                    let client_id = crate::web_transport::WEB_CLIENT_ID_BASE + connection_id;
                    let mut server_id = None;
                    if let Ok(lock) = datagram_sessions.lock() {
                        if let Some(sess) = lock.get(&client_id) {
                            server_id = Some(sess.presence.server_id);
                        }
                    }
                    if let Some(sid) = server_id {
                        let rt = datagram_runtime.lock().unwrap();
                        rt.route_btea_media_to_webtransport(sid, client_id, &payload);
                        rt.route_btea_media_to_desktop(sid, client_id, &payload);
                    }
                }"""
new_rx = """            match datagram_session.receive_datagram().await {
                Ok(datagram) => {
                    let raw = datagram.payload();
                    if raw.is_empty() { continue; }
                    let packet_type = raw[0];
                    let payload = &raw[1..];
                    
                    let client_id = crate::web_transport::WEB_CLIENT_ID_BASE + connection_id;
                    let mut server_id = None;
                    if let Ok(lock) = datagram_sessions.lock() {
                        if let Some(sess) = lock.get(&client_id) {
                            server_id = Some(sess.presence.server_id);
                        }
                    }
                    if let Some(sid) = server_id {
                        let rt = datagram_runtime.lock().unwrap();
                        rt.route_btea_media_to_webtransport(sid, client_id, packet_type, payload);
                        rt.route_btea_media_to_desktop(sid, client_id, packet_type, payload);
                    }
                }"""
content_wt = content_wt.replace(old_rx, new_rx)

with open(filepath_wt, "w", encoding="utf-8") as f:
    f.write(content_wt)

print("done")
