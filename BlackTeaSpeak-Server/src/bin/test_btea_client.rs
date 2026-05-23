use x25519_dalek::{EphemeralSecret, PublicKey};
use rand::rngs::OsRng;
use sha2::{Sha512, Digest};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "127.0.0.1:9988";
    let mut socket = TcpStream::connect(server_addr).await?;

    println!("Connecting to BTEA server at {}", server_addr);

    // 1. Generate client X25519 keypair
    let client_secret = EphemeralSecret::random_from_rng(OsRng);
    let client_public = PublicKey::from(&client_secret);

    // 2. Build BtInitRequest
    let mut request = [0u8; 41];
    request[0..8].copy_from_slice(b"BTEAINIT");
    request[8] = 0x01; // Request
    request[9..41].copy_from_slice(client_public.as_bytes());

    let mut framed_request = Vec::with_capacity(4 + request.len());
    framed_request.extend_from_slice(&(request.len() as u32).to_be_bytes());
    framed_request.extend_from_slice(&request);

    // 3. Send Request
    socket.write_all(&framed_request).await?;
    println!("Sent BtInitRequest");

    // 4. Receive Response
    let mut len_buf = [0u8; 4];
    let len_bytes = tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut len_buf)).await??;
    let len = u32::from_be_bytes(len_buf) as usize;
    
    let mut buf = [0u8; 1024];
    socket.read_exact(&mut buf[0..len]).await?;
    
    if len != 41 || &buf[0..8] != b"BTEAINIT" || buf[8] != 0x02 {
        println!("Invalid response received: len={}, type={}", len, buf[8]);
        return Ok(());
    }

    let mut server_public_bytes = [0u8; 32];
    server_public_bytes.copy_from_slice(&buf[9..41]);
    let server_public = PublicKey::from(server_public_bytes);

    println!("Received BtInitResponse");

    // 5. Compute shared keys
    let shared_secret = client_secret.diffie_hellman(&server_public);

    let mut hasher1 = Sha512::new();
    hasher1.update(b"BTEA_KEY_DERIVATION");
    hasher1.update(shared_secret.as_bytes());
    let iv_struct = hasher1.finalize().to_vec();

    let mut hasher2 = Sha512::new();
    hasher2.update(b"BTEA_SECRET_DERIVATION");
    hasher2.update(shared_secret.as_bytes());
    let session_shared_secret = hasher2.finalize().to_vec();

    println!("Successfully computed session keys!");

    // 6. Test Encryption by sending a fake clientinit packet
    let payload = b"clientinit client_version=BTEA_TEST";
    
    let packet_id: u16 = 1;
    let client_id: u16 = 0;
    let flags: u8 = 0x02; // Command packet
    
    let mut header = [0u8; 5];
    header[0..2].copy_from_slice(&packet_id.to_be_bytes());
    header[2..4].copy_from_slice(&client_id.to_be_bytes());
    header[4] = flags;
    
    let encrypted_out = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        packet_id,
        0,
        flags,
        &header,
        payload,
        &session_shared_secret,
        false, // client to server
    );
    
    let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
    final_packet.extend_from_slice(&encrypted_out[0..8]); // mac
    final_packet.extend_from_slice(&header);
    final_packet.extend_from_slice(&encrypted_out[8..]); // ciphertext
    
    let packet_len = (final_packet.len() as u32).to_be_bytes();
    let mut framed_packet = Vec::with_capacity(4 + final_packet.len());
    framed_packet.extend_from_slice(&packet_len);
    framed_packet.extend_from_slice(&final_packet);
    
    socket.write_all(&framed_packet).await?;
    println!("Sent encrypted clientinit command!");
    
    // 7. Wait for response fragments
    let mut fragment_buffer = Vec::new();
    loop {
        let mut resp_buf = [0u8; 4096];
        let len = match tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut resp_buf[0..4])).await {
            Ok(Ok(_)) => u32::from_be_bytes([resp_buf[0], resp_buf[1], resp_buf[2], resp_buf[3]]) as usize,
            _ => {
                println!("Timeout waiting for response length.");
                break;
            }
        };
        
        if len > 4096 { break; }
        socket.read_exact(&mut resp_buf[0..len]).await.unwrap();
        
        let packet = &resp_buf[..len];
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
            
            let decrypted = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
                res_packet_id,
                0,
                res_flags,
                &res_header,
                &payload_with_mac,
                &session_shared_secret,
                true, // server to client
            );
            
            if let Some(mut dec) = decrypted {
                let is_fragmented = (res_flags & 0x10) != 0;
                fragment_buffer.append(&mut dec);
                
                if !is_fragmented {
                    let full_payload = std::mem::take(&mut fragment_buffer);
                    if let Ok(cmd) = String::from_utf8(full_payload) {
                        println!("Server Response (Init): {}", cmd);
                        break;
                    }
                }
            } else {
                println!("Failed to decrypt server response!");
            }
        }
    }

    // 8. Test whoami command
    let payload_whoami = b"whoami";
    let packet_id_whoami: u16 = 2;
    let mut header_whoami = [0u8; 5];
    header_whoami[0..2].copy_from_slice(&packet_id_whoami.to_be_bytes());
    header_whoami[2..4].copy_from_slice(&client_id.to_be_bytes());
    header_whoami[4] = flags;
    
    let enc_whoami = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        packet_id_whoami, 0, flags, &header_whoami, payload_whoami, &session_shared_secret, false
    );
    let mut final_whoami = Vec::with_capacity(13 + enc_whoami.len() - 8);
    final_whoami.extend_from_slice(&enc_whoami[0..8]);
    final_whoami.extend_from_slice(&header_whoami);
    final_whoami.extend_from_slice(&enc_whoami[8..]);
    
    let packet_len_whoami = (final_whoami.len() as u32).to_be_bytes();
    let mut framed_whoami = Vec::with_capacity(4 + final_whoami.len());
    framed_whoami.extend_from_slice(&packet_len_whoami);
    framed_whoami.extend_from_slice(&final_whoami);
    
    socket.write_all(&framed_whoami).await?;
    println!("Sent encrypted whoami command!");
    
    let mut resp_buf = [0u8; 4096];
    if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut resp_buf[0..4])).await {
        let len = u32::from_be_bytes([resp_buf[0], resp_buf[1], resp_buf[2], resp_buf[3]]) as usize;
        socket.read_exact(&mut resp_buf[0..len]).await.unwrap();
        let packet = &resp_buf[..len];
        let res_packet_id = u16::from_be_bytes([packet[8], packet[9]]);
        let mut p_with_mac = Vec::new();
        p_with_mac.extend_from_slice(&packet[0..8]);
        p_with_mac.extend_from_slice(&packet[13..]);
        if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
            res_packet_id, 0, packet[12], &packet[8..13].to_vec(), &p_with_mac, &session_shared_secret, true
        ) {
            println!("Server Response (whoami): {}", String::from_utf8_lossy(&dec));
        }
    }

    // 9. Test clientmove command
    let payload_move = b"clientmove cid=2"; // moving to channel 2
    let packet_id_move: u16 = 3;
    let mut header_move = [0u8; 5];
    header_move[0..2].copy_from_slice(&packet_id_move.to_be_bytes());
    header_move[2..4].copy_from_slice(&client_id.to_be_bytes());
    header_move[4] = flags;
    
    let enc_move = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        packet_id_move, 0, flags, &header_move, payload_move, &session_shared_secret, false
    );
    let mut final_move = Vec::with_capacity(13 + enc_move.len() - 8);
    final_move.extend_from_slice(&enc_move[0..8]);
    final_move.extend_from_slice(&header_move);
    final_move.extend_from_slice(&enc_move[8..]);
    
    let packet_len_move = (final_move.len() as u32).to_be_bytes();
    let mut framed_move = Vec::with_capacity(4 + final_move.len());
    framed_move.extend_from_slice(&packet_len_move);
    framed_move.extend_from_slice(&final_move);
    
    socket.write_all(&framed_move).await?;
    println!("Sent encrypted clientmove command!");
    
    if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut resp_buf[0..4])).await {
        let len = u32::from_be_bytes([resp_buf[0], resp_buf[1], resp_buf[2], resp_buf[3]]) as usize;
        socket.read_exact(&mut resp_buf[0..len]).await.unwrap();
        let packet = &resp_buf[..len];
        let res_packet_id = u16::from_be_bytes([packet[8], packet[9]]);
        let mut p_with_mac = Vec::new();
        p_with_mac.extend_from_slice(&packet[0..8]);
        p_with_mac.extend_from_slice(&packet[13..]);
        if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
            res_packet_id, 0, packet[12], &packet[8..13].to_vec(), &p_with_mac, &session_shared_secret, true
        ) {
            println!("Server Response (clientmove): {}", String::from_utf8_lossy(&dec));
        }
    }

    // 10. Test clientupdate (Mute Mic)
    let payload_update = b"clientupdate client_input_muted=1";
    let packet_id_update: u16 = 4;
    let mut header_update = [0u8; 5];
    header_update[0..2].copy_from_slice(&packet_id_update.to_be_bytes());
    header_update[2..4].copy_from_slice(&client_id.to_be_bytes());
    header_update[4] = flags;
    
    let enc_update = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        packet_id_update, 0, flags, &header_update, payload_update, &session_shared_secret, false
    );
    let mut final_update = Vec::with_capacity(13 + enc_update.len() - 8);
    final_update.extend_from_slice(&enc_update[0..8]);
    final_update.extend_from_slice(&header_update);
    final_update.extend_from_slice(&enc_update[8..]);
    
    let packet_len_update = (final_update.len() as u32).to_be_bytes();
    let mut framed_update = Vec::with_capacity(4 + final_update.len());
    framed_update.extend_from_slice(&packet_len_update);
    framed_update.extend_from_slice(&final_update);
    
    socket.write_all(&framed_update).await?;
    println!("Sent encrypted clientupdate command!");
    
    if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut resp_buf[0..4])).await {
        let len = u32::from_be_bytes([resp_buf[0], resp_buf[1], resp_buf[2], resp_buf[3]]) as usize;
        socket.read_exact(&mut resp_buf[0..len]).await.unwrap();
        let packet = &resp_buf[..len];
        let res_packet_id = u16::from_be_bytes([packet[8], packet[9]]);
        let mut p_with_mac = Vec::new();
        p_with_mac.extend_from_slice(&packet[0..8]);
        p_with_mac.extend_from_slice(&packet[13..]);
        if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
            res_packet_id, 0, packet[12], &packet[8..13].to_vec(), &p_with_mac, &session_shared_secret, true
        ) {
            println!("Server Response (clientupdate): {}", String::from_utf8_lossy(&dec));
        }
    }

    // 11. Test sendtextmessage
    let payload_chat = b"sendtextmessage targetmode=2 target=1 msg=Hello\\sWorld";
    let packet_id_chat: u16 = 5;
    let mut header_chat = [0u8; 5];
    header_chat[0..2].copy_from_slice(&packet_id_chat.to_be_bytes());
    header_chat[2..4].copy_from_slice(&client_id.to_be_bytes());
    header_chat[4] = flags;
    
    let enc_chat = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        packet_id_chat, 0, flags, &header_chat, payload_chat, &session_shared_secret, false
    );
    let mut final_chat = Vec::with_capacity(13 + enc_chat.len() - 8);
    final_chat.extend_from_slice(&enc_chat[0..8]);
    final_chat.extend_from_slice(&header_chat);
    final_chat.extend_from_slice(&enc_chat[8..]);
    
    let packet_len_chat = (final_chat.len() as u32).to_be_bytes();
    let mut framed_chat = Vec::with_capacity(4 + final_chat.len());
    framed_chat.extend_from_slice(&packet_len_chat);
    framed_chat.extend_from_slice(&final_chat);
    
    socket.write_all(&framed_chat).await?;
    println!("Sent encrypted sendtextmessage command!");
    
    if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut resp_buf[0..4])).await {
        let len = u32::from_be_bytes([resp_buf[0], resp_buf[1], resp_buf[2], resp_buf[3]]) as usize;
        socket.read_exact(&mut resp_buf[0..len]).await.unwrap();
        let packet = &resp_buf[..len];
        let res_packet_id = u16::from_be_bytes([packet[8], packet[9]]);
        let mut p_with_mac = Vec::new();
        p_with_mac.extend_from_slice(&packet[0..8]);
        p_with_mac.extend_from_slice(&packet[13..]);
        if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
            res_packet_id, 0, packet[12], &packet[8..13].to_vec(), &p_with_mac, &session_shared_secret, true
        ) {
            println!("Server Response (sendtextmessage): {}", String::from_utf8_lossy(&dec));
        }
    }

    println!("BTEA Protocol Test Client Finished Successfully!");

    Ok(())
}
