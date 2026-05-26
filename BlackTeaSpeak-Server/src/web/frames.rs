use crate::runtime::WebServerInitInfo;

use crate::file_transfer::{FileEntryInfo, FileTransferError, PreparedFileTransfer};
use crate::query::{CommandRequest, QueryResponse};
use crate::transport::{SessionPresence, TransportNotification};

use crate::query::encode_query_value;
use crate::runtime::{
    BaselineRuntime, ChannelSnapshot, OnlineClientSnapshot, QuerySessionState, ServerSnapshot,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use std::collections::{BTreeMap, BTreeSet, HashMap};
pub(crate) const ERROR_COMMAND_NOT_FOUND: u32 = 0x100;
pub(crate) const ERROR_CLIENT_INVALID_ID: u32 = 0x200;
pub(crate) const ERROR_DATABASE_EMPTY_RESULT: u32 = 0x501;
pub(crate) const ERROR_PARAMETER_INVALID: u32 = 0x602;
pub(crate) const ERROR_FILE_ALREADY_EXISTS: u32 = 0x802;
pub(crate) const ERROR_FILE_NOT_FOUND: u32 = 0x803;
pub(crate) const ERROR_FILE_IO_ERROR: u32 = 0x804;
pub(crate) const ERROR_FILE_INVALID_PATH: u32 = 0x806;
pub(crate) const ERROR_FILE_OVERWRITE_EXCLUDES_RESUME: u32 = 0x808;
pub(crate) const ERROR_CURRENTLY_NOT_POSSIBLE: u32 = 0x704;
pub(crate) const ERROR_NOT_CONNECTED: u32 = 0x702;
pub(crate) const ERROR_PROTOCOL_VIOLATION: u32 = 1536;
pub(crate) type CommandRow = BTreeMap<String, String>;

use crate::web::BlackTeaWebPresence;
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
pub(crate) fn command_frame(command: &str, data: Vec<CommandRow>) -> Result<String> {
    serde_json::to_string(&json!({
        "type": "command",
        "command": command,
        "data": data,
        "flags": Vec::<String>::new(),
    }))
    .context("failed to encode BlackTeaWeb command frame")
}

pub(crate) fn raw_command_frame(command: Option<&str>, rows: &[CommandRow]) -> Result<String> {
    serde_json::to_string(&json!({
        "type": "command-raw",
        "payload": render_raw_payload(command, rows),
    }))
    .context("failed to encode BlackTeaWeb raw command frame")
}

pub(crate) fn ok_frame(return_code: &str) -> Result<String> {
    error_frame(return_code, 0, "ok", None)
}

pub(crate) fn bulk_ok_frame(return_code: &str, count: usize) -> Result<String> {
    command_frame(
        "error",
        (0..count.max(1))
            .map(|_| {
                row_map([
                    ("return_code", return_code.to_string()),
                    ("id", String::from("0")),
                    ("msg", String::from("ok")),
                ])
            })
            .collect(),
    )
}

pub(crate) fn command_request_from_rows(command: &str, rows: &[CommandRow]) -> CommandRequest {
    let mut named_args = BTreeMap::new();
    let option_groups = rows
        .iter()
        .map(|row| {
            row.iter()
                .filter(|(key, _)| key.as_str() != "return_code")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .filter(|group| !group.is_empty())
        .inspect(|group| named_args.extend(group.clone()))
        .collect::<Vec<_>>();

    CommandRequest {
        command: command.to_ascii_lowercase(),
        positional_args: Vec::new(),
        named_args,
        option_groups,
        flags: BTreeSet::new(),
    }
}

pub(crate) fn query_response_error_frame(
    return_code: &str,
    response: &QueryResponse,
) -> Result<String> {
    let mut row = row_map([
        ("return_code", return_code.to_string()),
        ("id", response.error_id.to_string()),
        ("msg", response.message.clone()),
    ]);
    for (key, value) in &response.extra_fields {
        row.insert(key.clone(), value.clone());
    }
    command_frame("error", vec![row])
}

pub(crate) fn query_notify_response_frames(
    notify_command: &str,
    response: &QueryResponse,
    return_code: &str,
    empty_result_is_error: bool,
) -> Result<Vec<String>> {
    if response.rows.is_empty() && empty_result_is_error {
        return Ok(vec![error_frame(
            return_code,
            ERROR_DATABASE_EMPTY_RESULT,
            "database empty result set",
            None,
        )?]);
    }

    let mut frames = Vec::new();
    if !response.rows.is_empty() {
        frames.push(command_frame(notify_command, response.rows.clone())?);
    }
    frames.push(ok_frame(return_code)?);
    Ok(frames)
}

pub(crate) fn _channel_snapshot_before(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
) -> Option<ChannelSnapshot> {
    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "channeledit" | "channeldelete" | "channelmove" => request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|channel_id| runtime.snapshot_channel(server_id, channel_id)),
        _ => None,
    }
}

pub(crate) fn _channel_snapshot_after(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
    response: &QueryResponse,
) -> Option<ChannelSnapshot> {
    if response.error_id != 0 {
        return None;
    }

    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "channeledit" | "channelmove" => request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|channel_id| runtime.snapshot_channel(server_id, channel_id)),
        "channelcreate" => response
            .rows
            .first()
            .and_then(|row| row.get("cid"))
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|channel_id| runtime.snapshot_channel(server_id, channel_id)),
        _ => None,
    }
}

