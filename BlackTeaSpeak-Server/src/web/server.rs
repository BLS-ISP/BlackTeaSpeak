use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tungstenite::error::Error as WebSocketError;
use tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use tungstenite::{Message, accept};
use wtransport::{Endpoint, ServerConfig as WTransportServerConfig, Identity};

use crate::file_transfer::{
    FileEntryInfo, FileTransferError, FileTransferEvent, FileTransferRegistry,
    PreparedFileTransfer,
};
use crate::query::{CommandRequest, QueryResponse, encode_query_value};
use crate::models::{
    WhisperTargetSelection, WHISPER_TARGET_CHANNEL, WHISPER_TARGET_CLIENT,
    WHISPER_TARGET_SERVER_GROUP, WHISPER_TARGET_SELF,
};
use crate::runtime::{
    AntiFloodSessionState, BaselineRuntime, ChannelSnapshot, MusicBotNotifyPayload,
    OnlineClientSnapshot, QuerySessionState, ServerSnapshot,
    WebServerGroupMutationError, WebServerInitInfo, create_baseline_runtime,
    create_baseline_runtime_with_state_path, stable_web_client_database_id,
    stable_web_client_unique_identifier,
};
use crate::transport::{SessionPresence, TransportNotification};

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

    pub(crate) async fn run_async(mut self) -> Result<()> {
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
                    wt_payload.push(packet_type);
                    wt_payload.extend_from_slice(&sender_client_id.to_le_bytes());
                    wt_payload.extend_from_slice(&payload);
                    for conn in targets {
                        let _ = conn.send_datagram(wt_payload.clone());
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
    params.not_before = rcgen::date_time_ymd(2026, 5, 20);
    params.not_after = rcgen::date_time_ymd(2026, 5, 28);

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
    
    params.not_before = rcgen::date_time_ymd(2026, 5, 20);
    params.not_after = rcgen::date_time_ymd(2026, 5, 28);

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
                        let mut rt = datagram_runtime.lock().unwrap();
                        rt.mark_client_seen(client_id);
                        rt.route_btea_media_to_webtransport(sid, client_id, packet_type, payload);
                        rt.route_btea_media_to_desktop(sid, client_id, packet_type, payload);
                    }
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
