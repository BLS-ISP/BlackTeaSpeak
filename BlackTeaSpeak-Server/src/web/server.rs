use super::*;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncBufReadExt;

use anyhow::{Context, Result, anyhow};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use wtransport::{Endpoint, ServerConfig as WTransportServerConfig, Identity};

use crate::file_transfer::FileTransferRegistry;
use crate::runtime::{
    BaselineRuntime, create_baseline_runtime,
    create_baseline_runtime_with_state_path,
};

pub struct BlackTeaWebTransportServer {
    pub(crate) bind_addr: SocketAddr,
    pub(crate) cert_path: PathBuf,
    pub(crate) key_path: PathBuf,
    pub(crate) runtime: Arc<Mutex<BaselineRuntime>>,
    pub(crate) file_transfers: Arc<FileTransferRegistry>,
    pub(crate) next_connection_id: Arc<AtomicU64>,
    pub(crate) shutdown: Arc<AtomicBool>,
    pub(crate) sessions: SharedBlackTeaWebSessions,
    
    pub(crate) media_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(u32, u64, u8, Vec<u8>)>>,
}

impl BlackTeaWebTransportServer {
    pub fn bind(
        workspace_root: impl AsRef<Path>,
        bind_addr: &str,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let file_transfers = Arc::new(FileTransferRegistry::new(workspace_root.as_ref()));
        let runtime = create_baseline_runtime(workspace_root)?;
        Self::bind_with_shared_runtime(
            Arc::new(Mutex::new(runtime)),
            bind_addr,
            certificate_path,
            private_key_path,
            file_transfers,
        )
    }

    pub fn bind_with_state_path(
        workspace_root: impl AsRef<Path>,
        state_path: impl AsRef<Path>,
        bind_addr: &str,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let file_transfers = Arc::new(FileTransferRegistry::new(workspace_root.as_ref()));
        let runtime = create_baseline_runtime_with_state_path(workspace_root, state_path)?;
        Self::bind_with_shared_runtime(
            Arc::new(Mutex::new(runtime)),
            bind_addr,
            certificate_path,
            private_key_path,
            file_transfers,
        )
    }

    pub fn bind_with_shared_runtime(
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
            
            media_rx: Some(media_rx),
        })
    }

    pub fn notification_bridge(&self) -> BlackTeaWebNotificationBridge {
        BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&self.sessions),
        }
    }

    pub fn file_transfer_registry(&self) -> Arc<FileTransferRegistry> {
        Arc::clone(&self.file_transfers)
    }

    

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.bind_addr)
    }

    pub fn run(self) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async { self.run_async().await })
    }

    fn should_autorenew(cert_path: &Path, key_path: &Path) -> bool {
        if !cert_path.exists() || !key_path.exists() {
            return true;
        }
        if let Ok(metadata) = std::fs::metadata(cert_path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    return elapsed.as_secs() > 7 * 24 * 3600;
                }
            }
        }
        false
    }

    pub(crate) async fn run_async(mut self) -> Result<()> {
        if self.cert_path.file_name().and_then(|n| n.to_str()).map_or(false, |s| s.contains("blackteaweb-localhost-cert")) {
            if Self::should_autorenew(&self.cert_path, &self.key_path) {
                println!("Autorenewing expired or old localhost certificates...");
                if let Err(e) = generate_localhost_tls_assets(&self.cert_path, &self.key_path) {
                    eprintln!("Failed to autorenew certificates: {e:#}");
                }
            }
        }
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
                while let Some((server_id, sender_client_id, packet_type, payload)) = media_rx.recv().await {
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
                    
                    let mut wt_payload = Vec::with_capacity(1 + 8 + payload.len());
                    let wt_packet_type = if packet_type == 0x0A || packet_type == 0x00 || packet_type == 1 {
                        1
                    } else {
                        packet_type
                    };
                    wt_payload.push(wt_packet_type);
                    wt_payload.extend_from_slice(&sender_client_id.to_le_bytes());
                    wt_payload.extend_from_slice(&payload);
                    if wt_packet_type == 2 || wt_packet_type == 4 {
                        let shared_frame = Arc::new(wt_payload);
                        for conn in targets {
                            let frame_ref = Arc::clone(&shared_frame);
                            tokio::spawn(async move {
                                if let Ok(opening) = conn.open_uni().await {
                                    if let Ok(mut send_uni) = opening.await {
                                        let _ = send_uni.write_all(&frame_ref).await;
                                        let _ = send_uni.finish().await;
                                    }
                                }
                            });
                        }
                    } else {
                        for conn in targets {
                            let _ = conn.send_datagram(wt_payload.clone());
                        }
                    }
                }
            });
        }
        
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                break;
            }
            
            let incoming_session = endpoint.accept().await;
            
            let runtime = Arc::clone(&self.runtime);
            let file_transfers = Arc::clone(&self.file_transfers);
            let sessions = Arc::clone(&self.sessions);
            let connection_id = self.next_connection_id.fetch_add(1, Ordering::SeqCst);
            
            tokio::spawn(async move {
                if let Err(error) = handle_wtransport_client(
                    incoming_session,
                    runtime,
                    file_transfers,
                    sessions,
                    connection_id,
                ).await {
                    eprintln!("WebTransport client error: {error:#}");
                }
            });
        }
        
        Ok(())
    }
}

