use crate::btea::AppState;
use tauri::State;
use rand::rngs::OsRng;
use sha2::{Digest, Sha512};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{UdpSocket, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};

#[tauri::command]
pub async fn connect_to_server(
    address: String,
    nickname: String,
    identity_public_key: Option<String>,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let (ip, port_str) = address.split_once(':').unwrap_or((&address, "9987"));
    let tcp_port = port_str;
    let tcp_address = format!("{}:{}", ip, tcp_port);
    let udp_address = format!("{}:{}", ip, port_str);

    let mut tcp_stream = match TcpStream::connect(&tcp_address).await {
        Ok(s) => s,
        Err(e) => return Err(format!("Failed to connect to server TCP {}: {}", tcp_address, e)),
    };

    let client_secret = EphemeralSecret::random_from_rng(OsRng);
    let client_public = X25519PublicKey::from(&client_secret);

    let mut request = [0u8; 45];
    request[0..4].copy_from_slice(&41u32.to_be_bytes()); // length
    request[4..12].copy_from_slice(b"BTEAINIT");
    request[12] = 0x01; // Request
    request[13..45].copy_from_slice(client_public.as_bytes());

    if let Err(e) = tcp_stream.write_all(&request).await {
        return Err(format!("Failed to send init request: {}", e));
    }

    let mut len_buf = [0u8; 4];
    match tokio::time::timeout(Duration::from_secs(5), tcp_stream.read_exact(&mut len_buf)).await {
        Ok(Ok(_)) => {},
        _ => return Err("Connection timed out waiting for server response".into()),
    };
    
    let len = u32::from_be_bytes(len_buf) as usize;
    if len != 41 {
        return Err(format!("Invalid response received from server (len: {})", len));
    }
    
    let mut buf = [0u8; 41];
    if let Err(e) = tcp_stream.read_exact(&mut buf).await {
        return Err(format!("Failed to read response: {}", e));
    }

    if &buf[0..8] != b"BTEAINIT" || buf[8] != 0x02 {
        return Err("Invalid response received from server".into());
    }

    let mut server_public_bytes = [0u8; 32];
    server_public_bytes.copy_from_slice(&buf[9..41]);
    let server_public = X25519PublicKey::from(server_public_bytes);

    let shared_secret = client_secret.diffie_hellman(&server_public);

    let mut hasher2 = Sha512::new();
    hasher2.update(b"BTEA_SECRET_DERIVATION");
    hasher2.update(shared_secret.as_bytes());
    let session_shared_secret = hasher2.finalize().to_vec();

    let (mut rx_tcp, tx_tcp) = tcp_stream.into_split();
    let tx_tcp_arc = Arc::new(tokio::sync::Mutex::new((tx_tcp, 1u16, session_shared_secret.clone())));

    // Send a clientinit command over TCP
    let mut payload = format!("clientinit client_nickname={} client_version=BTEA_TEST_CLIENT", nickname);
    if let Some(pubkey) = identity_public_key {
        payload.push_str(&format!(" client_publickey={}", pubkey));
    }
    
    let payload_bytes = payload.as_bytes();
    {
        let mut tx_lock = tx_tcp_arc.lock().await;
        let packet_id = tx_lock.1;
        tx_lock.1 = tx_lock.1.wrapping_add(1);
        let secret = tx_lock.2.clone();
        
        let mut header = [0u8; 5];
        header[0..2].copy_from_slice(&packet_id.to_be_bytes());
        header[2..4].copy_from_slice(&0u16.to_be_bytes()); // client_id
        header[4] = 0x02; // Command packet
        
        let encrypted_out = crate::btea::encrypt_btea_packet(
            packet_id, 0, 0x02, &header, payload_bytes, &secret, false
        )?;
        
        let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
        final_packet.extend_from_slice(&encrypted_out[0..8]);
        final_packet.extend_from_slice(&header);
        final_packet.extend_from_slice(&encrypted_out[8..]);
        
        let packet_len = (final_packet.len() as u32).to_be_bytes();
        let mut packet = Vec::with_capacity(4 + final_packet.len());
        packet.extend_from_slice(&packet_len);
        packet.extend_from_slice(&final_packet);
        
        if let Err(e) = tx_lock.0.write_all(&packet).await {
            return Err(format!("Failed to send clientinit: {}", e));
        }
    }

    // Wait for response over TCP
    if let Err(e) = rx_tcp.read_exact(&mut len_buf).await {
        return Err(format!("Failed to read response length: {}", e));
    }
    let res_len = u32::from_be_bytes(len_buf) as usize;
    if res_len > 65536 || res_len < 13 {
        return Err("Response size invalid".into());
    }
    
    let mut resp_buf = vec![0u8; res_len];
    if let Err(e) = rx_tcp.read_exact(&mut resp_buf).await {
        return Err(format!("Failed to read response: {}", e));
    }
    
    let mut cmd = String::new();
    
    let mut mac = [0u8; 8];
    mac.copy_from_slice(&resp_buf[0..8]);
    let packet_id = u16::from_be_bytes([resp_buf[8], resp_buf[9]]);
    let flags = resp_buf[12];
    let payload_enc = &resp_buf[13..];
    
    let header = resp_buf[8..13].to_vec();
    let mut payload_with_mac = Vec::with_capacity(8 + payload_enc.len());
    payload_with_mac.extend_from_slice(&mac);
    payload_with_mac.extend_from_slice(payload_enc);
    
    if let Some(decrypted) = crate::btea::decrypt_btea_packet(
        packet_id, 0, flags, &header, &payload_with_mac, &session_shared_secret, true
    ) {
        if let Ok(c) = String::from_utf8(decrypted) {
            cmd = c;
        }
    }

    let udp_socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => return Err(format!("Failed to bind UDP socket: {}", e)),
    };
    if let Err(e) = udp_socket.connect(&udp_address).await {
        return Err(format!("Failed to connect UDP socket to server {}: {}", udp_address, e));
    }

    let shared_secret_clone = session_shared_secret.clone();
    let socket_arc = Arc::new(udp_socket);
    
    if let Ok((am, mut rx_opus_out)) = crate::audio::AudioManager::new(Some(app_handle.clone())) {
        let tx_opus_in = am.tx_opus_in.clone();
        
        *state.audio_manager.lock().await = Some(am);
        *state.session_secret.lock().await = Some(session_shared_secret.clone());
        *state.socket.lock().await = Some(socket_arc.clone());
        *state.tcp_writer.lock().await = Some(tx_tcp_arc.clone());

        let secret_recv_tcp = session_shared_secret.clone();
        // TCP Receiver Loop
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            loop {
                if rx_tcp.read_exact(&mut len_buf).await.is_err() { break; }
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > 65536 || len < 13 { break; }
                let mut buf = vec![0u8; len];
                if rx_tcp.read_exact(&mut buf).await.is_err() { break; }
                
                let mut mac = [0u8; 8];
                mac.copy_from_slice(&buf[0..8]);
                let packet_id = u16::from_be_bytes([buf[8], buf[9]]);
                let flags = buf[12];
                let payload_enc = &buf[13..];
                
                let header = buf[8..13].to_vec();
                let mut payload_with_mac = Vec::with_capacity(8 + payload_enc.len());
                payload_with_mac.extend_from_slice(&mac);
                payload_with_mac.extend_from_slice(payload_enc);
                
                if let Some(decrypted) = crate::btea::decrypt_btea_packet(
                    packet_id, 0, flags, &header, &payload_with_mac, &secret_recv_tcp, true
                ) {
                    if let Ok(cmd_str) = String::from_utf8(decrypted) {
                        if cmd_str != "error id=0 msg=ok" {
                            println!("RECEIVED EVENT: {}", cmd_str);
                        }
                        use tauri::Emitter;
                        let _ = app_handle.emit("server_event", cmd_str);
                    }
                }
            }
            use tauri::Emitter;
            let _ = app_handle.emit("server_disconnect", ());
        });

        let socket_recv = socket_arc.clone();
        let secret_recv = shared_secret_clone.clone();
        // UDP Receiver Loop (for Voice)
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                if let Ok(len) = socket_recv.recv(&mut buf).await {
                    let packet = &buf[..len];
                    if packet.len() > 13 {
                        let mut mac = [0u8; 8];
                        mac.copy_from_slice(&packet[0..8]);
                        let res_packet_id = u16::from_be_bytes([packet[8], packet[9]]);
                        let res_flags = packet[12];
                        let res_payload = &packet[13..];
                        
                        let res_header = packet[8..13].to_vec();
                        let mut payload_with_mac = Vec::with_capacity(8 + res_payload.len());
                        payload_with_mac.extend_from_slice(&mac);
                        payload_with_mac.extend_from_slice(res_payload);
                        
                        if let Some(decrypted) = crate::btea::decrypt_btea_packet(
                            res_packet_id, 0, res_flags, &res_header, &payload_with_mac, &secret_recv, true
                        ) {
                            let packet_type = res_flags & 0x0F;
                            let sender_client_id = u16::from_be_bytes([res_header[2], res_header[3]]);
                            if packet_type == 0x0A || packet_type == 0x01 { // Voice or Whisper
                                let _ = tx_opus_in.send((sender_client_id, decrypted));
                            }
                        }
                    }
                }
            }
        });

        let socket_send = socket_arc.clone();
        let secret_send = shared_secret_clone;
        // UDP Sender Loop (for Voice and Keepalive)
        tokio::spawn(async move {
            let mut next_packet_id: u16 = 4; 
            loop {
                let (flags, payload_bytes) = match tokio::time::timeout(std::time::Duration::from_secs(15), rx_opus_out.recv()).await {
                    Ok(Some((is_whisper, opus_data))) => (if is_whisper { 0x0B } else { 0x0A }, opus_data),
                    Ok(None) => break, // Channel closed
                    Err(_) => {
                        // Timeout: Send keepalive packet
                        (0x00, vec![])
                    }
                };

                let mut header = [0u8; 5];
                header[0..2].copy_from_slice(&next_packet_id.to_be_bytes());
                header[2..4].copy_from_slice(&0u16.to_be_bytes()); // client_id not needed for outbound
                header[4] = flags;

                let encrypted = match crate::btea::encrypt_btea_packet(
                    next_packet_id, 0, flags, &header, &payload_bytes, &secret_send, false
                ) {
                    Ok(e) => e,
                    Err(err) => {
                        println!("Failed to encrypt UDP packet: {}", err);
                        continue;
                    }
                };

                let mut final_packet = Vec::with_capacity(13 + encrypted.len() - 8);
                final_packet.extend_from_slice(&encrypted[0..8]);
                final_packet.extend_from_slice(&header);
                final_packet.extend_from_slice(&encrypted[8..]);
                
                let _ = socket_send.send(&final_packet).await;
                next_packet_id = next_packet_id.wrapping_add(1);
            }
        });
        
        return Ok(format!("Connected Successfully! Server response: {}", cmd));
    } else {
        return Err("Connected but failed to initialize audio devices".into());
    }
}

