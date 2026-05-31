use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::net::UdpSocket;

use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::dtls_transport::dtls_role::DTLSRole;
use webrtc::track::track_local::TrackLocalWriter;

use crate::runtime::BaselineRuntime;
use std::collections::HashMap;
use crate::desktop_proto::{ObservedDesktopPacket, Ts3InitCommand};

fn decompress_if_needed(data: &[u8]) -> Vec<u8> {
    if data.len() < 3 {
        return data.to_vec();
    }
    let first = data[0];
    let is_compressed = (first & 1) == 1;
    let level = (first >> 2) & 0x0F;

    if is_compressed && level == 1 {
        // Read decompressed size from header
        let n = if (first & 2) == 2 { 4 } else { 1 };
        if data.len() < 1 + n + n {
            return data.to_vec();
        }
        let offset = 1 + n;
        let mut original_size: u32 = 0;
        for i in 0..n {
            original_size |= (data[offset + i] as u32) << (i * 8);
        }
        
        let mut cursor = data;
        match quicklz::decompress(&mut cursor, original_size) {
            Ok(decompressed) => {
                println!("QuickLZ Decompression success! Size {} -> {}", data.len(), decompressed.len());
                decompressed
            }
            Err(e) => {
                eprintln!("QuickLZ Decompression failed: {:?}", e);
                data.to_vec()
            }
        }
    } else {
        if (first & 1) == 0 && level == 1 {
            let n = if (first & 2) == 2 { 4 } else { 1 };
            let header_len = 2 * n + 1;
            if data.len() > header_len {
                println!("QuickLZ Uncompressed payload: stripping {} bytes header", header_len);
                return data[header_len..].to_vec();
            }
        }
        data.to_vec()
    }
}

const TS3INIT1_MAGIC: &[u8; 8] = b"TS3INIT1";
const TS3INIT_PACKET_ID: u16 = 101;

const TS3INIT_FLAGS: u8 = 0x88;
const TS3INIT_SET_COOKIE_PACKET_LENGTH: usize = 32;
const TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH: usize = 244;

pub struct UdpSession {
    pub seed_client: Vec<u8>,
    pub seed_server: Vec<u8>,
    pub server_sec: p256::SecretKey,
    pub shared_secret: [u8; 64],
    pub client_uid: String,
    pub server_uid: String,
    pub server_packet_id: Arc<Mutex<u16>>,
    pub server_ack_packet_id: u16,
    pub pending_rtc_describe: bool,
    pub pending_rtc_is_video: bool,
    pub rtc_describe_buffer: Vec<u8>,
    pub ephemeral_sec: Option<[u8; 32]>,
    pub entry2_hash: Option<[u8; 64]>,
}

pub struct TeaSpeakTransportServer {
    pub server_id: u32,
    pub bind_addr: String,
    pub runtime: Arc<Mutex<BaselineRuntime>>,
    pub rtc_manager: Arc<crate::rtc_manager::RtcBroadcastManager>,
}

