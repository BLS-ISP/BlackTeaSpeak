use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::io::{Read, Write};

use anyhow::{Context, Result};
use std::net::TcpListener;
use tokio::time::Instant;
use rand::rngs::OsRng;
use sha2::{Sha512, Digest};
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::runtime::BaselineRuntime;
use crate::desktop_session::DesktopHandshakeStage;
use crate::desktop_handler::DesktopSessionHandler;

pub const DEFAULT_DESKTOP_TCP_BIND: &str = "0.0.0.0:9988";

type SharedClients = Arc<Mutex<HashMap<u64, DesktopTcpClientSession>>>;

pub struct DesktopTcpClientSession {
    pub sender: std::sync::mpsc::Sender<Vec<u8>>,
    pub state: std::sync::Arc<std::sync::RwLock<crate::runtime::QuerySessionState>>,
}

#[derive(Clone)]
pub struct DesktopTcpNotificationBridge {
    clients: SharedClients,
}

impl DesktopTcpNotificationBridge {
    pub fn broadcast(
        &self,
        origin_client_id: Option<u64>,
        notifications: &[crate::transport::TransportNotification],
    ) {
        if notifications.is_empty() {
            return;
        }

        let mut out_str = String::new();
        for notif in notifications {
            let cmd_str = crate::transport::render_notification(notif);
            out_str.push_str(&cmd_str);
            out_str.push('\n');
        }

        let clients = self.clients.lock().unwrap();
        for (client_id, session) in clients.iter() {
            if Some(*client_id) != origin_client_id {
                let _ = session.sender.send(out_str.as_bytes().to_vec()); // TCP sender loop encrypts this
            }
        }
    }
}

pub struct DesktopTcpTransportServer {
    pub server_id: u32,
    pub listener: TcpListener,
    pub runtime: Arc<Mutex<BaselineRuntime>>,
    pub clients: SharedClients,
    pub next_client_id: Arc<AtomicU64>,
    pub shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
}

impl DesktopTcpTransportServer {
    pub fn bind_with_shared_runtime(
        server_id: u32,
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
        shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr)
            .with_context(|| format!("failed to bind desktop tcp transport to {bind_addr}"))?;
        let clients: Arc<Mutex<HashMap<u64, DesktopTcpClientSession>>> = Arc::new(Mutex::new(HashMap::new()));
        
        let clients_for_bus = clients.clone();
        runtime.lock().unwrap().subscribe_events(Box::new(move |_rt, _server_id, notif| {
            let clients_lock = clients_for_bus.lock().unwrap();
            for (_, session) in clients_lock.iter() {
                let state_lock = session.state.read().unwrap();
                if crate::transport::wants_notification(&state_lock, notif) {
                    let cmd_str = crate::transport::render_notification(notif);
                    let _ = session.sender.send(format!("{}\n", cmd_str).into_bytes());
                }
            }
        }));