pub(crate) fn _server_snapshot_before(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
) -> Option<ServerSnapshot> {
    if request.command != "serveredit" {
        return None;
    }

    session
        .selected_virtual_server_id
        .and_then(|server_id| runtime.snapshot_server(server_id))
}

pub(crate) fn _server_snapshot_after(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
    response: &QueryResponse,
) -> Option<ServerSnapshot> {
    if request.command != "serveredit" || response.error_id != 0 {
        return None;
    }

    session
        .selected_virtual_server_id
        .and_then(|server_id| runtime.snapshot_server(server_id))
}

pub(crate) fn _client_snapshot_before(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
) -> Option<OnlineClientSnapshot> {
    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "clientmove" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .or(Some(session.client_id).filter(|client_id| *client_id != 0))
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "clientkick" | "banclient" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "musicbotdelete" => request
            .named_args
            .get("botid")
            .or_else(|| request.named_args.get("bot_id"))
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|bot_identifier| {
                runtime.music_bot_client_snapshot_by_identifier(server_id, bot_identifier)
            }),
        _ => None,
    }
}

pub(crate) fn _client_snapshot_after(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
    response: &QueryResponse,
) -> Option<OnlineClientSnapshot> {
    if response.error_id != 0 {
        return None;
    }

    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "clientmove" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .or(Some(session.client_id).filter(|client_id| *client_id != 0))
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "clientkick" | "banclient" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "musicbotcreate" => response
            .rows
            .first()
            .and_then(|row| row.get("clid"))
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        _ => None,
    }
}

