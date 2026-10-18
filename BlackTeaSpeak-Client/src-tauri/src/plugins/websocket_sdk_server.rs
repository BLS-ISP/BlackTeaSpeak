use base64::Engine;
use sha1::{Digest, Sha1};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::sdk_models::{
    DEFAULT_PLUGIN_SDK_BIND, PluginSdkRequest, PluginSdkResponse, PluginSdkSharedState,
};

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn spawn_plugin_sdk_server(
    sdk_state: PluginSdkSharedState,
    app_handle: Option<tauri::AppHandle>,
) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.block_on(async move {
            let bind_addr = sdk_state
                .status
                .lock()
                .map(|s| s.bind_address.clone())
                .unwrap_or_else(|_| String::from(DEFAULT_PLUGIN_SDK_BIND));

            let listener = match TcpListener::bind(&bind_addr).await {
                Ok(l) => {
                    println!("Plugin & Game-Voice SDK Server listening on {}", bind_addr);
                    l
                }
                Err(err) => {
                    eprintln!("Failed to bind Plugin SDK server on {}: {}", bind_addr, err);
                    return;
                }
            };

            while let Ok((stream, _peer)) = listener.accept().await {
                let state_clone = sdk_state.clone();
                let app_clone = app_handle.clone();
                tokio::spawn(async move {
                    handle_sdk_connection(stream, state_clone, app_clone).await;
                });
            }
        });
    });
}

async fn handle_sdk_connection(
    mut stream: TcpStream,
    sdk_state: PluginSdkSharedState,
    app_handle: Option<tauri::AppHandle>,
) {
    if let Ok(mut status) = sdk_state.status.lock() {
        status.connected_plugins += 1;
    }
    notify_ui(&sdk_state, &app_handle);

    let mut peek_buf = [0u8; 4];
    let is_http_upgrade = match stream.peek(&mut peek_buf).await {
        Ok(n) if n >= 3 => &peek_buf[..3] == b"GET",
        _ => false,
    };

    if is_http_upgrade {
        let _ = handle_websocket_session(&mut stream, &sdk_state, &app_handle).await;
    } else {
        let _ = handle_raw_json_session(&mut stream, &sdk_state, &app_handle).await;
    }

    if let Ok(mut status) = sdk_state.status.lock() {
        status.connected_plugins = status.connected_plugins.saturating_sub(1);
    }
    notify_ui(&sdk_state, &app_handle);
}

async fn handle_raw_json_session(
    stream: &mut TcpStream,
    sdk_state: &PluginSdkSharedState,
    app_handle: &Option<tauri::AppHandle>,
) -> std::io::Result<()> {
    let (rx, mut tx) = stream.split();
    let mut reader = BufReader::new(rx);
    let mut line = String::new();
    let mut last_ui_emit = std::time::Instant::now();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = process_sdk_json_message(trimmed, sdk_state);
        if last_ui_emit.elapsed().as_millis() >= 50 {
            notify_ui(sdk_state, app_handle);
            last_ui_emit = std::time::Instant::now();
        }
        let out = serde_json::to_string(&resp).unwrap_or_else(|_| String::from("{\"ok\":false}"));
        tx.write_all(format!("{out}\n").as_bytes()).await?;
        tx.flush().await?;
    }
    Ok(())
}

