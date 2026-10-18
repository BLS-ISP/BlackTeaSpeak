use crate::teaspeak_transport::UdpSession;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;

pub fn teaspeak_escape(s: &str) -> String {
    s.replace("\\", "\\\\")
        .replace(" ", "\\s")
        .replace("/", "\\/")
        .replace("|", "\\p")
        .replace("\x07", "\\a")
        .replace("\x08", "\\b")
        .replace("\x0c", "\\f")
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
        .replace("\x0b", "\\v")
}

pub fn teaspeak_unescape(s: &str) -> String {
    s.replace("\\s", " ")
        .replace("\\p", "|")
        .replace("\\/", "/")
        .replace("\\\\", "\\")
}

pub(crate) fn build_teaspeak_query_session(
    session: &UdpSession,
    addr: SocketAddr,
    server_id: u32,
    runtime: &crate::runtime::BaselineRuntime,
) -> crate::runtime::QuerySessionState {
    let (current_cid, current_dbid) = runtime
        .store
        .online_clients
        .get(&session.client_id)
        .map(|c| (c.channel_id, c.database_id))
        .unwrap_or((1, session.client_id + 1000));
    crate::runtime::QuerySessionState {
        client_id: session.client_id,
        connection_ip: addr.ip().to_string(),
        authenticated_login: None,
        selected_virtual_server_id: Some(server_id),
        current_channel_id: Some(current_cid),
        virtual_mode: false,
        notification_subscriptions: Vec::new(),
        actor_client_database_id_override: Some(current_dbid),
        client_nickname: session.client_nickname.clone(),
        client_away: false,
        client_away_message: String::new(),
        client_input_muted: false,
        client_output_muted: false,
        is_desktop_client: true,
        whisper_targets: None,
        ignored_clients: Vec::new(),
        subscribed_channels: std::collections::BTreeSet::new(),
        points: 0,
        blocked_until_millis: 0,
        last_points_decay_millis: 0,
    }
}

pub(crate) fn strip_teaspeak_return_code(req: &mut crate::query::CommandRequest) {
    req.named_args.remove("return_code");
    req.positional_args.retain(|arg| !arg.starts_with("rt-"));
    for group in &mut req.option_groups {
        group.remove("return_code");
    }
    req.option_groups.retain(|g| !g.is_empty());
}