pub(crate) fn load_tls_config(certificate_path: &Path, private_key_path: &Path) -> Result<Arc<ServerConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    match load_tls_config_from_files(certificate_path, private_key_path) {
        Ok(config) => Ok(config),
        Err(error) => {
            eprintln!(
                "BlackTeaWeb TLS warning: failed to use configured certificate {} and key {}: {error:#}\nfalling back to an ephemeral localhost self-signed certificate",
                certificate_path.display(),
                private_key_path.display(),
            );
            generate_fallback_tls_config()
        }
    }
}

pub fn load_server_tls_config(
    certificate_path: impl AsRef<Path>,
    private_key_path: impl AsRef<Path>,
) -> Result<Arc<ServerConfig>> {
    load_tls_config(certificate_path.as_ref(), private_key_path.as_ref())
}

pub fn generate_localhost_tls_assets(
    certificate_path: impl AsRef<Path>,
    private_key_path: impl AsRef<Path>,
) -> Result<()> {
    let certificate_path = certificate_path.as_ref();
    let private_key_path = private_key_path.as_ref();

    if let Some(parent) = certificate_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create BlackTeaWeb certificate directory {}",
                parent.display()
            )
        })?;
    }
    if let Some(parent) = private_key_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create BlackTeaWeb private-key directory {}",
                parent.display()
            )
        })?;
    }

    let mut params = rcgen::CertificateParams::new(
        LOCALHOST_CERTIFICATE_NAMES
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
    )?;
    
    // WebTransport requires certificates to be valid for at most 14 days
    // when using serverCertificateHashes.
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(10);

    let key_pair = rcgen::KeyPair::generate().context("failed to generate key pair")?;
    let generated_cert = params.self_signed(&key_pair).context("failed to generate localhost BlackTeaWeb certificate")?;

    fs::write(certificate_path, generated_cert.pem()).with_context(|| {
        format!(
            "failed to write BlackTeaWeb certificate PEM {}",
            certificate_path.display()
        )
    })?;
    fs::write(private_key_path, key_pair.serialize_pem()).with_context(|| {
        format!(
            "failed to write BlackTeaWeb private key PEM {}",
            private_key_path.display()
        )
    })?;

    Ok(())
}

pub(crate) fn load_tls_config_from_files(
    certificate_path: &Path,
    private_key_path: &Path,
) -> Result<Arc<ServerConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut cert_reader = BufReader::new(File::open(certificate_path).with_context(|| {
        format!(
            "failed to open BlackTeaWeb certificate {}",
            certificate_path.display()
        )
    })?);
    let certificates = rustls_pemfile::certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to parse BlackTeaWeb certificate chain")?;

    let mut key_reader = BufReader::new(File::open(private_key_path).with_context(|| {
        format!(
            "failed to open BlackTeaWeb private key {}",
            private_key_path.display()
        )
    })?);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .context("failed to parse BlackTeaWeb private key")?
        .ok_or_else(|| anyhow!("BlackTeaWeb private key PEM did not contain a key"))?;

    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, clone_private_key(&private_key))
        .context("failed to build BlackTeaWeb TLS configuration")?;

    Ok(Arc::new(tls_config))
}