async fn handle_websocket_session(
    stream: &mut TcpStream,
    sdk_state: &PluginSdkSharedState,
    app_handle: &Option<tauri::AppHandle>,
) -> std::io::Result<()> {
    // 1. Read HTTP Upgrade headers
    let mut header_buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    while header_buf.len() < 8192 {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Ok(());
        }
        header_buf.push(byte[0]);
        if header_buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let headers_str = String::from_utf8_lossy(&header_buf);
    let mut ws_key = None;
    for line in headers_str.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("sec-websocket-key") {
                ws_key = Some(v.trim().to_string());
                break;
            }
        }
    }
    let Some(key) = ws_key else {
        return Ok(());
    };
    let accept_key = compute_websocket_accept_key(&key);
    let handshake_resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept_key}\r\n\r\n"
    );
    stream.write_all(handshake_resp.as_bytes()).await?;
    stream.flush().await?;

    let mut last_ui_emit = std::time::Instant::now();

    // 2. Read WebSocket frames (RFC 6455)
    loop {
        let mut hdr = [0u8; 2];
        if stream.read_exact(&mut hdr).await.is_err() {
            break;
        }
        let opcode = hdr[0] & 0x0F;
        let masked = (hdr[1] & 0x80) != 0;
        let mut payload_len = (hdr[1] & 0x7F) as usize;

        if opcode == 0x08 {
            // Close frame
            break;
        }
        if payload_len == 126 {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).await?;
            payload_len = u16::from_be_bytes(ext) as usize;
        } else if payload_len == 127 {
            let mut ext = [0u8; 8];
            stream.read_exact(&mut ext).await?;
            payload_len = u64::from_be_bytes(ext) as usize;
        }
        if payload_len > 262_144 {
            break;
        }

        let mut mask_key = [0u8; 4];
        if masked {
            stream.read_exact(&mut mask_key).await?;
        }
        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload).await?;
        if masked {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask_key[i % 4];
            }
        }

        if opcode == 0x09 {
            // Ping -> Pong
            let pong = encode_ws_frame(0x0A, &payload);
            stream.write_all(&pong).await?;
            continue;
        }

        if opcode == 0x01 {
            // Text frame
            if let Ok(text) = String::from_utf8(payload) {
                let resp = process_sdk_json_message(&text, sdk_state);
                if last_ui_emit.elapsed().as_millis() >= 50 {
                    notify_ui(sdk_state, app_handle);
                    last_ui_emit = std::time::Instant::now();
                }
                if let Ok(resp_json) = serde_json::to_string(&resp) {
                    let frame = encode_ws_frame(0x01, resp_json.as_bytes());
                    stream.write_all(&frame).await?;
                    stream.flush().await?;
                }
            }
        }
    }

    Ok(())
}

