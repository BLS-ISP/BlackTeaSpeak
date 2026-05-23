import re

filepath_runtime = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\runtime.rs"
with open(filepath_runtime, "r", encoding="utf-8") as f:
    content = f.read()

content = content.replace(
    "pub webtransport_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, Vec<u8>)>>,",
    "pub webtransport_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, Vec<u8>)>>,\n    pub desktop_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, Vec<u8>)>>,"
)
content = content.replace(
    "webtransport_btea_media_tx: None,",
    "webtransport_btea_media_tx: None,\n        desktop_btea_media_tx: None,"
)

add_methods = """
    pub fn route_btea_media_to_desktop(&self, server_id: u32, sender_client_id: u64, payload: &[u8]) {
        if let Some(tx) = &self.desktop_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, payload.to_vec()));
        }
    }
"""

content = content.replace(
    "pub fn route_btea_media_to_webtransport",
    add_methods + "\n    pub fn route_btea_media_to_webtransport"
)

with open(filepath_runtime, "w", encoding="utf-8") as f:
    f.write(content)

filepath_webtransport = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath_webtransport, "r", encoding="utf-8") as f:
    content_wt = f.read()

# Add datagram receiving loop
receive_loop = """
    let datagram_session = wtransport_session.clone();
    let datagram_runtime = Arc::clone(&runtime);
    let datagram_sessions = Arc::clone(&sessions);
    tokio::spawn(async move {
        loop {
            match datagram_session.receive_datagram().await {
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
                }
                Err(_) => break,
            }
        }
    });
"""

content_wt = content_wt.replace(
    "let (mut send, recv) = wtransport_session.accept_bi().await?;",
    receive_loop + "\n    let (mut send, recv) = wtransport_session.accept_bi().await?;"
)

with open(filepath_webtransport, "w", encoding="utf-8") as f:
    f.write(content_wt)
    
print("done")
