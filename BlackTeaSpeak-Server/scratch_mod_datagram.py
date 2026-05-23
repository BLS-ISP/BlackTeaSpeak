import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# 1. Add media_rx to struct BlackTeaWebTransportServer
content = re.sub(
    r"struct BlackTeaWebTransportServer \{\n\s*bind_addr: SocketAddr,\n\s*cert_path: PathBuf,\n\s*key_path: PathBuf,\n\s*runtime: Arc<Mutex<BaselineRuntime>>,\n\s*file_transfers: Arc<FileTransferRegistry>,\n\s*next_connection_id: Arc<AtomicU64>,\n\s*shutdown: Arc<AtomicBool>,\n\s*sessions: SharedBlackTeaWebSessions,\n\s*query_bridge: Option<QueryNotificationBridge>,\n\}",
    r"struct BlackTeaWebTransportServer {\n    bind_addr: SocketAddr,\n    cert_path: PathBuf,\n    key_path: PathBuf,\n    runtime: Arc<Mutex<BaselineRuntime>>,\n    file_transfers: Arc<FileTransferRegistry>,\n    next_connection_id: Arc<AtomicU64>,\n    shutdown: Arc<AtomicBool>,\n    sessions: SharedBlackTeaWebSessions,\n    query_bridge: Option<QueryNotificationBridge>,\n    media_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(u32, u64, Vec<u8>)>>,\n}",
    content,
    count=1
)

# 2. Modify bind_with_shared_runtime to initialize media channel
old_bind = """        let sessions_for_bus = sessions.clone();
        runtime.lock().unwrap().subscribe_events(Box::new(move |rt, _server_id, notif| {
            let frames = match visibility_aware_transport_broadcasts(
                &sessions_for_bus,
                rt,
                None,
                std::slice::from_ref(notif),
            ) {
                Ok(f) => f,
                Err(_) => return,
            };
            let _ = broadcast_queued_frames(&sessions_for_bus, &frames);
        }));

        Ok(Self {
            bind_addr: addr,
            cert_path: certificate_path.as_ref().to_path_buf(),
            key_path: private_key_path.as_ref().to_path_buf(),
            runtime,
            file_transfers,
            next_connection_id: Arc::new(AtomicU64::new(1)),
            shutdown: Arc::new(AtomicBool::new(false)),
            sessions,
            query_bridge: None,
        })"""

new_bind = """        let sessions_for_bus = sessions.clone();
        
        let (media_tx, media_rx) = tokio::sync::mpsc::unbounded_channel();
        {
            let mut rt_lock = runtime.lock().unwrap();
            rt_lock.webtransport_btea_media_tx = Some(media_tx);
            
            rt_lock.subscribe_events(Box::new(move |rt, _server_id, notif| {
                let frames = match visibility_aware_transport_broadcasts(
                    &sessions_for_bus,
                    rt,
                    None,
                    std::slice::from_ref(notif),
                ) {
                    Ok(f) => f,
                    Err(_) => return,
                };
                let _ = broadcast_queued_frames(&sessions_for_bus, &frames);
            }));
        }

        Ok(Self {
            bind_addr: addr,
            cert_path: certificate_path.as_ref().to_path_buf(),
            key_path: private_key_path.as_ref().to_path_buf(),
            runtime,
            file_transfers,
            next_connection_id: Arc::new(AtomicU64::new(1)),
            shutdown: Arc::new(AtomicBool::new(false)),
            sessions,
            query_bridge: None,
            media_rx: Some(media_rx),
        })"""

content = content.replace(old_bind, new_bind)

# 3. Spawn media broadcast loop in run_async
old_run = """    async fn run_async(self) -> Result<()> {
        let identity = Identity::load_pemfiles(&self.cert_path, &self.key_path).await?;
        let config = WTransportServerConfig::builder()
            .with_bind_default(self.bind_addr.port())
            .with_identity(identity)
            .build();
            
        let endpoint = Endpoint::server(config)?;"""

new_run = """    async fn run_async(mut self) -> Result<()> {
        let identity = Identity::load_pemfiles(&self.cert_path, &self.key_path).await?;
        let config = WTransportServerConfig::builder()
            .with_bind_default(self.bind_addr.port())
            .with_identity(identity)
            .build();
            
        let endpoint = Endpoint::server(config)?;
        
        if let Some(mut media_rx) = self.media_rx.take() {
            let sessions_for_media = Arc::clone(&self.sessions);
            let runtime_for_media = Arc::clone(&self.runtime);
            tokio::spawn(async move {
                while let Some((server_id, sender_client_id, payload)) = media_rx.recv().await {
                    let sender_channel_id = {
                        let rt = runtime_for_media.lock().unwrap();
                        rt.online_client_snapshot(server_id, sender_client_id).map(|c| c.channel_id).unwrap_or(0)
                    };
                    
                    let mut targets = Vec::new();
                    {
                        if let Ok(lock) = sessions_for_media.lock() {
                            for session in lock.values() {
                                if session.presence.server_id == server_id && session.presence.channel_id == sender_channel_id {
                                    if session.presence.client_id != sender_client_id {
                                        if let Some(conn) = &session.wtransport_session {
                                            targets.push(conn.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    for conn in targets {
                        let _ = conn.send_datagram(payload.clone());
                    }
                }
            });
        }"""

content = content.replace(old_run, new_run)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