pub fn render_teaspeak_rows_only(rows: &[BTreeMap<String, String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|(k, v)| format!("{}={}", k, teaspeak_escape(v)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub fn render_teaspeak_error_ack(
    resp: &crate::query::QueryResponse,
    return_code: Option<&str>,
) -> String {
    let mut parts = vec![
        format!("error id={}", resp.error_id),
        format!("msg={}", teaspeak_escape(&resp.message)),
    ];
    for (k, v) in &resp.extra_fields {
        parts.push(format!("{}={}", k, teaspeak_escape(v)));
    }
    if let Some(rc) = return_code {
        if !resp.extra_fields.contains_key("return_code") {
            parts.push(format!("return_code={}", teaspeak_escape(rc)));
        }
    }
    parts.join(" ")
}

pub(crate) fn teaspeak_notify_command_name(cmd: &str) -> String {
    match cmd {
        "handshakebegin" => "handshakeidentityproof".to_string(),
        "desktopgetmyperms" | "whoami" | "channellist" | "clientlist" => cmd.to_string(),
        "privilegekeylist" => "notifytokenlist".to_string(),
        "privilegekeyadd" => "notifytokenadd".to_string(),
        "ftgetfilelist" => "notifyfilelist".to_string(),
        "ftgetfileinfo" => "notifyfileinfo".to_string(),
        "ftinitupload" => "notifystartupload".to_string(),
        "ftinitdownload" => "notifystartdownload".to_string(),
        "messageget" => "notifymessage".to_string(),
        "logview" | "logquery" => "notifyserverlog".to_string(),
        "bantriggerlist" => "notifybantriggerlist".to_string(),
        "conversationfetch" => "notifyconversationfetch".to_string(),
        "conversationhistory" => "notifyconversationhistory".to_string(),
        "conversationmessagedelete" => "notifyconversationmessagedelete".to_string(),
        "rtcsessiondescribe" => "notifyrtcsessiondescription".to_string(),
        "broadcastlist" => "notifybroadcasts".to_string(),
        "musicbotcreate" | "mbotcreate" => "notifymusiccreated".to_string(),
        "musicbotplayerinfo" => "notifymusicplayerinfo".to_string(),
        "musicbotqueuelist" | "playlistsonglist" => "notifyplaylistsonglist".to_string(),
        "musicbotqueueadd" | "playlistsongadd" => "notifyplaylistsongadd".to_string(),
        "musicbotqueueremove" | "playlistsongremove" => "notifyplaylistsongremove".to_string(),
        "musicbotqueuereorder" | "playlistsongreorder" => "notifyplaylistsongreorder".to_string(),
        "playlistcreate" => "notifyplaylistcreated".to_string(),
        "playlistlist" => "notifyplaylistlist".to_string(),
        "playlistinfo" => "notifyplaylistinfo".to_string(),
        "listfeaturesupport" | "featurelist" => "notifyfeaturesupport".to_string(),
        "clientgetvariables" => "notifyclientupdated".to_string(),
        "servergetvariables" => "notifyserverupdated".to_string(),
        "serverrequestconnectioninfo" | "serverconnectioninfo" => {
            "notifyserverconnectioninfo".to_string()
        }
        other if other.starts_with("notify") => other.to_string(),
        other => format!("notify{}", other),
    }
}

pub async fn send_teaspeak_command_with_packet_id(
    cmd: &str,
    server_packet_id: &Arc<Mutex<u16>>,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
) {
    let raw_bytes = cmd.as_bytes();
    let (wire_bytes, is_compressed): (Vec<u8>, bool) = if raw_bytes.len() > 1000 {
        let compressed = quicklz::compress(raw_bytes, quicklz::CompressionLevel::Lvl1);
        if compressed.len() < raw_bytes.len() {
            (compressed, true)
        } else {
            (raw_bytes.to_vec(), false)
        }
    } else {
        (raw_bytes.to_vec(), false)
    };

    let chunk_size = 450; // TeaSpeak/TS3 fragment limit
    let chunks: Vec<&[u8]> = wire_bytes.chunks(chunk_size).collect();
    let total_chunks = chunks.len();
    if total_chunks == 0 {
        return;
    }

    let base_packet_id = {
        let mut lock = server_packet_id.lock().unwrap();
        let val = *lock;
        *lock = lock.wrapping_add(total_chunks as u16);
        val
    };

    for (i, chunk) in chunks.iter().enumerate() {
        let mut flags = 0x22;
        if is_compressed && i == 0 {
            flags |= 0x40; // Compressed
        }
        if total_chunks > 1 && (i == 0 || i == total_chunks - 1) {
            flags |= 0x10; // Fragmented
        }
        let out_packet_id = base_packet_id.wrapping_add(i as u16);
        let mut out_header = [0u8; 3];
        out_header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
        out_header[2] = flags;
        let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
            out_packet_id,
            0,
            flags,
            &out_header,
            chunk,
            iv_struct,
            true,
        );
        let mut final_packet = Vec::with_capacity(8 + 3 + encrypted_out.len() - 8);
        final_packet.extend_from_slice(&encrypted_out[0..8]);
        final_packet.extend_from_slice(&out_header);
        final_packet.extend_from_slice(&encrypted_out[8..]);
        let _ = socket.send_to(&final_packet, addr).await;
    }
}

pub async fn send_teaspeak_command(
    cmd: &str,
    session: &UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
) {
    send_teaspeak_command_with_packet_id(cmd, &session.server_packet_id, addr, socket, iv_struct)
        .await;
}

pub fn get_teaspeak_return_code(payload_str: &str) -> Option<String> {
    if let Some(rc) = parse_teaspeak_argument(payload_str, "return_code") {
        if !rc.is_empty() {
            return Some(rc);
        }
    }
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
            return Some(rc_val.to_string());
        }
    }
    None
}

pub fn parse_teaspeak_argument(payload: &str, key: &str) -> Option<String> {
    let key_eq = format!("{}=", key);
    if let Some(idx) = payload.find(&key_eq) {
        let value_start = idx + key_eq.len();
        let mut value_end = value_start;
        while value_end < payload.len() {
            let c = payload.as_bytes()[value_end];
            if c == b' ' || c == b'|' || c <= 0x1F || c == 0x7F {
                break;
            }
            value_end += 1;
        }
        let raw_val = &payload[value_start..value_end];
        return Some(teaspeak_unescape(raw_val));
    }
    None
}

pub fn parse_teaspeak_argument_u32(payload: &str, key: &str) -> Option<u32> {
    parse_teaspeak_argument(payload, key).and_then(|val| val.parse::<u32>().ok())
}

pub fn parse_teaspeak_argument_u64(payload: &str, key: &str) -> Option<u64> {
    parse_teaspeak_argument(payload, key).and_then(|val| val.parse::<u64>().ok())
}
