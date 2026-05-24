use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::UdpSocket;
use tokio::time::Instant;

use crate::desktop_proto::ObservedDesktopPacket;
use crate::runtime::BaselineRuntime;

struct UdpClientSession {
    pub client_id: u64,
    pub last_seen: Instant,
    pub next_packet_id: u16,
    pub is_talking: bool,
    pub last_talk_millis: Instant,
}

pub struct DesktopTransportServer {
    server_id: u32,
    bind_addr: String,
    runtime: Arc<Mutex<BaselineRuntime>>,
    shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    media_rx: tokio::sync::broadcast::Receiver<(u32, u64, u8, Vec<u8>)>,
}

impl DesktopTransportServer {
    pub fn bind_with_shared_runtime(
        server_id: u32,
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
        shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    ) -> Result<Self> {
        let media_rx = runtime.lock().unwrap().desktop_btea_media_tx.as_ref().unwrap().subscribe();
        Ok(Self {
            server_id,
            bind_addr: bind_addr.to_string(),
            runtime,
            shared_secrets,
            media_rx,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.bind_addr.parse().context("invalid bind address")
    }

    pub fn run(self, should_stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("failed to build tokio runtime for desktop transport")?;

        rt.block_on(async move {
            let socket = UdpSocket::bind(&self.bind_addr)
                .await
                .context("failed to bind desktop udp socket")?;
            
            let socket = Arc::new(socket);
            let mut addr_to_session = HashMap::<SocketAddr, UdpClientSession>::new();
            let mut buf = vec![0u8; 4096];

            let mut last_gc = Instant::now();
            let mut media_rx = self.media_rx;
            loop {
                if should_stop.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
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
                    }
                    result = socket.recv_from(&mut buf) => {
                        if let Ok((size, addr)) = result {
                            let packet = &buf[..size];
                            
                            if let Some(ObservedDesktopPacket::Ts3EncryptedPacket { mac, packet_id, client_id, flags, payload }) = ObservedDesktopPacket::parse(packet) {
                                let client_id_u64 = client_id as u64;
                                
                                let shared_secret_opt = {
                                    let secrets = self.shared_secrets.lock().unwrap();
                                    secrets.get(&client_id_u64).cloned()
                                };
                                
                                if let Some(shared_secret) = shared_secret_opt {
                                    let header = packet[8..13].to_vec();
                                    let mut payload_with_mac = Vec::with_capacity(8 + payload.len());
                                    payload_with_mac.extend_from_slice(&mac);
                                    payload_with_mac.extend_from_slice(&payload);
                                    
                                    if let Some(decrypted_payload) = crate::desktop_crypto::decrypt_btea_packet(
                                        packet_id,
                                        0, // generation_id
                                        flags,
                                        &header,
                                        &payload_with_mac,
                                        &shared_secret,
                                        false, // is_server_to_client
                                    ) {
                                        // Update session mapping
                                        let mut newly_talking = false;
                                        addr_to_session.entry(addr).and_modify(|s| {
                                            s.last_seen = Instant::now();
                                        }).or_insert(UdpClientSession {
                                            client_id: client_id_u64,
                                            last_seen: Instant::now(),
                                            next_packet_id: 0,
                                            is_talking: false,
                                            last_talk_millis: Instant::now(),
                                        });
                                        
                                        {
                                            let mut rt = self.runtime.lock().unwrap();
                                            rt.mark_client_seen(client_id_u64);
                                        }
                                        
                                        let packet_type = flags & 0x0F;
                                        if packet_type == 0x0A || packet_type == 0x0B {
                                            // Find channel id of this client
                                            let channel_id = {
                                                let rt_guard = self.runtime.lock().unwrap();
                                                let snapshot = rt_guard.online_client_snapshot(self.server_id, client_id_u64);
                                                snapshot.map(|c| c.channel_id)
                                            };
                                            
                                            if let Some(cid) = channel_id {
                                                // Talk status logic
                                                if let Some(session) = addr_to_session.get_mut(&addr) {
                                                    session.last_talk_millis = Instant::now();
                                                    if !session.is_talking {
                                                        session.is_talking = true;
                                                        let rt_guard = self.runtime.lock().unwrap();
                                                        rt_guard.broadcast_event(self.server_id, &crate::transport::TransportNotification::TalkStatus {
                                                            server_id: self.server_id,
                                                            channel_id: cid,
                                                            client_id: client_id_u64,
                                                            is_talking: true,
                                                        });
                                                    }
                                                }

                                                Self::broadcast_media(
                                                    &socket,
                                                    &mut addr_to_session,
                                                    &self.shared_secrets,
                                                    &self.runtime,
                                                    self.server_id,
                                                    client_id_u64,
                                                    addr,
                                                    decrypted_payload,
                                                    packet_type,
                                                ).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {
                        let now = Instant::now();
                        for session in addr_to_session.values_mut() {
                            if session.is_talking && now.duration_since(session.last_talk_millis) > Duration::from_millis(500) {
                                session.is_talking = false;
                                let rt_guard = self.runtime.lock().unwrap();
                                if let Some(c) = rt_guard.online_client_snapshot(self.server_id, session.client_id) {
                                    rt_guard.broadcast_event(self.server_id, &crate::transport::TransportNotification::TalkStatus {
                                        server_id: self.server_id,
                                        channel_id: c.channel_id,
                                        client_id: session.client_id,
                                        is_talking: false,
                                    });
                                }
                            }
                        }
                    }
                }
                
                let now = Instant::now();
                if now.duration_since(last_gc) > Duration::from_secs(10) {
                    addr_to_session.retain(|_, session| {
                        now.duration_since(session.last_seen) < Duration::from_secs(30)
                    });
                    last_gc = now;
                }
            }
            Ok(())
        })
    }

    async fn broadcast_media(
        socket: &Arc<UdpSocket>,
        sessions: &mut HashMap<SocketAddr, UdpClientSession>,
        shared_secrets: &Arc<Mutex<HashMap<u64, Vec<u8>>>>,
        runtime: &Arc<Mutex<BaselineRuntime>>,
        server_id: u32,
        sender_client_id: u64,
        sender: SocketAddr,
        payload: Vec<u8>,
        flag: u8,
    ) {
        if payload.len() > 2500 {
            return; // Anti-Flood / Codec Enforcement: Drop unreasonably large voice payloads
        }
        
        let rt_guard = runtime.lock().unwrap();
        
        let sender_snapshot = rt_guard.online_client_snapshot(server_id, sender_client_id);
        let sender_channel_id = sender_snapshot.as_ref().map(|c| c.channel_id).unwrap_or(0);
        let whisper_targets = sender_snapshot.as_ref().and_then(|c| c.whisper_targets.clone());
        let is_whisper = (flag & 0x0F) == 0x01 || (flag & 0x0F) == 0x03;
        let packet_type = flag & 0x0F;

        let mut recipients = Vec::new();
        for (peer_addr, peer_session) in sessions.iter() {
            if *peer_addr != sender {
                let peer_snapshot = rt_guard.online_client_snapshot(server_id, peer_session.client_id);
                if let Some(c) = peer_snapshot {
                    let mut should_route = false;
                    if is_whisper {
                        if let Some(ref targets) = whisper_targets {
                            if targets.client_ids.contains(&c.id) {
                                should_route = true;
                            } else if targets.channel_ids.contains(&c.channel_id) {
                                should_route = true;
                            }
                        }
                    } else {
                        if c.channel_id == sender_channel_id {
                            should_route = true;
                        }
                    }
                    
                    if should_route && !c.ignored_clients.contains(&sender_client_id) {
                        recipients.push(*peer_addr);
                    }
                }
            }
        }
        drop(rt_guard);
        
        if packet_type == 0x0A || packet_type == 0x0B {
            let rt_guard = runtime.lock().unwrap();
            rt_guard.route_btea_media_to_webtransport(server_id, sender_client_id, packet_type, &payload);
        }
        
        for peer_addr in recipients {
            if let Some(peer_session) = sessions.get_mut(&peer_addr) {
                let shared_secret_opt = {
                    let secrets = shared_secrets.lock().unwrap();
                    secrets.get(&peer_session.client_id).cloned()
                };
                
                if let Some(shared_secret) = shared_secret_opt {
                    let out_packet_id = peer_session.next_packet_id;
                    peer_session.next_packet_id = peer_session.next_packet_id.wrapping_add(1);
                    
                    let out_flags = flag;
                    let mut out_header = [0u8; 5];
                    out_header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
                    out_header[2..4].copy_from_slice(&(peer_session.client_id as u16).to_be_bytes());
                    out_header[4] = out_flags;
                    
                    let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
                        out_packet_id,
                        0,
                        out_flags,
                        &out_header,
                        &payload,
                        &shared_secret,
                        true,
                    );
                    
                    let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
                    final_packet.extend_from_slice(&encrypted_out[0..8]);
                    final_packet.extend_from_slice(&out_header);
                    final_packet.extend_from_slice(&encrypted_out[8..]);
                    
                    let _ = socket.send_to(&final_packet, peer_addr).await;
                }
            }
        }
    }
}