        Ok(Self {
            server_id,
            listener,
            runtime,
            clients,
            next_client_id: Arc::new(AtomicU64::new(1)),
            shared_secrets,
        })
    }

    pub fn notification_bridge(&self) -> DesktopTcpNotificationBridge {
        DesktopTcpNotificationBridge {
            clients: Arc::clone(&self.clients),
        }
    }


    
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener.local_addr().context("failed to get local addr")
    }

    pub fn run(self, should_stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
        self.listener.set_nonblocking(true).unwrap();
        loop {
            if should_stop.load(Ordering::SeqCst) {
                break;
            }
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let runtime = Arc::clone(&self.runtime);
                    let clients = Arc::clone(&self.clients);
                    let shared_secrets = Arc::clone(&self.shared_secrets);
                    let client_id = self.next_client_id.fetch_add(1, Ordering::SeqCst);
                    let server_id = self.server_id;
                    
                    std::thread::spawn(move || {
                        let mut stage = DesktopHandshakeStage::Initial;
                        let mut shared_secret_opt: Option<Vec<u8>> = None;
                        
                        let mut length_buf = [0u8; 4];
                        let mut next_out_packet_id = 0u16;
                        
                        // Handshake loop
                        if stream.read_exact(&mut length_buf).is_ok() {
                            let len = u32::from_be_bytes(length_buf) as usize;
                            if len == 41 {
                                let mut handshake_buf = vec![0u8; 41];
                                if stream.read_exact(&mut handshake_buf).is_ok() {
                                    if &handshake_buf[0..8] == b"BTEAINIT" && handshake_buf[8] == 0x01 {
                                        let mut client_public_key = [0u8; 32];
                                        client_public_key.copy_from_slice(&handshake_buf[9..41]);
                                        
                                        let server_secret = EphemeralSecret::random_from_rng(OsRng);
                                        let server_public = PublicKey::from(&server_secret);
                                        let client_public = PublicKey::from(client_public_key);
                                        let shared_secret = server_secret.diffie_hellman(&client_public);
                                        
                                        let mut hasher = Sha512::new();
                                        hasher.update(b"BTEA_SECRET_DERIVATION");
                                        hasher.update(shared_secret.as_bytes());
                                        shared_secret_opt = Some(hasher.finalize().to_vec());
                                        
                                        let mut response = [0u8; 41];
                                        response[0..8].copy_from_slice(b"BTEAINIT");
                                        response[8] = 0x02;
                                        response[9..41].copy_from_slice(server_public.as_bytes());
                                        
                                        let len_bytes = (41u32).to_be_bytes();
                                        let mut resp_packet = Vec::new();
                                        resp_packet.extend_from_slice(&len_bytes);
                                        resp_packet.extend_from_slice(&response);
                                        if stream.write_all(&resp_packet).is_ok() {
                                            stage = DesktopHandshakeStage::Connected;
                                        }
                                    }
                                }
                            }
                        }
                        
                        if stage != DesktopHandshakeStage::Connected {
                            return; // Handshake failed
                        }
                        
                        let shared_secret = shared_secret_opt.unwrap();
                        let shared_secret_clone = shared_secret.clone();
                        
                        {
                            let mut secrets_lock = shared_secrets.lock().unwrap();
                            secrets_lock.insert(client_id, shared_secret.clone());
                        }
                        
                        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                        let connection_ip = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());
                        let mut handler = DesktopSessionHandler::new(client_id, connection_ip);
                        
                        // Auto-select the correct virtual server based on the port this transport handles
                        handler.session.selected_virtual_server_id = Some(server_id);
                        let channel_id = {
                            let rt = runtime.lock().unwrap();
                            rt.web_default_channel_id(server_id).unwrap_or(1)
                        };
                        handler.session.current_channel_id = Some(channel_id);
                        
                        {
                            let mut rt = runtime.lock().unwrap();
                            rt.upsert_web_client(
                                client_id,
                                server_id,
                                channel_id,
                                handler.session.client_nickname.clone(),
                                "tcp-session".to_string(),
                                client_id + 1000,
                                "1.0.0".to_string(),
                                "Windows".to_string(),
                                handler.session.connection_ip.clone(),
                            );
                        }
                        
                        let shared_state = std::sync::Arc::new(std::sync::RwLock::new(handler.session.clone()));

                        {
                            let mut clients_lock = clients.lock().unwrap();
                            clients_lock.insert(client_id, DesktopTcpClientSession { sender: tx, state: shared_state.clone() });
                        }

                        
                        let mut stream_clone = stream.try_clone().unwrap();
                        let client_id_clone = client_id;
                        
                        std::thread::spawn(move || {
                            let mut next_packet_id = 0u16;
                            while let Ok(msg) = rx.recv() {
                                let out_packet_id = next_packet_id;
                                next_packet_id = next_packet_id.wrapping_add(1);
                                
                                let mut out_header = [0u8; 5];
                                out_header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
                                out_header[2..4].copy_from_slice(&(client_id_clone as u16).to_be_bytes());
                                out_header[4] = 0x02; // Command flag
                                
                                let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
                                    out_packet_id, 0, 0x02, &out_header, &msg, &shared_secret_clone, true
                                );
                                
                                let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
                                final_packet.extend_from_slice(&encrypted_out[0..8]);
                                final_packet.extend_from_slice(&out_header);
                                final_packet.extend_from_slice(&encrypted_out[8..]);
                                
                                let len_bytes = (final_packet.len() as u32).to_be_bytes();
                                let mut frame = Vec::with_capacity(4 + final_packet.len());
                                frame.extend_from_slice(&len_bytes);
                                frame.extend_from_slice(&final_packet);
                                
                                if stream_clone.write_all(&frame).is_err() {
                                    break;
                                }
                            }
                        });
                        
                        loop {
                            if stream.read_exact(&mut length_buf).is_err() {
                                break;
                            }
                            let len = u32::from_be_bytes(length_buf) as usize;
                            if len > 65536 || len < 13 {
                                break;
                            }
                            let mut packet = vec![0u8; len];
                            if stream.read_exact(&mut packet).is_err() {
                                break;
                            }
                            
                            let mut mac = [0u8; 8];
                            mac.copy_from_slice(&packet[0..8]);
                            let packet_id = u16::from_be_bytes([packet[8], packet[9]]);
                            let res_client_id = u16::from_be_bytes([packet[10], packet[11]]);
                            let flags = packet[12];
                            let payload = &packet[13..];
                            
                            let header = packet[8..13].to_vec();
                            let mut payload_with_mac = Vec::with_capacity(8 + payload.len());
                            payload_with_mac.extend_from_slice(&mac);
                            payload_with_mac.extend_from_slice(payload);
                            
                            if let Some(decrypted) = crate::desktop_crypto::decrypt_btea_packet(
                                packet_id, res_client_id as u32, flags, &header, &payload_with_mac, &shared_secret, false
                            ) {
                                {
                                    let mut rt = runtime.lock().unwrap();
                                    rt.mark_client_seen(client_id);
                                }
                                let packet_type = flags & 0x0F;
                                if packet_type == 0x02 { // Command
                                    if let Ok(cmd_str) = String::from_utf8(decrypted) {
                                        let is_quit = cmd_str.trim() == "quit";
                                        let (resp_lines, _notifs) = {
                                            let mut rt = runtime.lock().unwrap();
                                            handler.handle_command(&cmd_str, &mut rt)
                                        };
                                        {
                                            let mut state_lock = shared_state.write().unwrap();
                                            *state_lock = handler.session.clone();
                                        }
                                        if is_quit {
                                            break;
                                        }
                                        

                                        
                                        let mut out_str = String::new();
                                        for line in resp_lines {
                                            out_str.push_str(&line);
                                            out_str.push('\n');
                                        }
                                        if !out_str.is_empty() {
                                            let out_packet_id = next_out_packet_id;
                                            next_out_packet_id = next_out_packet_id.wrapping_add(1);
                                            
                                            let mut out_header = [0u8; 5];
                                            out_header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
                                            out_header[2..4].copy_from_slice(&(client_id as u16).to_be_bytes());
                                            out_header[4] = 0x02; // Command flag
                                            
                                            let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
                                                out_packet_id, 0, 0x02, &out_header, out_str.as_bytes(), &shared_secret, true
                                            );
                                            
                                            let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
                                            final_packet.extend_from_slice(&encrypted_out[0..8]);
                                            final_packet.extend_from_slice(&out_header);
                                            final_packet.extend_from_slice(&encrypted_out[8..]);
                                            
                                            let len_bytes = (final_packet.len() as u32).to_be_bytes();
                                            let mut frame = Vec::with_capacity(4 + final_packet.len());
                                            frame.extend_from_slice(&len_bytes);
                                            frame.extend_from_slice(&final_packet);
                                            if stream.write_all(&frame).is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Client disconnected
                        {
                            let mut clients_lock = clients.lock().unwrap();
                            clients_lock.remove(&client_id);
                        }
                        {
                            let mut secrets_lock = shared_secrets.lock().unwrap();
                            secrets_lock.remove(&client_id);
                        }
                        // Execute quit to clean up BaselineRuntime
                        {
                            let mut rt = runtime.lock().unwrap();
                            let (_, _notifs) = handler.handle_command("quit", &mut rt);
                            rt.remove_session_client(client_id, 8, "disconnected".to_string());
                            {
                                let mut state_lock = shared_state.write().unwrap();
                                *state_lock = handler.session.clone();
                            }

                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => eprintln!("desktop tcp accept error: {}", e),
            }
        }
        Ok(())
    }
}