impl TeaSpeakTransportServer {
    pub fn bind_with_shared_runtime(
        server_id: u32,
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
    ) -> Result<Self> {
        Ok(Self {
            server_id,
            bind_addr: bind_addr.to_string(),
            runtime,
            rtc_manager: Arc::new(crate::rtc_manager::RtcBroadcastManager::new()),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.bind_addr.parse().context("invalid bind address")
    }

    pub fn run(self, should_stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("failed to build tokio runtime for teaspeak transport")?;

        rt.block_on(async move {
            let socket = UdpSocket::bind(&self.bind_addr)
                .await
                .context("failed to bind teaspeak udp socket")?;
            
            let socket = Arc::new(socket);
            let mut buf = vec![0u8; 4096];
            let mut sessions: HashMap<SocketAddr, UdpSession> = HashMap::new();

            loop {
                if should_stop.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
                    result = socket.recv_from(&mut buf) => {
                        if let Ok((size, addr)) = result {
                            let packet = &buf[..size];
                            
                            if let Some(parsed) = ObservedDesktopPacket::parse(packet) {
                                match parsed {
                                    ObservedDesktopPacket::Ts3InitGetCookie { random_sequence } => {
                                        println!("teaspeak udp: received GetCookie from {} (seq: {})", addr, random_sequence);
                                        println!("teaspeak udp: RAW PACKET = {:02X?}", packet);
                                        // Send SetCookie
                                        let mut out = [0_u8; TS3INIT_SET_COOKIE_PACKET_LENGTH];
                                        out[..8].copy_from_slice(TS3INIT1_MAGIC);
                                        out[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
                                        out[10] = TS3INIT_FLAGS;
                                        out[11] = 0x01;
                                        
                                        let cookie: u64 = 0x1122334455667788; // Dummy cookie
                                        let packet_index = 0;
                                        out[12..20].copy_from_slice(&cookie.to_le_bytes());
                                        out[20] = packet_index;
                                        out[21..28].fill(0);
                                        out[28..32].copy_from_slice(&random_sequence.to_le_bytes());
                                        
                                        let _ = socket.send_to(&out, addr).await;
                                    }
                                    ObservedDesktopPacket::Ts3InitGetPuzzle { cookie, packet_index } => {
                                        println!("teaspeak udp: received GetPuzzle from {} (cookie: {})", addr, cookie);
                                        // Send SetPuzzle
                                        let mut out = [0_u8; TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH];
                                        out[..8].copy_from_slice(TS3INIT1_MAGIC);
                                        out[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
                                        out[10] = TS3INIT_FLAGS;
                                        out[11] = 0x03;

                                        // Observed on-wire success path from a real BlackTeaSpeak server.
                                        out[75] = 0x01;
                                        out[139] = 0x01;
                                        out[142] = 0x03;
                                        out[143] = 0xE8;
                                        
                                        let _ = socket.send_to(&out, addr).await;
                                    }
                                    ObservedDesktopPacket::Ts3InitSolvePuzzle { payload } => {
                                        let payload_str = String::from_utf8_lossy(&payload);
                                        println!("teaspeak udp: received SolvePuzzle from {}", addr);
                                        println!("teaspeak udp: SolvePuzzle payload: {}", payload_str);
                                        
                                        let is_teaspeak = payload_str.contains("teaspeak");
                                        
                                        if is_teaspeak {
                                            println!("Handling TeaSpeak TCP handshake (initivexpand)...");
                                            
                                            // Extract alpha and omega
                                            let mut alpha_val = "";
                                            let mut client_omega_val = "";
                                            for part in payload_str.split_whitespace() {
                                                if part.starts_with("alpha=") {
                                                    alpha_val = &part[6..];
                                                } else if part.starts_with("omega=") {
                                                    client_omega_val = &part[6..];
                                                }
                                            }
                                            
                                            let ts3_unescape = |s: &str| -> String {
                                                s.replace("\\s", " ").replace("\\p", "|").replace("\\a", "\x07").replace("\\b", "\x08").replace("\\f", "\x0c").replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t").replace("\\v", "\x0b").replace("\\/", "/").replace("\\\\", "\\")
                                            };
                                            let alpha_unescaped = ts3_unescape(alpha_val);
                                            let client_omega_unescaped = ts3_unescape(client_omega_val);
                                            
                                            let mut beta_bytes = [0u8; 10];
                                            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut beta_bytes);
                                            use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
                                            
                                            let (server_sec, server_pub) = crate::desktop_crypto::generate_server_keypair();
                                            let omega_bytes = crate::desktop_crypto::export_public_key_libtomcrypt_asn1(&server_pub);
                                            
                                            // Calculate shared secret using client's omega
                                            let client_omega_bytes = BASE64_STANDARD.decode(&client_omega_unescaped).unwrap_or_default();
                                            let mut shared_secret_array = [0u8; 64];
                                            if let Some(client_sec1) = crate::desktop_crypto::import_public_key_libtomcrypt_asn1(&client_omega_bytes) {
                                                println!("Successfully parsed client omega to sec1! len={}", client_sec1.len());
                                                if let Some(shared_sec_20) = crate::desktop_crypto::calculate_shared_secret(&client_sec1, &server_sec) {
                                                    println!("Successfully calculated shared secret! shared_sec={:?}", shared_sec_20);
                                                    shared_secret_array[0..20].copy_from_slice(&shared_sec_20);
                                                } else {
                                                    println!("Failed to calculate shared secret!");
                                                }
                                            } else {
                                                println!("Failed to parse client omega! Bytes: {:02X?}", client_omega_bytes);
                                            }
                                            
                                            let ts3_escape = |s: &str| -> String {
                                                s.replace("\\", "\\\\").replace(" ", "\\s").replace("/", "\\/").replace("|", "\\p").replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t").replace("\x0b", "\\v")
                                            };
                                            
                                            let beta_b64 = ts3_escape(&BASE64_STANDARD.encode(&beta_bytes));
                                            let omega_b64 = ts3_escape(&BASE64_STANDARD.encode(&omega_bytes));
                                            
                                            let initivexpand_cmd = format!("initivexpand alpha={} beta={} omega={} teaspeak=1", alpha_val, beta_b64, omega_b64);
                                            
                                            let mut header = [0u8; 3];
                                            header[0..2].copy_from_slice(&0u16.to_be_bytes());
                                            header[2] = 0x82; // Unencrypted Command
                                            
                                            let mut encrypted_payload = Vec::new();
                                            encrypted_payload.extend_from_slice(&[0u8; 8]); // Dummy MAC
                                            encrypted_payload.extend_from_slice(&header);
                                            encrypted_payload.extend_from_slice(initivexpand_cmd.as_bytes());
                                            
                                            use sha1::{Digest, Sha1};
                                            let mut hasher = Sha1::new();
                                            hasher.update(client_omega_unescaped.as_bytes());
                                            let client_uid = BASE64_STANDARD.encode(hasher.finalize());

                                            let mut server_hasher = Sha1::new();
                                            let omega_string = BASE64_STANDARD.encode(&omega_bytes);
                                            server_hasher.update(omega_string.as_bytes());
                                            let server_uid = BASE64_STANDARD.encode(server_hasher.finalize());

                                            sessions.insert(addr, UdpSession {
                                                seed_client: BASE64_STANDARD.decode(&alpha_unescaped).unwrap_or_default(),
                                                seed_server: beta_bytes.to_vec(),
                                                server_sec,
                                                shared_secret: shared_secret_array,
                                                client_uid,
                                                server_uid,
                                                server_packet_id: Arc::new(Mutex::new(1)),
                                                server_ack_packet_id: 1,
                                                pending_rtc_describe: false,
                                                pending_rtc_is_video: false,
                                                rtc_describe_buffer: Vec::new(),
                                                ephemeral_sec: None,
                                                entry2_hash: None,
                                            });
                                            
                                            let _ = socket.send_to(&encrypted_payload, addr).await;
                                            println!("Sent initivexpand to {}: {}", addr, initivexpand_cmd);
                                        } else {
                                             println!("Handling new protocol (ot=1)...");
                                             
                                             if let Some(parsed_payload) = crate::desktop_proto::parse_ts3init_solve_puzzle_payload(&payload) {
                                                 let mut beta_bytes = [0u8; 54];
                                                 rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut beta_bytes);
                                                 
                                                 use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
                                                 
                                                  let (chain_b64, identity_sec, root_key_pbl) = crate::desktop_crypto::load_protocol_key().unwrap();
                                                  let mut crypto_chain = BASE64_STANDARD.decode(&chain_b64).unwrap();
                                                  
                                                  let (server_sec, server_pub) = crate::desktop_crypto::generate_server_keypair();
                                                  let omega_bytes_asn1 = crate::desktop_crypto::export_public_key_asn1_der(&server_pub);
                                                  
                                                  let is_teaspeak = parsed_payload.teaspeak;
                                                  
                                                  // Ephemeral Ed25519 Keypair generation (only for official TS3 client, not teaspeak client)
                                                  let mut ephemeral_sec_bytes = [0u8; 32];
                                                  let mut ephemeral_pub_bytes = [0u8; 32];
                                                  if !is_teaspeak {
                                                      let mut rng = rand::thread_rng();
                                                      rand::RngCore::fill_bytes(&mut rng, &mut ephemeral_sec_bytes);
                                                      let ephemeral_signing_key = ed25519_dalek::SigningKey::from_bytes(&ephemeral_sec_bytes);
                                                      ephemeral_pub_bytes = ephemeral_signing_key.verifying_key().to_bytes();
                                                  }
                                                  
                                                  // Construct Ephemeral Entry
                                                  let mut entry2_bytes = Vec::new();
                                                  entry2_bytes.push(0x00); // Fixed prefix/separator
                                                  if !is_teaspeak {
                                                      entry2_bytes.extend_from_slice(&ephemeral_pub_bytes);
                                                  } else {
                                                      let omega_bytes_raw = crate::desktop_crypto::export_public_key_libtomcrypt(&server_pub);
                                                      entry2_bytes.extend_from_slice(&omega_bytes_raw[0..32]);
                                                  }
                                                  entry2_bytes.push(0x20); // LicenseType::EPHEMERAL (0x20)
                                                  
                                                  const TIMESTAMP_OFFSET: u64 = 1356998400;
                                                  let validation_time = std::time::SystemTime::now()
                                                      .duration_since(std::time::UNIX_EPOCH)
                                                      .unwrap_or_default()
                                                      .as_secs();
                                                  let begin = (validation_time - TIMESTAMP_OFFSET) as u32;
                                                  let end = (validation_time + 30 * 24 * 3600 - TIMESTAMP_OFFSET) as u32;
                                                  entry2_bytes.extend_from_slice(&begin.to_be_bytes());
                                                  entry2_bytes.extend_from_slice(&end.to_be_bytes());
                                                  
                                                  let mut entry2_hash_bytes = [0u8; 64];
                                                  if !is_teaspeak {
                                                      // For official TS3 client, sign the ephemeral entry using intermediate Ed25519 identity key
                                                      use ed25519_dalek::{SigningKey, Signer};
                                                      let mut prv_array = [0u8; 32];
                                                      prv_array.copy_from_slice(&identity_sec);
                                                      let signing_key = SigningKey::from_bytes(&prv_array);
                                                      let signature = signing_key.sign(&entry2_bytes[1..]);
                                                      entry2_bytes.extend_from_slice(&signature.to_bytes());
                                                      
                                                      use sha2::{Digest as _, Sha512};
                                                      let mut hasher = Sha512::new();
                                                      hasher.update(&entry2_bytes[1..]);
                                                      entry2_hash_bytes = hasher.finalize().into();
                                                  }
                                                  
                                                  // Append Ephemeral Entry to Chain
                                                  crypto_chain.extend_from_slice(&entry2_bytes);
                                                  
                                                  let chain_b64_new = BASE64_STANDARD.encode(&crypto_chain);
                                                  let root_b64 = BASE64_STANDARD.encode(&root_key_pbl);
                                                  
                                                  let proof = crate::desktop_crypto::generate_server_proof(
                                                      &server_sec.to_bytes(), 
                                                      &crypto_chain
                                                  ).unwrap();
                                                 
                                                 let mut shared_secret_array = [0u8; 64];
                                                 if is_teaspeak {
                                                     if let Some(client_sec1) = crate::desktop_crypto::import_public_key_libtomcrypt_asn1(&parsed_payload.omega_bytes) {
                                                         if let Some(shared_sec_20) = crate::desktop_crypto::calculate_shared_secret(&client_sec1, &server_sec) {
                                                             println!("ot=1 successfully calculated teaspeak shared secret!");
                                                             shared_secret_array[0..20].copy_from_slice(&shared_sec_20);
                                                         }
                                                     }
                                                 }
                                                 
                                                 use sha1::{Digest, Sha1};
                                                 let client_uid = {
                                                     let mut hasher = Sha1::new();
                                                     hasher.update(parsed_payload.omega.as_bytes());
                                                     BASE64_STANDARD.encode(hasher.finalize())
                                                 };
                                                 let server_uid = {
                                                     let mut hasher = Sha1::new();
                                                     let omega_string = BASE64_STANDARD.encode(&omega_bytes_asn1);
                                                     hasher.update(omega_string.as_bytes());
                                                     BASE64_STANDARD.encode(hasher.finalize())
                                                 };
                                                 
                                                 sessions.insert(addr, UdpSession {
                                                     seed_client: parsed_payload.alpha_bytes.clone(),
                                                     seed_server: beta_bytes.to_vec(),
                                                     server_sec: server_sec.clone(),
                                                     shared_secret: shared_secret_array,
                                                     client_uid,
                                                     server_uid,
                                                     server_packet_id: Arc::new(Mutex::new(1)),
                                                     server_ack_packet_id: 1,
                                                     pending_rtc_describe: false,
                                                     pending_rtc_is_video: false,
                                                     rtc_describe_buffer: Vec::new(),
                                                     ephemeral_sec: if is_teaspeak { None } else { Some(ephemeral_sec_bytes) },
                                                     entry2_hash: if is_teaspeak { None } else { Some(entry2_hash_bytes) },
                                                 });
                                                
                                                let ts3_escape = |s: &str| -> String {
                                                    s.replace("\\", "\\\\").replace(" ", "\\s").replace("/", "\\/").replace("|", "\\p").replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t").replace("\x0b", "\\v")
                                                };
                                                
                                                let beta_b64 = ts3_escape(&BASE64_STANDARD.encode(&beta_bytes));
                                                let omega_b64 = ts3_escape(&BASE64_STANDARD.encode(&omega_bytes_asn1));
                                                let proof_b64 = ts3_escape(&BASE64_STANDARD.encode(&proof));
                                                let root_b64_esc = ts3_escape(&root_b64);
                                                let chain_b64_esc = ts3_escape(&chain_b64_new);
                                                
                                                let initivexpand2_cmd = format!("initivexpand2 time={} l={} beta={} omega={} proof={} tvd root={} ot=1",
                                                    validation_time,
                                                    chain_b64_esc,
                                                    beta_b64,
                                                    omega_b64,
                                                    proof_b64,
                                                    root_b64_esc
                                                );
                                                
                                                let mut header = [0u8; 3];
                                                header[0..2].copy_from_slice(&0u16.to_be_bytes());
                                                header[2] = 0x20 | 0x02; // NewProtocol | Command
                                                
                                                let encrypted_payload = crate::desktop_crypto::encrypt_with_dummy_key(0, &header, initivexpand2_cmd.as_bytes());
                                                
                                                let _ = socket.send_to(&encrypted_payload, addr).await;
                                                let _ = socket.send_to(&encrypted_payload, addr).await;
                                                
                                                println!("Sent initivexpand2 to {}: {}", addr, initivexpand2_cmd);
                                            } else {
                                                println!("ot=1: failed to parse solve puzzle payload!");
                                            }
                                        }
                                    }
                                    ObservedDesktopPacket::Ts3EncryptedPacket { mac, packet_id, client_id, flags, payload } => {
                                        if let Some(session) = sessions.get_mut(&addr) {
                                            // Construct the header that was used for AAD
                                            let mut header = [0u8; 5];
                                            header[0..2].copy_from_slice(&packet_id.to_be_bytes());
                                            header[2..4].copy_from_slice(&client_id.to_be_bytes());
                                            header[4] = flags;
                                            
                                            // Try decrypting with session key first, fallback to dummy key
                                            let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                                &session.shared_secret[0..20], 
                                                &session.seed_client, 
                                                &session.seed_server
                                            );
                                            
                                             let decrypted_opt = if (flags & 0x80) != 0 {
                                                 Some((payload.clone(), "unencrypted"))
                                             } else {
                                                 let mut payload_with_mac = Vec::with_capacity(mac.len() + payload.len());
                                                 payload_with_mac.extend_from_slice(&mac);
                                                 payload_with_mac.extend_from_slice(&payload);
                                                 
                                                 crate::desktop_crypto::decrypt_with_session_key(
                                                     packet_id,
                                                     0, // generation_id
                                                     flags,
                                                     &header,
                                                     &payload_with_mac,
                                                     &iv_struct,
                                                     false
                                                 ).map(|d| (d, "session")).or_else(|| {
                                                     crate::desktop_crypto::decrypt_with_dummy_key(packet_id, &header, &mac, &payload)
                                                         .map(|d| (d, "dummy"))
                                                 })
                                             };

                                             if let Some((decrypted, key_type)) = decrypted_opt {
                                                 println!("teaspeak udp: decrypted packet from {} using {} key. packet_id: {}, flags: {:02X}, payload_hex: {:02X?}", addr, key_type, packet_id, flags, decrypted);
                                                 
                                                 // Update baseline runtime keepalive to prevent zombie housekeeping timeout
                                                 {
                                                     let mut rt = self.runtime.lock().unwrap();
                                                     rt.mark_client_seen(client_id as u64);
                                                 }
                                                 
                                                 // Send UDP ACK/PONG if it is a COMMAND (type 2) or PING (type 4)
                                                 let packet_type = flags & 0x0F;
                                                 if packet_type == 0x02 || packet_type == 0x04 {
                                                     let ack_payload = packet_id.to_be_bytes();
                                                     let mut ack_header = [0u8; 3];
                                                     let is_new_protocol = (flags & 0x20) != 0;
                                                     let is_unencrypted = (flags & 0x80) != 0;
                                                     
                                                     let resp_type = if packet_type == 0x04 { 0x05 } else { 0x06 };
                                                     let ack_flags = if is_new_protocol {
                                                         if is_unencrypted { 0xA0 | resp_type } else { 0x20 | resp_type }
                                                     } else {
                                                         if is_unencrypted { 0x80 | resp_type } else { resp_type }
                                                     };
                                                     ack_header[0..2].copy_from_slice(&session.server_ack_packet_id.to_be_bytes());
                                                     ack_header[2] = ack_flags;
                                                     
                                                     if is_unencrypted {
                                                         let mut final_ack = Vec::with_capacity(8 + 3 + 2);
                                                         final_ack.extend_from_slice(&[0u8; 8]); // Dummy MAC
                                                         final_ack.extend_from_slice(&ack_header);
                                                         final_ack.extend_from_slice(&ack_payload);
                                                         let _ = socket.send_to(&final_ack, addr).await;
                                                     } else {
                                                         let enc_ack = crate::desktop_crypto::encrypt_btea_packet(
                                                             session.server_ack_packet_id, 0, ack_flags, &ack_header, &ack_payload,
                                                             &iv_struct, true
                                                         );
                                                         let mut final_ack = Vec::with_capacity(8 + 3 + enc_ack.len() - 8);
                                                         final_ack.extend_from_slice(&enc_ack[0..8]);
                                                         final_ack.extend_from_slice(&ack_header);
                                                         final_ack.extend_from_slice(&enc_ack[8..]);
                                                         let _ = socket.send_to(&final_ack, addr).await;
                                                     }
                                                     
                                                     session.server_ack_packet_id = session.server_ack_packet_id.wrapping_add(1);
                                                     println!("Sent UDP {} {} for client packet_id {} (flags={:02X})", if resp_type == 0x05 { "PONG" } else { "ACK" }, session.server_ack_packet_id - 1, packet_id, ack_flags);
                                                 }
                                                
                                                let is_fragmented = (flags & 0x10) != 0;
                                                let processed_decrypted = if !is_fragmented {
                                                    decompress_if_needed(&decrypted)
                                                } else {
                                                    decrypted.clone()
                                                };
                                                
                                                let payload_str = String::from_utf8_lossy(&processed_decrypted);
                                                println!("teaspeak udp: payload_str: {}", payload_str);
                                                
                                                if payload_str.contains("rtcsessiondescribe") {
                                                    session.pending_rtc_describe = true;
                                                    session.pending_rtc_is_video = payload_str.contains("m=video") || payload_str.contains("video");
                                                    session.rtc_describe_buffer.clear();
                                                    session.rtc_describe_buffer.extend_from_slice(&decrypted);
                                                } else if session.pending_rtc_describe {
                                                    session.rtc_describe_buffer.extend_from_slice(&decrypted);
                                                }
                                                
                                                 if payload_str.contains("clientek") {
                                                     let mut ek_base64 = None;
                                                     let ek_key = b"ek=";
                                                     if let Some(ek_idx) = processed_decrypted.windows(ek_key.len()).position(|window| window == ek_key) {
                                                         let value_start = ek_idx + ek_key.len();
                                                         let mut value_end = value_start;
                                                         while value_end < processed_decrypted.len() {
                                                             let c = processed_decrypted[value_end];
                                                             if c <= 0x20 || c > 0x7E {
                                                                 break;
                                                             }
                                                             value_end += 1;
                                                         }
                                                         if let Ok(ek_str) = std::str::from_utf8(&processed_decrypted[value_start..value_end]) {
                                                             ek_base64 = Some(ek_str.replace("\\/", "/"));
                                                         }
                                                     }
                                                     
                                                     if let Some(ek_b64) = ek_base64 {
                                                         use base64::Engine as _;
                                                         if let Ok(client_ek_bytes) = base64::engine::general_purpose::STANDARD.decode(ek_b64.trim()) {
                                                             if let (Some(ephemeral_sec_bytes), Some(entry2_hash_bytes)) = (session.ephemeral_sec, session.entry2_hash) {
                                                                 if let Some((_, identity_sec, _)) = crate::desktop_crypto::load_protocol_key() {
                                                                     use curve25519_dalek::scalar::Scalar;
                                                                     
                                                                     let parent_private = Scalar::from_bytes_mod_order(identity_sec.try_into().unwrap_or([0u8; 32]));
                                                                     let ephemeral_private = Scalar::from_bytes_mod_order(ephemeral_sec_bytes);
                                                                     
                                                                     let mut clamped_hash_bytes = [0u8; 32];
                                                                     clamped_hash_bytes.copy_from_slice(&entry2_hash_bytes[0..32]);
                                                                     clamped_hash_bytes[0] &= 0xF8;
                                                                     clamped_hash_bytes[31] &= 0x3F;
                                                                     clamped_hash_bytes[31] |= 0x40;
                                                                     
                                                                     let clamped_hash = Scalar::from_bytes_mod_order(clamped_hash_bytes);
                                                                     let derived_private = (ephemeral_private * clamped_hash) + parent_private;
                                                                     let derived_private_bytes = derived_private.to_bytes();
                                                                     
                                                                     if let Some(shared_secret_64) = crate::desktop_crypto::get_shared_secret2(&client_ek_bytes, &derived_private_bytes) {
                                                                         println!("clientek: successfully computed 64-byte shared secret!");
                                                                         session.shared_secret.copy_from_slice(&shared_secret_64);
                                                                         
                                                                         // Send encrypted ACK/NewProtocol packet back to the client!
                                                                         let mut ack_header = [0u8; 3];
                                                                         let next_pid = {
                                                                             let mut pid_g = session.server_packet_id.lock().unwrap();
                                                                             let p = *pid_g;
                                                                             *pid_g = p.wrapping_add(1);
                                                                             p
                                                                         };
                                                                         ack_header[0..2].copy_from_slice(&next_pid.to_be_bytes());
                                                                         ack_header[2] = 0x20 | 0x06; // NewProtocol | Ack
                                                                         
                                                                         let ack_payload = packet_id.to_be_bytes();
                                                                         
                                                                         let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                                                             &session.shared_secret, 
                                                                             &session.seed_client, 
                                                                             &session.seed_server
                                                                         );
                                                                         
                                                                         let enc_ack = crate::desktop_crypto::encrypt_btea_packet(
                                                                             next_pid, 0, 0x20 | 0x06, &ack_header, &ack_payload,
                                                                             &iv_struct, true
                                                                         );
                                                                         let mut final_ack = Vec::with_capacity(8 + 3 + enc_ack.len() - 8);
                                                                         final_ack.extend_from_slice(&enc_ack[0..8]);
                                                                         final_ack.extend_from_slice(&ack_header);
                                                                         final_ack.extend_from_slice(&enc_ack[8..]);
                                                                         
                                                                         let _ = socket.send_to(&final_ack, addr).await;
                                                                         println!("Sent encrypted clientek ACK packet to client. packet_id: {}, ack_to: {}", next_pid, packet_id);
                                                                     }
                                                                 }
                                                             }
                                                         }
                                                     }
                                                 }
                                                 
                                                 if payload_str.contains("clientinit") {
                                                     let mut client_nickname = "BlackTeaUser".to_string();
                                                     
                                                     // Search for "client_nickname=" in the raw decrypted packet bytes
                                                     let nickname_key = b"client_nickname=";
                                                     if let Some(nick_idx) = processed_decrypted.windows(nickname_key.len()).position(|window| window == nickname_key) {
                                                         let value_start = nick_idx + nickname_key.len();
                                                         // Find the next space or control character
                                                         let mut value_end = value_start;
                                                         while value_end < processed_decrypted.len() {
                                                             let c = processed_decrypted[value_end];
                                                             if c <= 0x20 || c > 0x7E {
                                                                 break;
                                                             }
                                                             value_end += 1;
                                                         }
                                                         if let Ok(nick) = std::str::from_utf8(&processed_decrypted[value_start..value_end]) {
                                                             let ts3_unescape = |s: &str| -> String {
                                                                 s.replace("\\s", " ").replace("\\p", "|").replace("\\/", "/").replace("\\\\", "\\")
                                                             };
                                                             let parsed = ts3_unescape(nick);
                                                             if !parsed.is_empty() {
                                                                 client_nickname = parsed;
                                                             }
                                                         }
                                                     }

                                                     println!("Received clientinit! Client nickname: {}", client_nickname);
                                                     
                                                     let ts3_escape = |s: &str| -> String {
                                                         s.replace("\\", "\\\\").replace(" ", "\\s").replace("/", "\\/").replace("|", "\\p").replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t").replace("\x0b", "\\v")
                                                     };
                                                     
                                                     // Register client in the baseline runtime so mark_client_seen works and other users see them
                                                     {
                                                         let mut rt = self.runtime.lock().unwrap();
                                                         rt.upsert_web_client(
                                                             1, // hardcoded compat client ID
                                                             self.server_id,
                                                             1, // Default channel ID
                                                             client_nickname.clone(),
                                                             "teaspeak-compat-udp".to_string(),
                                                             1001, // database ID
                                                             "TeaClient".to_string(),
                                                             "Windows".to_string(),
                                                             addr.ip().to_string(),
                                                         );
                                                     }
                                                     
                                                     // Fetch real virtual server name and welcome message from runtime
                                                     let (server_name, welcome_msg) = {
                                                         let rt = self.runtime.lock().unwrap();
                                                         if let Some(info) = rt.web_server_init_info() {
                                                             (info.server_name.clone(), info.welcome_message.clone())
                                                         } else {
                                                             ("BlackTeaSpeak".to_string(), "Welcome!".to_string())
                                                         }
                                                     };
                                                     
                                                     let server_name_escaped = ts3_escape(&server_name);
                                                     let welcome_msg_escaped = ts3_escape(&welcome_msg);
                                                     
                                                     // We need to re-escape it for the outgoing packet:
                                                     let client_nickname_escaped = ts3_escape(&client_nickname);
                                                     println!("Client successfully passed TS3 handshake puzzle!");
                                                     
                                                     // Send ACK for clientinit
                                                     let mut ack_header = [0u8; 3];
                                                     ack_header[0..2].copy_from_slice(&0u16.to_be_bytes()); // ACK packet_id = 0
                                                     ack_header[2] = 0x22; // NewProtocol | Command ? Wait, ACK for clientinit usually doesn't have NewProtocol if it's an ACK?
                                                     // Let's just use what worked
                                                     
                                                     let mut ack_payload = Vec::new();
                                                     ack_payload.extend_from_slice(&ack_header);
                                                     
                                                     let enc_ack = crate::desktop_crypto::encrypt_with_dummy_key(packet_id, &[], &ack_payload);
                                                     
                                                     if let Err(e) = socket.send_to(&enc_ack, addr).await {
                                                         eprintln!("Failed to send ACK: {}", e);
                                                     }
                                                     println!("Sent ACK for clientinit (packet_id: {})", packet_id);
                                                     
                                                     let client_uid_escaped = ts3_escape(&session.client_uid);
                                                     let server_uid_escaped = ts3_escape(&session.server_uid);
                                                     let initserver = format!("initserver virtualserver_name={} virtualserver_welcomemessage={} virtualserver_maxclients=32 virtualserver_password virtualserver_clientsonline=1 virtualserver_channelsonline=1 virtualserver_created=1494921612 virtualserver_uptime=33245 virtualserver_hostmessage virtualserver_hostmessage_mode=0 virtualserver_filebase=files\\/virtualserver_1 virtualserver_default_server_group=8 virtualserver_default_channel_group=8 virtualserver_flag_password=0 virtualserver_default_channel_admin_group=5 virtualserver_max_download_total_bandwidth=-1 virtualserver_max_upload_total_bandwidth=-1 virtualserver_hostbanner_url virtualserver_hostbanner_gfx_url virtualserver_hostbanner_gfx_interval=0 virtualserver_complain_autoban_count=5 virtualserver_complain_autoban_time=1200 virtualserver_complain_remove_time=3600 virtualserver_min_clients_in_channel_before_forced_silence=100 virtualserver_priority_speaker_dimm_modificator=-18.0000 virtualserver_id=1 virtualserver_antiflood_points_tick_reduce=5 virtualserver_antiflood_points_needed_command_block=150 virtualserver_antiflood_points_needed_ip_block=250 virtualserver_client_connections=1 virtualserver_query_client_connections=0 virtualserver_hostbutton_tooltip virtualserver_hostbutton_url virtualserver_hostbutton_gfx_url virtualserver_queryclientsonline=0 virtualserver_download_quota=-1 virtualserver_upload_quota=-1 virtualserver_month_bytes_downloaded=0 virtualserver_month_bytes_uploaded=0 virtualserver_total_bytes_downloaded=0 virtualserver_total_bytes_uploaded=0 virtualserver_port=9987 virtualserver_autostart=1 virtualserver_machine_id virtualserver_needed_identity_security_level=8 virtualserver_log_client=0 virtualserver_log_query=0 virtualserver_log_channel=0 virtualserver_log_permissions=1 virtualserver_log_server=0 virtualserver_log_filetransfer=0 virtualserver_min_client_version=1481105459 virtualserver_name_phonetic virtualserver_icon_id=0 virtualserver_reserved_slots=0 virtualserver_total_packetloss_speech=0.0000 virtualserver_total_packetloss_keepalive=0.0000 virtualserver_total_packetloss_control=0.0000 virtualserver_total_packetloss_total=0.0000 virtualserver_total_ping=0.0000 virtualserver_ip=0.0000 virtualserver_weblist_identifier virtualserver_ask_for_privilegekey=0 virtualserver_hostbanner_mode=0 virtualserver_channel_temp_delete_delay_default=0 virtualserver_min_android_version=1429007622 virtualserver_min_ios_version=1429007622 virtualserver_nickname virtualserver_unique_identifier={} virtualserver_platform=Windows virtualserver_version=3.5.6 virtualserver_status=online virtualserver_codec_encryption_mode=0 client_talk_power=0 client_needed_serverquery_view_power=0 client_myteamspeak_id client_integrations lt=0 pv=6 acn={} aclid=1", server_name_escaped, welcome_msg_escaped, server_uid_escaped, client_nickname_escaped);
                                                    let channellist = format!("channellist cid=1 cpid=0 channel_name=Default\\sChannel channel_topic channel_description channel_password channel_codec=4 channel_codec_quality=6 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=1 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic channel_icon_id=0 channel_flag_private=0");
                                                    let notifycliententerview = format!(
                                                        "notifycliententerview cfid=0 ctid=1 reasonid=0 clid=1 client_unique_identifier={} client_nickname={} client_input_muted=0 client_output_muted=0 client_outputonly_muted=0 client_input_hardware=0 client_output_hardware=0 client_meta_data client_is_recording=0 client_database_id=1 client_channel_group_id=8 client_servergroups=8 client_away=0 client_away_message client_type=0 client_flag_avatar client_talk_power=0 client_talk_request=0 client_talk_request_msg client_description client_is_talker=0 client_is_priority_speaker=0 client_unread_messages=0 client_nickname_phonetic client_needed_serverquery_view_power=0 client_icon_id=0 client_is_channel_commander=0 client_country=DE client_channel_group_inherited_channel_id=1 client_badges client_myteamspeak_id client_integrations",
                                                        client_uid_escaped, client_nickname_escaped
                                                    );
                                                    let channellistfinished = "channellistfinished";

                                                    let commands_to_send = vec![initserver, channellist.to_string(), notifycliententerview, channellistfinished.to_string()];

                                                    for cmd in commands_to_send.iter() {
                                                        let payload_bytes = cmd.as_bytes();
                                                        let chunk_size = 400; // TS3 fragment limit
                                                        let chunks: Vec<&[u8]> = payload_bytes.chunks(chunk_size).collect();
                                                        
                                                        let total_chunks = chunks.len();
                                                        for (i, chunk) in chunks.iter().enumerate() {
                                                            let mut flags = 0x22;
                                                            if total_chunks > 1 && (i == 0 || i == total_chunks - 1) {
                                                                flags |= 0x10; // Fragmented
                                                            }
                                                            let out_packet_id = {
                                                                let mut lock = session.server_packet_id.lock().unwrap();
                                                                let val = *lock;
                                                                *lock = lock.wrapping_add(1);
                                                                val
                                                            };
                                                            let mut out_header = [0u8; 3];
                                                            out_header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
                                                            out_header[2] = flags;
                                                            let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
                                                                out_packet_id, 0, flags, &out_header, chunk,
                                                                &iv_struct, true
                                                            );
                                                            let mut final_packet = Vec::with_capacity(8 + 3 + encrypted_out.len() - 8);
                                                            final_packet.extend_from_slice(&encrypted_out[0..8]);
                                                            final_packet.extend_from_slice(&out_header);
                                                            final_packet.extend_from_slice(&encrypted_out[8..]);
                                                            let _ = socket.send_to(&final_packet, addr).await;
                                                        }
                                                        println!("Sent {} to {}", cmd.split(' ').next().unwrap_or(""), addr);
                                                    }
                                                } else if payload_str.starts_with("handshakebegin ") {
                                                    println!("Received handshakebegin! Client using TeaSpeak handshake.");
                                                    
                                                    if let Some(rc_idx) = payload_str.find("return_code=") {
                                                        let rc_val = payload_str[rc_idx + 12..].split(' ').next().unwrap_or("");
                                                        
                                                        // Send ACK FIRST
                                                        let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                                                        let ack_packet_id = {
                                                            let mut lock = session.server_packet_id.lock().unwrap();
                                                            let val = *lock;
                                                            *lock = lock.wrapping_add(1);
                                                            val
                                                        };
                                                        let mut out_header = [0u8; 3];
                                                        out_header[0..2].copy_from_slice(&ack_packet_id.to_be_bytes());
                                                        out_header[2] = 0x22;
                                                        
                                                        let enc_payload = crate::desktop_crypto::encrypt_btea_packet(
                                                            ack_packet_id, 0, 0x22, &out_header, ack_cmd.as_bytes(),
                                                            &iv_struct, true
                                                        );
                                                        
                                                        let mut final_packet = Vec::with_capacity(8 + 3 + enc_payload.len() - 8);
                                                        final_packet.extend_from_slice(&enc_payload[0..8]);
                                                        final_packet.extend_from_slice(&out_header);
                                                        final_packet.extend_from_slice(&enc_payload[8..]);
                                                        
                                                        let _ = socket.send_to(&final_packet, addr).await;
                                                        println!("Sent ack to {}: {}", addr, ack_cmd);

                                                        // Send handshakeidentityproof SECOND
                                                        let message = "TeaSpeak,\\smade\\swith\\slove\\sand\\scoffee\\sby\\sWolverinDEV\\s(#QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=)";
                                                        let handshakeidentityproof = format!("handshakeidentityproof message={} digest=SHA-256", message);
                                                        let proof_packet_id = {
                                                            let mut lock = session.server_packet_id.lock().unwrap();
                                                            let val = *lock;
                                                            *lock = lock.wrapping_add(1);
                                                            val
                                                        };
                                                        let mut out_header = [0u8; 3];
                                                        let is_new_protocol = true;
                                                        let flags = if is_new_protocol { 0x22 } else { 0x02 };
                                                        out_header[0..2].copy_from_slice(&proof_packet_id.to_be_bytes());
                                                        out_header[2] = flags;
                                                        
                                                        let enc_payload = crate::desktop_crypto::encrypt_btea_packet(
                                                            proof_packet_id, 0, flags, &out_header, handshakeidentityproof.as_bytes(),
                                                            &iv_struct, true
                                                        );
                                                        
                                                        let mut final_packet = Vec::with_capacity(8 + 3 + enc_payload.len() - 8);
                                                        final_packet.extend_from_slice(&enc_payload[0..8]);
                                                        final_packet.extend_from_slice(&out_header);
                                                        final_packet.extend_from_slice(&enc_payload[8..]);
                                                        
                                                        let _ = socket.send_to(&final_packet, addr).await;
                                                        println!("Sent handshakeidentityproof to {}: {}", addr, handshakeidentityproof);
                                                    }
                                                } else if payload_str.starts_with("handshakeidentityproof ") || payload_str.starts_with("handshakeindentityproof ") {
                                                    println!("Received handshakeidentityproof! Client signed the proof message.");
                                                    
                                                    if let Some(rc_idx) = payload_str.find("return_code=") {
                                                        let rc_val = payload_str[rc_idx + 12..].split_whitespace().next().unwrap_or("");
                                                        let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                                                        
                                                        let ack_packet_id = {
                                                            let mut lock = session.server_packet_id.lock().unwrap();
                                                            let val = *lock;
                                                            *lock = lock.wrapping_add(1);
                                                            val
                                                        };
                                                        let mut out_header = [0u8; 3];
                                                        let is_new_protocol = true;
                                                        let flags = if is_new_protocol { 0x22 } else { 0x02 };
                                                        out_header[0..2].copy_from_slice(&ack_packet_id.to_be_bytes());
                                                        out_header[2] = flags;
                                                        
                                                        let enc_payload = crate::desktop_crypto::encrypt_btea_packet(
                                                            ack_packet_id, 0, flags, &out_header, ack_cmd.as_bytes(),
                                                            &iv_struct, true
                                                        );
                                                        
                                                        let mut final_packet = Vec::with_capacity(8 + 3 + enc_payload.len() - 8);
                                                        final_packet.extend_from_slice(&enc_payload[0..8]);
                                                        final_packet.extend_from_slice(&out_header);
                                                        final_packet.extend_from_slice(&enc_payload[8..]);
                                                        
                                                        let _ = socket.send_to(&final_packet, addr).await;
                                                        println!("Sent ack to {}: {}", addr, ack_cmd);
                                                    }
                                                    
                                                    // (Send initserver here if needed, omitted for now)
                                                 } else {
                                                     // Fallback to ACK any other client commands by finding "rt-<number>"
                                                      if let Some(rt_idx) = payload_str.rfind("rt-") {
                                                          let mut end_idx = rt_idx + 3;
                                                          while end_idx < payload_str.len() {
                                                              let c = payload_str.as_bytes()[end_idx];
                                                              if c.is_ascii_digit() {
                                                                  end_idx += 1;
                                                              } else {
                                                                  break;
                                                              }
                                                          }
                                                          let rc_val = &payload_str[rt_idx..end_idx];
                                                           if rc_val.len() > 3 {
                                                               let is_rtc = session.pending_rtc_describe;
                                                               let is_join = payload_str.contains("broadcastvideojoin");
                                                               let is_candidate = payload_str.contains("rtcicecandidate");
                                                               
                                                               let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                                                               let ack_packet_id = {
                                                                   let mut lock = session.server_packet_id.lock().unwrap();
                                                                   let val = *lock;
                                                                   *lock = lock.wrapping_add(1);
                                                                   val
                                                               };
                                                               let mut out_header = [0u8; 3];
                                                               let flags = 0x22;
                                                               out_header[0..2].copy_from_slice(&ack_packet_id.to_be_bytes());
                                                               out_header[2] = flags;
                                                               
                                                               let enc_payload = crate::desktop_crypto::encrypt_btea_packet(
                                                                   ack_packet_id, 0, flags, &out_header, ack_cmd.as_bytes(),
                                                                   &iv_struct, true
                                                               );
                                                               
                                                               let mut final_packet = Vec::with_capacity(8 + 3 + enc_payload.len() - 8);
                                                               final_packet.extend_from_slice(&enc_payload[0..8]);
                                                               final_packet.extend_from_slice(&out_header);
                                                               final_packet.extend_from_slice(&enc_payload[8..]);
                                                               let _ = socket.send_to(&final_packet, addr).await;
                                                               println!("Sent fallback ACK to {}: {}", addr, ack_cmd);
                                                               
                                                               if is_rtc {
                                                                   session.pending_rtc_describe = false;
                                                                   let decompressed_buffer = decompress_if_needed(&session.rtc_describe_buffer);
                                                                   let full_sdp_offer = String::from_utf8_lossy(&decompressed_buffer).to_string();
                                                                   
                                                                   let ts3_unescape = |s: &str| -> String {
                                                                       s.replace("\\\\", "\\").replace("\\s", " ").replace("\\/", "/").replace("\\p", "|").replace("\\a", "\x07").replace("\\b", "\x08").replace("\\f", "\x0c").replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t").replace("\\v", "\x0b")
                                                                   };
                                                                   
                                                                   if let Some(sdp_idx) = full_sdp_offer.find("sdp=") {
                                                                       let sdp_substring = &full_sdp_offer[sdp_idx + 4..];
                                                                       let mut end_idx = 0;
                                                                       while end_idx < sdp_substring.len() {
                                                                           let c = sdp_substring.as_bytes()[end_idx];
                                                                           if c == b' ' {
                                                                               break;
                                                                           }
                                                                           end_idx += 1;
                                                                       }
                                                                       let raw_sdp_val = &sdp_substring[..end_idx];
                                                                       let unescaped_sdp = ts3_unescape(raw_sdp_val);
                                                                       
                                                                       println!("Parsed Client WebRTC SDP Offer:\n{}", unescaped_sdp);
                                                                       
                                                                       let rtc_manager = Arc::clone(&self.rtc_manager);
                                                                       let broadcaster_id = addr.port() as u32;
                                                                       let socket_clone = Arc::clone(&socket);
                                                                       let iv_struct_clone = iv_struct.clone();
                                                                       let addr_clone = addr;
                                                                       let server_packet_id_mutex = Arc::clone(&session.server_packet_id);
tokio::spawn(async move {
                                                                           let mut m = MediaEngine::default();
                                                                           let _ = m.register_default_codecs();
                                                                           
                                                                            let mut se = webrtc::api::setting_engine::SettingEngine::default();
                                                                            let _ = se.set_answering_dtls_role(DTLSRole::Server);
                                                                            
                                                                            let api = APIBuilder::new()
                                                                               .with_media_engine(m)
                                                                               .with_setting_engine(se)
                                                                               .build();
                                                                           
                                                                           let config = RTCConfiguration {
                                                                               ..Default::default()
                                                                           };
                                                                           
                                                                           if let Ok(pc) = api.new_peer_connection(config).await {
                                                                               let pc = Arc::new(pc);
                                                                               
                                                                               // Register connection in the shared map IMMEDIATELY so we can receive ICE candidates
                                                                               rtc_manager.register_connection(addr_clone, Arc::clone(&pc));
                                                                               
                                                                               // Register connection state callbacks for troubleshooting
                                                                               let addr_ice_state = addr_clone;
                                                                               pc.on_ice_connection_state_change(Box::new(move |state| {
                                                                                   println!("WebRTC ICE Connection State Changed for {}: {:?}", addr_ice_state, state);
                                                                                   Box::pin(async move {})
                                                                               }));
                                                                               
                                                                               let addr_pc_state = addr_clone;
                                                                               pc.on_peer_connection_state_change(Box::new(move |state| {
                                                                                   println!("WebRTC Peer Connection State Changed for {}: {:?}", addr_pc_state, state);
                                                                                   Box::pin(async move {})
                                                                               }));
                                                                               
                                                                               // Register track publisher
                                                                               let rtc_mgr = Arc::clone(&rtc_manager);
                                                                                pc.on_track(Box::new(move |track, _receiver, _transceiver| {
                                                                                   let rtc_mgr = Arc::clone(&rtc_mgr);
                                                                                   let track = Arc::clone(&track);
                                                                                   Box::pin(async move {
                                                                                       println!("Received WebRTC video track from broadcaster {}", broadcaster_id);
                                                                                       let mime = track.codec().capability.mime_type.clone();
                                                                                       let local_track = rtc_mgr.register_broadcast(broadcaster_id, mime);
                                                                                       
                                                                                       while let Ok((packet, _)) = track.read_rtp().await {
                                                                                           let _ = local_track.write_rtp(&packet).await;
                                                                                       }
                                                                                       
                                                                                       rtc_mgr.remove_broadcast(broadcaster_id);
                                                                                       println!("Broadcaster {} stopped video stream", broadcaster_id);
                                                                                   })
                                                                               }));
                                                                               
                                                                                // Set remote offer
                                                                                if let Ok(offer) = RTCSessionDescription::offer(unescaped_sdp) {
                                                                                    if let Ok(_) = pc.set_remote_description(offer).await {
                                                                                        // Create answer
                                                                                        if let Ok(answer) = pc.create_answer(None).await {
                                                                                            // Setup ICE gathering completion signal
                                                                                            let (ice_tx, mut ice_rx) = tokio::sync::mpsc::channel::<()>(1);
                                                                                            pc.on_ice_gathering_state_change(Box::new(move |state| {
                                                                                                 if state == RTCIceGathererState::Complete {
                                                                                                    let _ = ice_tx.try_send(());
                                                                                                }
                                                                                                Box::pin(async move {})
                                                                                            }));
                                                                                            
                                                                                            if let Ok(_) = pc.set_local_description(answer).await {
                                                                                                // Wait for ICE candidate gathering
                                                                                                let _ = tokio::time::timeout(std::time::Duration::from_millis(200), ice_rx.recv()).await;
                                                                                                
                                                                                                 if let Some(local_sdp) = pc.local_description().await {
                                                                                                     let mut sdp_str = local_sdp.sdp.clone();
                                                                                                     sdp_str = sdp_str.replace("a=setup:active", "a=setup:passive");
                                                                                                     println!("Generated Local WebRTC SDP Answer (DTLS setup modified to passive):\n{}", sdp_str);
                                                                                                    let ts3_escape = |s: &str| -> String {
                                                                                                        s.replace("\\", "\\\\").replace(" ", "\\s").replace("/", "\\/").replace("|", "\\p").replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t").replace("\x0b", "\\v")
                                                                                                    };
                                                                                                    let escaped_answer_sdp = ts3_escape(&sdp_str);
                                                                                                    let notify_cmd = format!("notifyrtcsessiondescription mode=answer sdp={} compression=0", escaped_answer_sdp);
                                                                                                    
                                                                                                    let payload_bytes = notify_cmd.as_bytes();
                                                                                                    let chunk_size = 400; // TS3 fragment limit
                                                                                                    let chunks: Vec<&[u8]> = payload_bytes.chunks(chunk_size).collect();
                                                                                                    let total_chunks = chunks.len();
                                                                                                    
                                                                                                    for (i, chunk) in chunks.iter().enumerate() {
                                                                                                        let mut flags = 0x22;
                                                                                                        if total_chunks > 1 && (i == 0 || i == total_chunks - 1) {
                                                                                                            flags |= 0x10; // Fragmented
                                                                                                        }
                                                                                                        let out_packet_id = {
                                                                                                            let mut lock = server_packet_id_mutex.lock().unwrap();
                                                                                                            let val = *lock;
                                                                                                            *lock = lock.wrapping_add(1);
                                                                                                            val
                                                                                                        };
                                                                                                        let mut out_header = [0u8; 3];
                                                                                                        out_header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
                                                                                                        out_header[2] = flags;
                                                                                                        
                                                                                                        let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
                                                                                                            out_packet_id, 0, flags, &out_header, chunk,
                                                                                                            &iv_struct_clone, true
                                                                                                        );
                                                                                                        let mut final_packet = Vec::with_capacity(8 + 3 + encrypted_out.len() - 8);
                                                                                                        final_packet.extend_from_slice(&encrypted_out[0..8]);
                                                                                                        final_packet.extend_from_slice(&out_header);
                                                                                                        final_packet.extend_from_slice(&encrypted_out[8..]);
                                                                                                        
                                                                                                        let _ = socket_clone.send_to(&final_packet, addr_clone).await;
                                                                                                    }
                                                                                                    println!("Sent fragmented notifyrtcsessiondescription answer to {}", addr_clone);
                                                                                                }
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                }
                                                                           }
                                                                       });
                                                                   }
                                                                   session.rtc_describe_buffer.clear();
                                                               }
                                                               
                                                               if is_candidate {
                                                                   let ts3_unescape = |s: &str| -> String {
                                                                       s.replace("\\\\", "\\").replace("\\s", " ").replace("\\/", "/").replace("\\p", "|").replace("\\a", "\x07").replace("\\b", "\x08").replace("\\f", "\x0c").replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t").replace("\\v", "\x0b")
                                                                   };
                                                                   let mut sdp_mline_index = None;
                                                                   if let Some(ml_idx) = payload_str.find("media_line=") {
                                                                       let ml_substring = &payload_str[ml_idx + 11..];
                                                                       let mut end_idx = 0;
                                                                       while end_idx < ml_substring.len() {
                                                                           let c = ml_substring.as_bytes()[end_idx];
                                                                           if c.is_ascii_digit() {
                                                                               end_idx += 1;
                                                                           } else {
                                                                               break;
                                                                           }
                                                                       }
                                                                       if let Ok(ml_val) = ml_substring[..end_idx].parse::<u16>() {
                                                                           sdp_mline_index = Some(ml_val);
                                                                       }
                                                                   }
                                                                   if let Some(cand_idx) = payload_str.find("candidate=") {
                                                                       let cand_substring = &payload_str[cand_idx + 10..];
                                                                       let mut end_idx = 0;
                                                                       while end_idx < cand_substring.len() {
                                                                           let c = cand_substring.as_bytes()[end_idx];
                                                                           if c == b' ' {
                                                                               break;
                                                                           }
                                                                           end_idx += 1;
                                                                       }
                                                                       let raw_cand_val = &cand_substring[..end_idx];
                                                                       let unescaped_cand = ts3_unescape(raw_cand_val);
                                                                       
                                                                        let rtc_mgr = Arc::clone(&self.rtc_manager);
                                                                        let addr_clone = addr;
                                                                        let cand_clone = unescaped_cand.clone();
                                                                        tokio::spawn(async move {
                                                                            let mut found_pc = None;
                                                                            for _ in 0..10 {
                                                                                if let Some(pc) = rtc_mgr.get_connection(addr_clone) {
                                                                                    found_pc = Some(pc);
                                                                                    break;
                                                                                }
                                                                                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                                                                            }
                                                                            if let Some(pc) = found_pc {
                                                                                println!("Adding client ICE candidate for {} (m-line index {:?}): {}", addr_clone, sdp_mline_index, cand_clone);
                                                                                match pc.add_ice_candidate(RTCIceCandidateInit {
                                                                                    candidate: cand_clone,
                                                                                    sdp_mline_index,
                                                                                    ..Default::default()
                                                                                }).await {
                                                                                    Ok(_) => println!("Successfully added ICE candidate for {}", addr_clone),
                                                                                    Err(e) => println!("Failed to add ICE candidate for {}: {:?}", addr_clone, e),
                                                                                }
                                                                            } else {
                                                                                println!("WARNING: Discarded client ICE candidate for unknown connection: {}", addr_clone);
                                                                            }
                                                                         });
                                                                    }
                                                               }
                                                               
                                                               if is_join {
                                                                   let mut target_bid = 0u32;
                                                                   if let Some(bid_idx) = payload_str.find("bid=") {
                                                                       let bid_substring = &payload_str[bid_idx + 4..];
                                                                       let mut end_idx = 0;
                                                                       while end_idx < bid_substring.len() {
                                                                           let c = bid_substring.as_bytes()[end_idx];
                                                                           if c.is_ascii_digit() {
                                                                               end_idx += 1;
                                                                           } else {
                                                                               break;
                                                                           }
                                                                       }
                                                                       if let Ok(bid_val) = bid_substring[..end_idx].parse::<u32>() {
                                                                           target_bid = bid_val;
                                                                       }
                                                                   }
                                                                   
                                                                   if target_bid != 0 {
                                                                       if let Some(pc) = self.rtc_manager.get_connection(addr) {
                                                                           if let Some(track) = self.rtc_manager.get_broadcast(target_bid) {
                                                                               let pc_clone = Arc::clone(&pc);
                                                                               let track_clone = Arc::clone(&track);
                                                                               tokio::spawn(async move {
                                                                                   let _ = pc_clone.add_track(track_clone).await;
                                                                                   println!("Added broadcaster video track {} to listener {}", target_bid, addr);
                                                                               });
                                                                           }
                                                                       }
                                                                   }
                                                               }
                                                           }
                                                      }
                                                 }
                                            } else {
                                                println!("teaspeak udp: failed to decrypt packet from {} (flags={:02X}, id={})", addr, flags, packet_id);
                                            }
                                        } else {
                                            println!("teaspeak udp: received encrypted packet from unknown session {}", addr);
                                        }
                                    }
                                    _ => {}
                                }
                            } else {
                                println!("teaspeak udp: received non-init packet from {} (len={})", addr, size);
                                println!("teaspeak udp: RAW NON-INIT = {:02X?}", packet);
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {
                        // Background tasks
                    }
                }
            }
            Ok(())
        })
    }
}