pub(crate) fn server_edited_row(
    before: &crate::runtime::ServerSnapshot,
    after: &crate::runtime::ServerSnapshot,
    invoker_id: u64,
    invoker_name: &str,
) -> Option<CommandRow> {
    let mut row = row_map([("virtualserver_id", after.id.to_string())]);
    let mut changed = false;

    if before.name != after.name {
        row.insert(String::from("virtualserver_name"), after.name.clone());
        changed = true;
    }
    if before.welcome_message != after.welcome_message {
        row.insert(
            String::from("virtualserver_welcomemessage"),
            after.welcome_message.clone(),
        );
        changed = true;
    }
    if before.host_message != after.host_message {
        row.insert(
            String::from("virtualserver_hostmessage"),
            after.host_message.clone(),
        );
        changed = true;
    }
    if before.host_message_mode != after.host_message_mode {
        row.insert(
            String::from("virtualserver_hostmessage_mode"),
            after.host_message_mode.to_string(),
        );
        changed = true;
    }
    if before.ask_for_privilegekey != after.ask_for_privilegekey {
        row.insert(
            String::from("virtualserver_ask_for_privilegekey"),
            after.ask_for_privilegekey.to_string(),
        );
        changed = true;
    }
    if before.max_clients != after.max_clients {
        row.insert(
            String::from("virtualserver_maxclients"),
            after.max_clients.to_string(),
        );
        changed = true;
    }
    if before.antiflood_points_tick_reduce != after.antiflood_points_tick_reduce {
        row.insert(
            String::from("virtualserver_antiflood_points_tick_reduce"),
            after.antiflood_points_tick_reduce.to_string(),
        );
        changed = true;
    }
    if before.antiflood_points_needed_command_block != after.antiflood_points_needed_command_block {
        row.insert(
            String::from("virtualserver_antiflood_points_needed_command_block"),
            after.antiflood_points_needed_command_block.to_string(),
        );
        changed = true;
    }
    if before.antiflood_points_needed_ip_block != after.antiflood_points_needed_ip_block {
        row.insert(
            String::from("virtualserver_antiflood_points_needed_ip_block"),
            after.antiflood_points_needed_ip_block.to_string(),
        );
        changed = true;
    }
    if before.antiflood_ban_time != after.antiflood_ban_time {
        row.insert(
            String::from("virtualserver_antiflood_ban_time"),
            after.antiflood_ban_time.to_string(),
        );
        changed = true;
    }

    if !changed {
        return None;
    }

    row.insert(String::from("invokerid"), invoker_id.to_string());
    row.insert(String::from("invokername"), invoker_name.to_string());
    row.insert(String::from("invokeruid"), format!("query-{}", invoker_id));
    Some(row)
}