pub fn process_sdk_json_message(
    raw_json: &str,
    sdk_state: &PluginSdkSharedState,
) -> PluginSdkResponse {
    if let Ok(status) = sdk_state.status.lock() {
        if !status.enabled {
            return PluginSdkResponse {
                ok: false,
                message: Some(String::from("Plugin SDK is currently disabled in client settings")),
                status: None,
                spatial: None,
            };
        }
    }

    let parsed: Result<PluginSdkRequest, _> = serde_json::from_str(raw_json);
    let request = match parsed {
        Ok(req) => req,
        Err(err) => {
            return PluginSdkResponse {
                ok: false,
                message: Some(format!("Invalid JSON SDK payload: {err}")),
                status: None,
                spatial: None,
            };
        }
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    if let Ok(mut status) = sdk_state.status.lock() {
        status.packets_processed += 1;
        status.last_update_ms = now_ms;
    }

    match request {
        PluginSdkRequest::RegisterPlugin { plugin_name, version } => {
            let display_name = match version {
                Some(v) if !v.is_empty() => format!("{plugin_name} (v{v})"),
                _ => plugin_name,
            };
            if let Ok(mut status) = sdk_state.status.lock() {
                status.active_game_name = Some(display_name.clone());
            }
            PluginSdkResponse {
                ok: true,
                message: Some(format!("Registered plugin {display_name}")),
                status: sdk_state.status.lock().ok().map(|s| s.clone()),
                spatial: None,
            }
        }
        PluginSdkRequest::SetListenerPose { position, forward, up } => {
            if let Ok(mut spatial) = sdk_state.spatial_audio.lock() {
                spatial.set_listener_pose(position, forward, up);
            }
            PluginSdkResponse {
                ok: true,
                message: None,
                status: None,
                spatial: None,
            }
        }
        PluginSdkRequest::SetClientPosition {
            client_id,
            position,
            channel_name,
        } => {
            if let Ok(mut spatial) = sdk_state.spatial_audio.lock() {
                spatial.set_client_position(client_id, position, channel_name);
            }
            PluginSdkResponse {
                ok: true,
                message: None,
                status: None,
                spatial: None,
            }
        }
        PluginSdkRequest::SetClientMute { client_id, muted } => {
            if let Ok(mut vols) = sdk_state.client_volumes.lock() {
                vols.insert(client_id, if muted { 0.0 } else { 1.0 });
            }
            PluginSdkResponse {
                ok: true,
                message: None,
                status: None,
                spatial: None,
            }
        }
        PluginSdkRequest::BulkUpdate {
            game_name,
            listener,
            clients,
        } => {
            if let Some(game) = game_name {
                if !game.trim().is_empty() {
                    if let Ok(mut status) = sdk_state.status.lock() {
                        status.active_game_name = Some(game);
                    }
                }
            }
            if let Ok(mut spatial) = sdk_state.spatial_audio.lock() {
                if let Some(pose) = listener {
                    spatial.set_listener_pose(pose.position, Some(pose.forward), Some(pose.up));
                }
                for c in &clients {
                    spatial.set_client_position(c.client_id, c.position, c.channel_name.clone());
                }
            }
            if let Ok(mut vols) = sdk_state.client_volumes.lock() {
                for c in &clients {
                    if let Some(muted) = c.muted {
                        if muted {
                            vols.insert(c.client_id, 0.0);
                            continue;
                        }
                    }
                    if let Some(vol) = c.volume {
                        vols.insert(c.client_id, vol.clamp(0.0, 2.0));
                    }
                }
            }
            PluginSdkResponse {
                ok: true,
                message: None,
                status: None,
                spatial: None,
            }
        }
        PluginSdkRequest::GetState => PluginSdkResponse {
            ok: true,
            message: None,
            status: sdk_state.status.lock().ok().map(|s| s.clone()),
            spatial: sdk_state.spatial_audio.lock().ok().map(|s| s.snapshot()),
        },
        PluginSdkRequest::Reset => {
            if let Ok(mut spatial) = sdk_state.spatial_audio.lock() {
                spatial.reset();
            }
            PluginSdkResponse {
                ok: true,
                message: Some(String::from("Spatial audio state reset")),
                status: None,
                spatial: None,
            }
        }
    }
}

pub fn compute_websocket_accept_key(client_key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(client_key.trim().as_bytes());
    hasher.update(WS_GUID.as_bytes());
    let digest = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn encode_ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(10 + payload.len());
    frame.push(0x80 | (opcode & 0x0F)); // FIN + opcode
    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    frame
}

fn notify_ui(sdk_state: &PluginSdkSharedState, app_handle: &Option<tauri::AppHandle>) {
    if let Some(app) = app_handle {
        if let Ok(status) = sdk_state.status.lock() {
            let _ = app.emit("plugin_sdk_updated", status.clone());
        }
        if let Ok(spatial) = sdk_state.spatial_audio.lock() {
            let _ = app.emit("spatial_audio_updated", spatial.snapshot());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6455_accept_key_and_json_sdk_processing_work() {
        // Standard RFC 6455 test vector
        assert_eq!(
            compute_websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );

        let state = PluginSdkSharedState::default();
        let reg = process_sdk_json_message(
            r#"{"action":"register_plugin","plugin_name":"SaltyChat","version":"2.3.6"}"#,
            &state,
        );
        assert!(reg.ok);
        assert_eq!(
            state.status.lock().unwrap().active_game_name.as_deref(),
            Some("SaltyChat (v2.3.6)")
        );

        let bulk = process_sdk_json_message(
            r#"{
                "action":"bulk_update",
                "game_name":"GTA V / FiveM",
                "listener":{"position":{"x":0.0,"y":0.0,"z":0.0},"forward":{"x":0.0,"y":0.0,"z":1.0},"up":{"x":0.0,"y":1.0,"z":0.0}},
                "clients":[{"client_id":7,"position":{"x":6.0,"y":0.0,"z":0.0},"volume":1.0,"muted":false,"channel_name":"Ingame_Direct"}]
            }"#,
            &state,
        );
        assert!(bulk.ok);
        let (l, r) = state.spatial_audio.lock().unwrap().compute_stereo_gains(7);
        assert!(r > l, "expected client at +X to pan right: l={l}, r={r}");
    }
}