#[tauri::command]
pub async fn send_command(command: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(writer) = state.tcp_writer.lock().await.as_ref() {
        let payload = command.as_bytes();
        let mut tx_lock = writer.lock().await;
        let packet_id = tx_lock.1;
        tx_lock.1 = tx_lock.1.wrapping_add(1);
        let secret = tx_lock.2.clone();
        
        let mut header = [0u8; 5];
        header[0..2].copy_from_slice(&packet_id.to_be_bytes());
        header[2..4].copy_from_slice(&0u16.to_be_bytes());
        header[4] = 0x02;
        
        let encrypted_out = crate::btea::encrypt_btea_packet(
            packet_id, 0, 0x02, &header, payload, &secret, false
        )?;
        let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
        final_packet.extend_from_slice(&encrypted_out[0..8]);
        final_packet.extend_from_slice(&header);
        final_packet.extend_from_slice(&encrypted_out[8..]);
        
        let packet_len = (final_packet.len() as u32).to_be_bytes();
        let mut packet = Vec::with_capacity(4 + final_packet.len());
        packet.extend_from_slice(&packet_len);
        packet.extend_from_slice(&final_packet);
        
        let _ = tx_lock.0.write_all(&packet).await;
        Ok(())
    } else {
        Err("Not connected".into())
    }
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(writer) = state.tcp_writer.lock().await.as_ref() {
        let payload = b"quit";
        let mut tx_lock = writer.lock().await;
        let packet_id = tx_lock.1;
        tx_lock.1 = tx_lock.1.wrapping_add(1);
        let secret = tx_lock.2.clone();
        
        let mut header = [0u8; 5];
        header[0..2].copy_from_slice(&packet_id.to_be_bytes());
        header[2..4].copy_from_slice(&0u16.to_be_bytes());
        header[4] = 0x02;
        
        let encrypted_out = crate::btea::encrypt_btea_packet(
            packet_id, 0, 0x02, &header, payload, &secret, false
        )?;
        let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
        final_packet.extend_from_slice(&encrypted_out[0..8]);
        final_packet.extend_from_slice(&header);
        final_packet.extend_from_slice(&encrypted_out[8..]);
        
        let packet_len = (final_packet.len() as u32).to_be_bytes();
        let mut packet = Vec::with_capacity(4 + final_packet.len());
        packet.extend_from_slice(&packet_len);
        packet.extend_from_slice(&final_packet);
        
        let _ = tx_lock.0.write_all(&packet).await;
    }

    *state.audio_manager.lock().await = None;
    *state.session_secret.lock().await = None;
    *state.socket.lock().await = None;
    *state.tcp_writer.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn toggle_microphone(muted: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut am_lock = state.audio_manager.lock().await;
    if let Some(am) = am_lock.as_mut() {
        am.is_mic_muted.store(muted, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    } else {
        Err("Not connected".into())
    }
}

#[tauri::command]
pub async fn toggle_speaker(muted: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut am_lock = state.audio_manager.lock().await;
    if let Some(am) = am_lock.as_mut() {
        am.is_speaker_muted.store(muted, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    } else {
        Err("Not connected".into())
    }
}

#[tauri::command]
pub async fn set_ptt_state(active: bool, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(am) = &*state.audio_manager.lock().await {
        am.is_ptt_pressed.store(active, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_client_volume(client_id: u16, volume: f32, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(am) = &*state.audio_manager.lock().await {
        am.client_volumes.lock().unwrap().insert(client_id, volume);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_whisper_state(active: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut am_lock = state.audio_manager.lock().await;
    if let Some(am) = am_lock.as_mut() {
        am.is_whisper_active.store(active, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    } else {
        Err("Not connected".into())
    }
}
