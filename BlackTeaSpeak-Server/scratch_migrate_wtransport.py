import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Add wtransport import
content = re.sub(r"use tungstenite::\{Message, accept\};", "use tungstenite::{Message, accept};\nuse wtransport::{Endpoint, ServerConfig, Identity};\nuse std::path::PathBuf;", content)

# Change struct definition
struct_old = r"""pub struct BlackTeaWebTransportServer \{
    listener: TcpListener,
    runtime: Arc<Mutex<BaselineRuntime>>,
    tls_config: Arc<ServerConfig>,
    file_transfers: Arc<FileTransferRegistry>,
    next_connection_id: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    sessions: SharedBlackTeaWebSessions,
    query_bridge: Option<QueryNotificationBridge>,
\}"""

struct_new = """pub struct BlackTeaWebTransportServer {
    bind_addr: SocketAddr,
    cert_path: PathBuf,
    key_path: PathBuf,
    runtime: Arc<Mutex<BaselineRuntime>>,
    file_transfers: Arc<FileTransferRegistry>,
    next_connection_id: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    sessions: SharedBlackTeaWebSessions,
    query_bridge: Option<QueryNotificationBridge>,
}"""

content = re.sub(struct_old, struct_new, content)

# Change bind_with_shared_runtime
bind_old = r"""    pub fn bind_with_shared_runtime\(
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
        file_transfers: Arc<FileTransferRegistry>,
    \) -> Result<Self> \{
        let listener = TcpListener::bind\(bind_addr\)
            \.with_context\(\|\| format!\("failed to bind BlackTeaWeb transport to \{bind_addr\}"\)\)\?;
        listener
            \.set_nonblocking\(true\)
            \.context\("failed to switch BlackTeaWeb listener to nonblocking mode"\)\?;

        let sessions = Arc::new\(Mutex::new\(HashMap::new\(\)\)\);
        install_file_transfer_notifier\(&file_transfers, &sessions\);

        let sessions_for_bus = sessions\.clone\(\);
        runtime\.lock\(\)\.unwrap\(\)\.subscribe_events\(Box::new\(move \|rt, _server_id, notif\| \{
            let frames = match visibility_aware_transport_broadcasts\(
                &sessions_for_bus,
                rt,
                None,
                std::slice::from_ref\(notif\),
            \) \{
                Ok\(f\) => f,
                Err\(_\) => return,
            \};
            let _ = broadcast_queued_frames\(&sessions_for_bus, &frames\);
        \}\)\);

        Ok\(Self \{
            listener,
            runtime,
            tls_config: load_tls_config\(certificate_path\.as_ref\(\), private_key_path\.as_ref\(\)\)\?,
            file_transfers,
            next_connection_id: Arc::new\(AtomicU64::new\(1\)\),
            shutdown: Arc::new\(AtomicBool::new\(false\)\),
            sessions,
            query_bridge: None,
        \}\)
    \}"""

bind_new = """    pub fn bind_with_shared_runtime(
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
        file_transfers: Arc<FileTransferRegistry>,
    ) -> Result<Self> {
        let addr: SocketAddr = bind_addr.parse()?;
        
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        install_file_transfer_notifier(&file_transfers, &sessions);

        let sessions_for_bus = sessions.clone();
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
        })
    }"""

content = re.sub(bind_old, bind_new, content)

# Change local_addr
local_addr_old = r"""    pub fn local_addr\(&self\) -> Result<SocketAddr> \{
        self\.listener
            \.local_addr\(\)
            \.context\("failed to read BlackTeaWeb listener address"\)
    \}"""
    
local_addr_new = """    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.bind_addr)
    }"""
    
content = re.sub(local_addr_old, local_addr_new, content)

# Change run
run_old = r"""    pub fn run\(self\) -> Result<\(\)> \{
        while !self\.shutdown\.load\(Ordering::SeqCst\) \{
            match self\.listener\.accept\(\) \{
                Ok\(\(stream, _\)\) => \{
                    let runtime = Arc::clone\(&self\.runtime\);
                    let tls_config = Arc::clone\(&self\.tls_config\);
                    let file_transfers = Arc::clone\(&self\.file_transfers\);
                    let sessions = Arc::clone\(&self\.sessions\);
                    let query_bridge = self\.query_bridge\.clone\(\);
                    let connection_id = self\.next_connection_id\.fetch_add\(1, Ordering::SeqCst\);
                    thread::spawn\(move \|\| \{
                        if let Err\(error\) = handle_client\(
                            stream,
                            runtime,
                            tls_config,
                            file_transfers,
                            sessions,
                            query_bridge,
                            connection_id,
                        \) \{
                            eprintln!\("BlackTeaWeb transport client error: \{error:#\}"\);
                        \}
                    \}\);
                \}
                Err\(error\) if error\.kind\(\) == io::ErrorKind::WouldBlock => \{
                    thread::sleep\(Duration::from_millis\(25\)\);
                \}
                Err\(error\) => \{
                    eprintln!\("BlackTeaWeb transport listener error: \{error:#\}"\);
                \}
            \}
        \}
        Ok\(\(\)\)
    \}"""
    
run_new = """    pub fn run(self) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async { self.run_async().await })
    }

    async fn run_async(self) -> Result<()> {
        let identity = Identity::load_pemfiles(&self.cert_path, &self.key_path).await?;
        let config = ServerConfig::builder()
            .with_bind_address(self.bind_addr)
            .with_identity(identity)
            .build();
            
        let endpoint = Endpoint::server(config)?;
        
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                break;
            }
            
            let incoming_session = endpoint.accept().await;
            
            let runtime = Arc::clone(&self.runtime);
            let file_transfers = Arc::clone(&self.file_transfers);
            let sessions = Arc::clone(&self.sessions);
            let query_bridge = self.query_bridge.clone();
            let connection_id = self.next_connection_id.fetch_add(1, Ordering::SeqCst);
            
            tokio::spawn(async move {
                if let Err(error) = handle_wtransport_client(
                    incoming_session,
                    runtime,
                    file_transfers,
                    sessions,
                    query_bridge,
                    connection_id,
                ).await {
                    eprintln!("WebTransport client error: {error:#}");
                }
            });
        }
        
        Ok(())
    }"""
    
content = re.sub(run_old, run_new, content)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