pub(crate) fn generate_fallback_tls_config() -> Result<Arc<ServerConfig>> {
    let mut params = rcgen::CertificateParams::new(
        LOCALHOST_CERTIFICATE_NAMES
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
    )?;
    
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(10);

    let key_pair = rcgen::KeyPair::generate().context("failed to generate key pair")?;
    let generated_cert = params.self_signed(&key_pair).context("failed to generate fallback BlackTeaWeb certificate")?;
    
    let certificate = generated_cert.der().clone();
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    
    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], private_key)
        .context("failed to build fallback BlackTeaWeb TLS configuration")?;

    Ok(Arc::new(tls_config))
}

pub(crate) fn clone_private_key(private_key: &PrivateKeyDer<'_>) -> PrivateKeyDer<'static> {
    private_key.clone_key()
}

async fn handle_wtransport_client(
    incoming: wtransport::endpoint::IncomingSession,
    runtime: Arc<Mutex<BaselineRuntime>>,
    file_transfers: Arc<FileTransferRegistry>,
    sessions: SharedBlackTeaWebSessions,
    connection_id: u64,
) -> Result<()> {
    let session_request = incoming.await?;
    let wtransport_session = session_request.accept().await?;
    let datagram_session = wtransport_session.clone();
    let datagram_runtime = Arc::clone(&runtime);
    let datagram_sessions = Arc::clone(&sessions);
    tokio::spawn(async move {
        loop {
            match datagram_session.receive_datagram().await {
                Ok(datagram) => {
                    let raw = datagram.payload();
                    if raw.is_empty() { continue; }
                    let packet_type = raw[0];
                    let payload = &raw[1..];
                    let client_id = crate::web::WEB_CLIENT_ID_BASE + connection_id;
                    let mut server_id = None;
                    if let Ok(lock) = datagram_sessions.lock() {
                        if let Some(sess) = lock.get(&client_id) {
                            server_id = Some(sess.presence.server_id);
                        }
                    }
                    if let Some(sid) = server_id {
                        let desktop_packet_type = if packet_type == 1 { 0x0A } else { packet_type };
                        let rtc_mgr_opt = {
                            let mut rt = datagram_runtime.lock().unwrap();
                            rt.mark_client_seen(client_id);
                            if packet_type == 1 || packet_type == 3 {
                                if let Some(c) = rt.online_client_snapshot(sid, client_id) {
                                    rt.broadcast_event(sid, &crate::transport::TransportNotification::TalkStatus {
                                        server_id: sid,
                                        channel_id: c.channel_id,
                                        client_id,
                                        is_talking: !payload.is_empty(),
                                        is_whisper: packet_type == 3 || c.whisper_targets.is_some(),
                                        whisper_targets: c.whisper_targets.clone(),
                                    });
                                }
                                rt.route_btea_media_to_webtransport(sid, client_id, packet_type, payload);
                                rt.route_btea_media_to_desktop(sid, client_id, desktop_packet_type, payload);
                                None
                            } else if !payload.is_empty() {
                                rt.route_btea_media_to_webtransport(sid, client_id, packet_type, payload);
                                if packet_type == 2 || packet_type == 4 {
                                    Some(Arc::clone(&rt.rtc_manager))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        };
                        if let Some(rtc_mgr) = rtc_mgr_opt {
                            let bt = if packet_type == 4 { 1 } else { 0 };
                            rtc_mgr.forward_web_vp8_frame(client_id, bt, payload).await;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Unidirectional QUIC streams for high-bitrate WebCodecs video / screen-share frames
    let uni_session = wtransport_session.clone();
    let uni_runtime = Arc::clone(&runtime);
    let uni_sessions = Arc::clone(&sessions);
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        loop {
            match uni_session.accept_uni().await {
                Ok(mut recv_uni) => {
                    let uni_runtime = Arc::clone(&uni_runtime);
                    let uni_sessions = Arc::clone(&uni_sessions);
                    tokio::spawn(async move {
                        let mut buf = Vec::with_capacity(65536);
                        if recv_uni.read_to_end(&mut buf).await.is_ok() && buf.len() > 1 {
                            let packet_type = buf[0];
                            let payload = &buf[1..];
                            let client_id = crate::web::WEB_CLIENT_ID_BASE + connection_id;
                            let mut server_id = None;
                            if let Ok(lock) = uni_sessions.lock() {
                                if let Some(sess) = lock.get(&client_id) {
                                    server_id = Some(sess.presence.server_id);
                                }
                            }
                            if let Some(sid) = server_id {
                                let (sender_channel_id, rtc_mgr) = {
                                    let mut rt = uni_runtime.lock().unwrap();
                                    rt.mark_client_seen(client_id);
                                    let cid = rt.online_client_snapshot(sid, client_id).map(|c| c.channel_id).unwrap_or(0);
                                    (cid, Arc::clone(&rt.rtc_manager))
                                };
                                if (packet_type == 2 || packet_type == 4) && payload.len() > 13 {
                                    let bt = if packet_type == 4 { 1 } else { 0 };
                                    rtc_mgr.forward_web_vp8_frame(client_id, bt, payload).await;
                                }
                                let mut targets = Vec::new();
                                if let Ok(lock) = uni_sessions.lock() {
                                    for session in lock.values() {
                                        if session.presence.server_id == sid
                                            && session.presence.channel_id == sender_channel_id
                                            && session.presence.client_id != client_id
                                        {
                                            if let Some(conn) = &session.wtransport_session {
                                                targets.push(conn.clone());
                                            }
                                        }
                                    }
                                }
                                let mut frame = Vec::with_capacity(1 + 8 + payload.len());
                                frame.push(packet_type);
                                frame.extend_from_slice(&client_id.to_le_bytes());
                                frame.extend_from_slice(payload);
                                let shared_frame = Arc::new(frame);
                                for conn in targets {
                                    let frame_ref = Arc::clone(&shared_frame);
                                    tokio::spawn(async move {
                                        if let Ok(opening) = conn.open_uni().await {
                                            if let Ok(mut send_uni) = opening.await {
                                                let _ = send_uni.write_all(&frame_ref).await;
                                                let _ = send_uni.finish().await;
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });
    let (mut send, recv) = wtransport_session.accept_bi().await?;
    let mut recv_reader = tokio::io::BufReader::new(recv);
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
            data.push('\n');
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
                    if let Ok(mut rt) = runtime.lock() {
                        rt.mark_client_seen(session.client_id);
                    }
                    send.write_all(b"\n").await?;
                    continue;
                }
                trace_blackteaweb_frame(connection_id, "in", text);
                let before_presence = session.presence();
                let mut outbound = {
                    let mut rt = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    rt.mark_client_seen(session.client_id);
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
                    register_or_update_session(
                        &sessions,
                        after_presence.clone(),
                        session
                            .self_client_database_id()
                            .unwrap_or(session.client_id + 1000),
                        visible_channel_ids,
                        Arc::clone(&pending_frames),
                    )?;
                    if let Ok(mut lock) = sessions.lock() {
                        if let Some(reg) = lock.get_mut(&after_presence.client_id) {
                            reg.wtransport_session = Some(wtransport_session.clone());
                        }
                    }
                }
                session.sync_rtc_presence()?;
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
                    let mut data = message;
                    data.push('\n');
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
        rt.remove_session_client(session.client_id, disconnect_reason_id, disconnect_reason_message.to_string());
    }
    let disconnect_cleanup_notifications = {
        let mut rt = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        match disconnect_presence.as_ref() {
            Some(presence) => _cleanup_notifications(
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
            if let Some(pres) = disconnect_presence.as_ref() {
                for notif in &disconnect_cleanup_notifications {
                    rt.broadcast_event(pres.server_id, notif);
                }
            }
        }
    }
    Ok(())
}
