import re

# 1. Modify runtime.rs
filepath_runtime = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\runtime.rs"
with open(filepath_runtime, "r", encoding="utf-8") as f:
    content_rt = f.read()

content_rt = content_rt.replace(
    "pub desktop_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, Vec<u8>)>>",
    "pub desktop_btea_media_tx: Option<tokio::sync::broadcast::Sender<(u32, u64, Vec<u8>)>>"
)

# Initialize in create_baseline_runtime_with_state_path
init_tx = """    let (desktop_tx, _) = tokio::sync::broadcast::channel(1024);
    let mut runtime = BaselineRuntime {"""

content_rt = content_rt.replace(
    "    let mut runtime = BaselineRuntime {",
    init_tx
)

content_rt = content_rt.replace(
    "desktop_btea_media_tx: None,",
    "desktop_btea_media_tx: Some(desktop_tx),"
)

with open(filepath_runtime, "w", encoding="utf-8") as f:
    f.write(content_rt)


# 2. Modify desktop_transport.rs
filepath_desktop = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\desktop_transport.rs"
with open(filepath_desktop, "r", encoding="utf-8") as f:
    content_desktop = f.read()
    
# Add rx channel to DesktopTransportServer
content_desktop = content_desktop.replace(
    "shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,",
    "shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,\n    media_rx: tokio::sync::broadcast::Receiver<(u32, u64, Vec<u8>)>,"
)

# Add init logic in bind_with_shared_runtime
old_bind = """    ) -> Result<Self> {
        Ok(Self {
            server_id,
            bind_addr: bind_addr.to_string(),
            runtime,
            shared_secrets,
        })
    }"""
new_bind = """    ) -> Result<Self> {
        let media_rx = runtime.lock().unwrap().desktop_btea_media_tx.as_ref().unwrap().subscribe();
        Ok(Self {
            server_id,
            bind_addr: bind_addr.to_string(),
            runtime,
            shared_secrets,
            media_rx,
        })
    }"""

content_desktop = content_desktop.replace(old_bind, new_bind)

# Modify run loop to use media_rx
old_run_loop = """            loop {
                if should_stop.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
                    result = socket.recv_from(&mut buf) => {"""

new_run_loop = """            let mut media_rx = self.media_rx;
            loop {
                if should_stop.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
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
                    }
                    result = socket.recv_from(&mut buf) => {"""

content_desktop = content_desktop.replace(old_run_loop, new_run_loop)

with open(filepath_desktop, "w", encoding="utf-8") as f:
    f.write(content_desktop)

print("done")