pub(crate) fn _transport_notifications(
    command: &str,
    server_id: Option<u32>,
    response: &QueryResponse,
    before_channel_snapshot: Option<ChannelSnapshot>,
    after_channel_snapshot: Option<ChannelSnapshot>,
    before_server_snapshot: Option<ServerSnapshot>,
    after_server_snapshot: Option<ServerSnapshot>,
    before_client_snapshot: Option<OnlineClientSnapshot>,
    after_client_snapshot: Option<OnlineClientSnapshot>,
    invoker_id: u64,
    invoker_name: &str,
) -> Vec<TransportNotification> {
    if response.error_id != 0 {
        return Vec::new();
    }

    match command {
        "channelcreate" => match (server_id, after_channel_snapshot) {
            (Some(server_id), Some(channel)) => vec![TransportNotification::ChannelCreated {
                server_id,
                channel,
                invoker_id,
                invoker_name: invoker_name.to_string(),
            }],
            _ => Vec::new(),
        },
        "channeldelete" => match (server_id, before_channel_snapshot) {
            (Some(server_id), Some(channel)) => vec![TransportNotification::ChannelDeleted {
                server_id,
                channel,
                invoker_id,
                invoker_name: invoker_name.to_string(),
            }],
            _ => Vec::new(),
        },
        "channeledit" => match (server_id, before_channel_snapshot, after_channel_snapshot) {
            (Some(server_id), Some(before), Some(channel)) if before != channel => {
                let description_changed = before.description != channel.description;
                vec![TransportNotification::ChannelEdited {
                    server_id,
                    channel,
                    description_changed,
                    invoker_id,
                    invoker_name: invoker_name.to_string(),
                }]
            }
            _ => Vec::new(),
        },
        "channelmove" => match (server_id, before_channel_snapshot, after_channel_snapshot) {
            (Some(server_id), Some(before), Some(channel)) if before != channel => {
                vec![TransportNotification::ChannelMoved {
                    server_id,
                    previous_parent_id: before.parent_id,
                    channel,
                    invoker_id,
                    invoker_name: invoker_name.to_string(),
                }]
            }
            _ => Vec::new(),
        },
        "serveredit" => match (server_id, before_server_snapshot, after_server_snapshot) {
            (Some(server_id), Some(before), Some(after))
                if server_edited_row(&before, &after, invoker_id, invoker_name).is_some() =>
            {
                vec![TransportNotification::ServerEdited {
                    server_id,
                    before,
                    after,
                    invoker_id,
                    invoker_name: invoker_name.to_string(),
                }]
            }
            _ => Vec::new(),
        },
        "musicbotcreate" => after_client_snapshot
            .map(|snapshot| TransportNotification::ClientEnterView {
                presence: _presence_from_snapshot(&snapshot),
                from_channel_id: None,
                reason_id: 0,
            })
            .into_iter()
            .collect(),
        "musicbotdelete" => before_client_snapshot
            .map(|snapshot| TransportNotification::ClientLeftView {
                presence: _presence_from_snapshot(&snapshot),
                to_channel_id: None,
                reason_id: 5,
                reason_message: String::from("music bot deleted"),
                invoker_id,
                invoker_name: invoker_name.to_string(),
                invoker_uid: String::new(),
                ban_time: None,
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn _presence_from_snapshot(snapshot: &OnlineClientSnapshot) -> SessionPresence {
    SessionPresence {
        client_id: snapshot.id,
        login_name: snapshot.nickname.clone(),
        unique_identifier: snapshot.unique_identifier.clone(),
        client_type: snapshot.client_type_exact,
        server_id: snapshot.server_id,
        channel_id: snapshot.channel_id,
    }
}

pub(crate) fn _cleanup_channel_ids(
    request: &CommandRequest,
    before_client_snapshot: Option<&OnlineClientSnapshot>,
) -> Vec<u32> {
    match request.command.as_str() {
        "clientmove" | "clientkick" | "banclient" | "musicbotdelete" => before_client_snapshot
            .map(|snapshot| vec![snapshot.channel_id])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(crate) fn _cleanup_notifications(
    server_id: u32,
    cleanups: Vec<crate::runtime::TemporaryChannelCleanup>,
    invoker_id: u64,
    invoker_name: &str,
    invoker_uid: &str,
) -> Vec<TransportNotification> {
    let mut notifications = Vec::new();

    for cleanup in cleanups {
        if let Some(client) = cleanup.removed_client {
            notifications.push(TransportNotification::ClientLeftView {
                presence: _presence_from_snapshot(&client),
                to_channel_id: None,
                reason_id: 5,
                reason_message: String::from("temporary channel cleanup"),
                invoker_id,
                invoker_name: invoker_name.to_string(),
                invoker_uid: invoker_uid.to_string(),
                ban_time: None,
            });
        }
        if let Some(channel) = cleanup.removed_channel {
            notifications.push(TransportNotification::ChannelDeleted {
                server_id,
                channel,
                invoker_id,
                invoker_name: invoker_name.to_string(),
            });
        }
    }

    notifications
}

pub(crate) fn error_frame(
    return_code: &str,
    id: u32,
    message: &str,
    extra_message: Option<&str>,
) -> Result<String> {
    error_frame_with_fields(return_code, id, message, extra_message, [])
}

pub(crate) fn actor_avatar_id_from_unique_identifier(unique_identifier: &str) -> Option<String> {
    let decoded = BASE64_STANDARD.decode(unique_identifier).ok()?;
    let mut avatar_id = String::with_capacity(decoded.len().saturating_mul(2));
    for byte in decoded {
        let high = ((byte >> 4) & 0x0f) as u8;
        let low = (byte & 0x0f) as u8;
        avatar_id.push(char::from(b'a' + high));
        avatar_id.push(char::from(b'a' + low));
    }
    Some(avatar_id)
}

pub(crate) fn file_transfer_error_tuple(error: &FileTransferError) -> (u32, &'static str) {
    match error {
        FileTransferError::NotFound => (ERROR_FILE_NOT_FOUND, "file not found"),
        FileTransferError::AlreadyExists => (ERROR_FILE_ALREADY_EXISTS, "file already exists"),
        FileTransferError::InvalidPath => (ERROR_FILE_INVALID_PATH, "invalid path"),
        FileTransferError::InvalidPayload => (ERROR_PARAMETER_INVALID, "invalid parameter"),
        FileTransferError::Io => (ERROR_FILE_IO_ERROR, "file io error"),
    }
}

pub(crate) fn file_list_row(cid: u32, request_path: &str, entry: FileEntryInfo) -> CommandRow {
    let mut row = row_map([
        ("cid", cid.to_string()),
        ("path", request_path.to_string()),
        ("name", entry.name),
        ("size", entry.size.to_string()),
        ("datetime", entry.datetime.to_string()),
        ("type", entry.entry_type.to_string()),
    ]);
    if entry.entry_type == 0 {
        row.insert(
            String::from("empty"),
            if entry.empty {
                String::from("1")
            } else {
                String::from("0")
            },
        );
    }
    row
}

pub(crate) fn file_info_notify_row(
    cid: u32,
    requested_name: &str,
    return_code: &str,
    entry: FileEntryInfo,
) -> CommandRow {
    let (path, name) = split_virtual_parent_and_name(requested_name);
    let mut row = row_map([
        ("return_code", return_code.to_string()),
        ("cid", cid.to_string()),
        ("path", path),
        ("name", if name.is_empty() { entry.name } else { name }),
        ("size", entry.size.to_string()),
        ("datetime", entry.datetime.to_string()),
        ("type", entry.entry_type.to_string()),
        (
            "empty",
            if entry.empty {
                String::from("1")
            } else {
                String::from("0")
            },
        ),
    ]);
    if cid == 0 {
        row.insert(String::from("cid"), String::from("0"));
    }
    row
}

pub(crate) fn split_virtual_parent_and_name(value: &str) -> (String, String) {
    let normalized = value.replace('\\', "/");
    if normalized.is_empty() || normalized == "/" {
        return (String::from("/"), String::new());
    }

    let trimmed = normalized.trim_end_matches('/');
    if let Some((parent, name)) = trimmed.rsplit_once('/') {
        if parent.is_empty() {
            (String::from("/"), name.to_string())
        } else {
            (parent.to_string(), name.to_string())
        }
    } else {
        (
            String::from("/"),
            trimmed.trim_start_matches('/').to_string(),
        )
    }
}

pub(crate) fn bulk_ok_row(return_code: &str) -> CommandRow {
    row_map([
        ("return_code", return_code.to_string()),
        ("id", String::from("0")),
        ("msg", String::from("ok")),
    ])
}

pub(crate) fn bulk_error_row(return_code: &str, id: u32, message: &str) -> CommandRow {
    row_map([
        ("return_code", return_code.to_string()),
        ("id", id.to_string()),
        ("msg", message.to_string()),
    ])
}

pub(crate) fn transfer_start_row(
    client_transfer_id: &str,
    proto: &str,
    prepared: PreparedFileTransfer,
    include_size: bool,
) -> CommandRow {
    let mut row = row_map([
        ("clientftfid", client_transfer_id.to_string()),
        ("serverftfid", prepared.server_transfer_id.to_string()),
        ("ftkey", prepared.transfer_key),
        ("port", prepared.port.to_string()),
        ("seekpos", prepared.seek_position.to_string()),
        ("proto", proto.to_string()),
    ]);
    if include_size {
        row.insert(String::from("size"), prepared.size.to_string());
    }
    if let Some(ip) = prepared.ip {
        row.insert(String::from("ip"), ip);
    }
    row
}

pub(crate) fn transfer_started_row(client_transfer_id: &str) -> CommandRow {
    row_map([("clientftfid", client_transfer_id.to_string())])
}

pub(crate) fn transfer_progress_row(
    client_transfer_id: &str,
    file_bytes_transferred: u64,
    file_current_offset: u64,
    file_start_offset: u64,
    file_total_size: u64,
    network_bytes_received: u64,
    network_bytes_send: u64,
    network_current_speed: u64,
    network_average_speed: u64,
) -> CommandRow {
    row_map([
        ("clientftfid", client_transfer_id.to_string()),
        ("file_bytes_transferred", file_bytes_transferred.to_string()),
        ("file_current_offset", file_current_offset.to_string()),
        ("file_start_offset", file_start_offset.to_string()),
        ("file_total_size", file_total_size.to_string()),
        ("network_bytes_received", network_bytes_received.to_string()),
        ("network_bytes_send", network_bytes_send.to_string()),
        ("network_current_speed", network_current_speed.to_string()),
        ("network_average_speed", network_average_speed.to_string()),
    ])
}

pub(crate) fn transfer_status_row(
    client_transfer_id: &str,
    status: u32,
    message: &str,
) -> CommandRow {
    row_map([
        ("clientftfid", client_transfer_id.to_string()),
        ("status", status.to_string()),
        ("msg", message.to_string()),
    ])
}

pub(crate) fn parse_bool_flag(value: Option<&str>) -> bool {
    matches!(value, Some("1") | Some("true") | Some("yes") | Some("on"))
}

pub(crate) fn error_frame_with_fields<const N: usize>(
    return_code: &str,
    id: u32,
    message: &str,
    extra_message: Option<&str>,
    extra_fields: [(&str, String); N],
) -> Result<String> {
    let mut row = row_map([
        ("return_code", return_code.to_string()),
        ("id", id.to_string()),
        ("msg", message.to_string()),
    ]);
    if let Some(extra_message) = extra_message {
        row.insert(String::from("extra_msg"), extra_message.to_string());
    }
    for (key, value) in extra_fields {
        row.insert(key.to_string(), value);
    }
    command_frame("error", vec![row])
}

pub(crate) fn row_map<const N: usize>(pairs: [(&str, String); N]) -> CommandRow {
    pairs
        .into_iter()
        .map(|(key, value)| (String::from(key), value))
        .collect()
}

pub(crate) fn render_raw_payload(command: Option<&str>, rows: &[CommandRow]) -> String {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let mut parts = row
                .iter()
                .map(|(key, value)| format!("{}={}", key, encode_query_value(value)))
                .collect::<Vec<_>>();
            if index == 0 {
                if let Some(command) = command {
                    parts.insert(0, String::from(command));
                }
            }
            parts.join(" ")
        })
        .collect::<Vec<_>>()
        .join("|")
}

// MIGRATED FROM MOD.RS:
pub(crate) fn default_self_client_state(client_id: u64) -> CommandRow {
    row_map([
        ("client_nickname", String::from("BlackTeaWeb User")),
        (
            "client_unique_identifier",
            format!("compat-web-{}", client_id),
        ),
        ("client_type", String::from("0")),
        ("client_type_exact", String::from("3")),
        ("client_database_id", (client_id + 1000).to_string()),
        ("client_servergroups", String::from("8")),
        (
            "client_version",
            String::from("BlackTeaSpeak Compat BlackTeaWeb"),
        ),
        ("client_platform", String::from("web")),
        ("client_country", String::from("ZZ")),
        ("connection_client_ip", String::from("127.0.0.1")),
        ("client_away", String::from("0")),
        ("client_away_message", String::new()),
        ("client_input_hardware", String::from("0")),
        ("client_output_hardware", String::from("0")),
        ("client_input_muted", String::from("0")),
        ("client_output_muted", String::from("0")),
        ("client_flag_avatar", String::new()),
    ])
}

pub(crate) fn normalize_client_update_value(key: &str, value: &str) -> String {
    match key {
        "client_away"
        | "client_input_hardware"
        | "client_output_hardware"
        | "client_input_muted"
        | "client_output_muted" => normalize_boolish_string(value),
        _ => value.to_string(),
    }
}

pub(crate) fn normalize_boolish_string(value: &str) -> String {
    if matches!(value, "1" | "true" | "TRUE" | "True") {
        String::from("1")
    } else {
        String::from("0")
    }
}

pub(crate) fn bridge_timestamp() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u128::from(u64::MAX)) as u64,
        Err(_) => 0,
    }
}

pub(crate) fn text_message_row(
    target: &crate::runtime::TextMessageTarget,
    invoker_id: u64,
    invoker_name: &str,
    invoker_uid: &str,
    timestamp: u64,
) -> CommandRow {
    let mut row = row_map([
        ("targetmode", target.target_mode.to_string()),
        ("msg", target.message.clone()),
        ("invokerid", invoker_id.to_string()),
        ("invokername", invoker_name.to_string()),
        ("invokeruid", invoker_uid.to_string()),
        ("timestamp", timestamp.to_string()),
    ]);

    if let Some(channel_id) = target.channel_id {
        row.insert(String::from("cid"), channel_id.to_string());
    }
    if let Some(target_client_id) = target.target_client_id {
        row.insert(String::from("target"), target_client_id.to_string());
    }

    row
}

pub(crate) fn presence_enter_view_row(
    presence: &BlackTeaWebPresence,
    from_channel_id: Option<u32>,
    reason_id: u32,
) -> CommandRow {
    let mut row = presence.client_state.clone();
    row.insert(String::from("clid"), presence.client_id.to_string());
    row.insert(
        String::from("cfid"),
        from_channel_id.unwrap_or(0).to_string(),
    );
    row.insert(String::from("ctid"), presence.channel_id.to_string());
    row.insert(String::from("reasonid"), reason_id.to_string());
    row
}

pub(crate) fn presence_move_row(
    presence: &BlackTeaWebPresence,
    from_channel_id: u32,
    reason_id: u32,
    reason_message: &str,
) -> CommandRow {
    presence_move_row_for_invoker(
        presence,
        from_channel_id,
        reason_id,
        reason_message,
        presence.client_id,
        presence
            .client_state
            .get("client_nickname")
            .map(String::as_str)
            .unwrap_or("BlackTeaWeb User"),
        presence
            .client_state
            .get("client_unique_identifier")
            .map(String::as_str)
            .unwrap_or(""),
    )
}

pub(crate) fn presence_move_row_for_invoker(
    presence: &BlackTeaWebPresence,
    from_channel_id: u32,
    reason_id: u32,
    reason_message: &str,
    invoker_id: u64,
    invoker_name: &str,
    invoker_uid: &str,
) -> CommandRow {
    let mut row = row_map([
        ("clid", presence.client_id.to_string()),
        ("cfid", from_channel_id.to_string()),
        ("ctid", presence.channel_id.to_string()),
        ("reasonid", reason_id.to_string()),
        ("reasonmsg", reason_message.to_string()),
    ]);
    row.insert(String::from("invokerid"), invoker_id.to_string());
    row.insert(String::from("invokername"), String::from(invoker_name));
    row.insert(
        String::from("invokeruid"),
        if invoker_uid.is_empty() {
            format!("compat-web-{invoker_id}")
        } else {
            String::from(invoker_uid)
        },
    );
    row
}

pub(crate) fn presence_left_view_row(
    presence: &BlackTeaWebPresence,
    to_channel_id: Option<u32>,
    reason_id: u32,
    reason_message: &str,
) -> CommandRow {
    presence_left_view_row_for_invoker(
        presence,
        to_channel_id,
        reason_id,
        reason_message,
        presence.client_id,
        presence
            .client_state
            .get("client_nickname")
            .map(String::as_str)
            .unwrap_or("BlackTeaWeb User"),
        presence
            .client_state
            .get("client_unique_identifier")
            .map(String::as_str)
            .unwrap_or(""),
        None,
    )
}

pub(crate) fn presence_left_view_row_for_invoker(
    presence: &BlackTeaWebPresence,
    to_channel_id: Option<u32>,
    reason_id: u32,
    reason_message: &str,
    invoker_id: u64,
    invoker_name: &str,
    invoker_uid: &str,
    ban_time: Option<u32>,
) -> CommandRow {
    let mut row = row_map([
        ("clid", presence.client_id.to_string()),
        ("cfid", presence.channel_id.to_string()),
        ("ctid", to_channel_id.unwrap_or(0).to_string()),
        ("reasonid", reason_id.to_string()),
        ("reasonmsg", reason_message.to_string()),
    ]);
    row.insert(String::from("invokerid"), invoker_id.to_string());
    row.insert(String::from("invokername"), String::from(invoker_name));
    row.insert(
        String::from("invokeruid"),
        if invoker_uid.is_empty() {
            format!("compat-web-{invoker_id}")
        } else {
            String::from(invoker_uid)
        },
    );
    if let Some(ban_time) = ban_time {
        row.insert(String::from("bantime"), ban_time.to_string());
    }
    row
}

pub(crate) fn presence_update_row(
    before: &BlackTeaWebPresence,
    after: &BlackTeaWebPresence,
) -> Option<CommandRow> {
    let mut row = row_map([("clid", after.client_id.to_string())]);
    let mut changed = false;

    for (key, value) in &after.client_state {
        if before.client_state.get(key) != Some(value) {
            row.insert(key.clone(), value.clone());
            changed = true;
        }
    }

    changed.then_some(row)
}

pub(crate) fn build_initserver_row(
    server_info: &WebServerInitInfo,
    client_id: u64,
    nickname: &str,
) -> CommandRow {
    row_map([
        ("aclid", client_id.to_string()),
        ("acn", nickname.to_string()),
        ("virtualserver_id", server_info.server_id.to_string()),
        ("virtualserver_port", server_info.server_port.to_string()),
        ("virtualserver_name", server_info.server_name.clone()),
        (
            "virtualserver_unique_identifier",
            server_info.server_unique_identifier.clone(),
        ),
        (
            "virtualserver_welcomemessage",
            server_info.welcome_message.clone(),
        ),
        (
            "virtualserver_hostmessage",
            server_info.host_message.clone(),
        ),
        (
            "virtualserver_hostmessage_mode",
            server_info.host_message_mode.to_string(),
        ),
        (
            "virtualserver_ask_for_privilegekey",
            server_info.ask_for_privilegekey.to_string(),
        ),
        (
            "virtualserver_antiflood_points_tick_reduce",
            server_info.antiflood_points_tick_reduce.to_string(),
        ),
        (
            "virtualserver_antiflood_points_needed_command_block",
            server_info
                .antiflood_points_needed_command_block
                .to_string(),
        ),
        (
            "virtualserver_antiflood_points_needed_ip_block",
            server_info.antiflood_points_needed_ip_block.to_string(),
        ),
        (
            "virtualserver_antiflood_ban_time",
            server_info.antiflood_ban_time.to_string(),
        ),
    ])
}

pub(crate) fn decode_rows(rows: &[Map<String, Value>]) -> Vec<CommandRow> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|(key, value)| (key.clone(), json_value_to_string(value)))
                .collect::<CommandRow>()
        })
        .collect()
}

pub(crate) fn json_value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => {
            if *value {
                String::from("1")
            } else {
                String::from("0")
            }
        }
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

pub(crate) fn pong_frame(payload: Option<&Value>) -> Result<String> {
    let payload = payload.map(json_value_to_string).unwrap_or_default();
    serde_json::to_string(&json!({
        "type": "pong",
        "payload": payload,
        "ping_native": "0",
    }))
    .context("failed to encode BlackTeaWeb pong frame")
}
