use super::*;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::file_transfer::FileTransferRegistry;
use crate::runtime::{
    AntiFloodSessionState, BaselineRuntime, QuerySessionState, stable_web_client_database_id,
    stable_web_client_unique_identifier,
};
use crate::transport::TransportNotification;

#[derive(Clone)]
pub(crate) struct BlackTeaWebSessionHandler {
    pub(crate) client_id: u64,
    pub(crate) login_phase: LoginPhase,
    pub(crate) raw_commands_enabled: bool,
    pub(crate) identity_public_key: Option<String>,
    pub(crate) requested_nickname: Option<String>,
    pub(crate) accepted_nickname: Option<String>,
    pub(crate) selected_server_id: Option<u32>,
    pub(crate) current_channel_id: Option<u32>,
    pub(crate) self_client_state: CommandRow,
    pub(crate) connection_ip: String,
    pub(crate) anti_flood_state: AntiFloodSessionState,
    pub(crate) file_transfers: Option<Arc<FileTransferRegistry>>,
    pub(crate) sessions: Option<SharedBlackTeaWebSessions>,
    pub(crate) pending_broadcasts: Vec<BlackTeaWebFrameBroadcast>,
    pub(crate) pending_permission_refreshes: Vec<BlackTeaWebPermissionRefresh>,
    pub(crate) pending_query_notifications: Vec<TransportNotification>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InboundFrame {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) return_code: Option<String>,
    #[serde(default)]
    pub(crate) data: Vec<Map<String, Value>>,
    #[serde(default)]
    pub(crate) flags: Vec<String>,
    #[serde(default)]
    pub(crate) payload: Option<Value>,
}

impl BlackTeaWebSessionHandler {
    #[cfg(test)]
    pub(crate) fn new(connection_id: u64) -> Self {
        Self::new_with_connection_ip(connection_id, String::from("127.0.0.1"))
    }

    pub(crate) fn new_with_connection_ip(connection_id: u64, connection_ip: String) -> Self {
        Self {
            client_id: WEB_CLIENT_ID_BASE + connection_id,
            login_phase: LoginPhase::AwaitHandshake,
            raw_commands_enabled: false,
            identity_public_key: None,
            requested_nickname: None,
            accepted_nickname: None,
            selected_server_id: None,
            current_channel_id: None,
            self_client_state: default_self_client_state(WEB_CLIENT_ID_BASE + connection_id),
            connection_ip,
            anti_flood_state: AntiFloodSessionState::default(),
            file_transfers: None,
            sessions: None,
            pending_broadcasts: Vec::new(),
            pending_permission_refreshes: Vec::new(),
            pending_query_notifications: Vec::new(),
        }
    }

    pub(crate) fn set_file_transfers(&mut self, file_transfers: Arc<FileTransferRegistry>) {
        self.file_transfers = Some(file_transfers);
    }

    pub(crate) fn set_sessions(&mut self, sessions: SharedBlackTeaWebSessions) {
        self.sessions = Some(sessions);
    }

    pub(crate) fn drain_pending_broadcasts(&mut self) -> Vec<BlackTeaWebFrameBroadcast> {
        self.pending_broadcasts.drain(..).collect()
    }

    pub(crate) fn drain_pending_permission_refreshes(&mut self) -> Vec<BlackTeaWebPermissionRefresh> {
        self.pending_permission_refreshes.drain(..).collect()
    }

    pub(crate) fn drain_pending_query_notifications(&mut self) -> Vec<TransportNotification> {
        self.pending_query_notifications.drain(..).collect()
    }

    pub(crate) fn queue_transport_notifications_for_all_web_sessions(
        &mut self,
        runtime: &BaselineRuntime,
        notifications: &[TransportNotification],
    ) {
        if let Some(sessions) = self.sessions.as_ref() {
            self.pending_broadcasts.extend(
                visibility_aware_transport_broadcasts(sessions, runtime, None, notifications)
                    .expect("BlackTeaWeb visibility-aware broadcasts should encode"),
            );
        } else {
            self.pending_broadcasts
                .extend(frame_broadcasts_from_transport_notifications(
                    runtime,
                    None,
                    notifications,
                ));
        }
        self.pending_query_notifications
            .extend(notifications.iter().cloned());
    }

    pub(crate) fn queue_temporary_channel_cleanup_notifications(
        &mut self,
        runtime: &mut BaselineRuntime,
        server_id: u32,
        channel_ids: &[u32],
        invoker_id: u64,
        invoker_name: &str,
        invoker_uid: &str,
    ) {
        let notifications = _cleanup_notifications(
            server_id,
            runtime.cleanup_temporary_channels(server_id, channel_ids),
            invoker_id,
            invoker_name,
            invoker_uid,
        );
        if !notifications.is_empty() {
            self.queue_transport_notifications_for_all_web_sessions(runtime, &notifications);
        }
    }

    pub(crate) fn self_invoker_identity(&self) -> (u64, String, String) {
        (
            self.client_id,
            self.self_client_nickname(),
            self.self_client_state
                .get("client_unique_identifier")
                .cloned()
                .unwrap_or_else(|| format!("compat-web-{}", self.client_id)),
        )
    }

    pub(crate) fn queue_client_moved_notification(
        &mut self,
        presence: &BlackTeaWebPresence,
        from_channel_id: u32,
        reason_id: u32,
        reason_message: String,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
    ) -> Result<()> {
        let frame = command_frame(
            "notifyclientmoved",
            vec![presence_move_row_for_invoker(
                presence,
                from_channel_id,
                reason_id,
                &reason_message,
                invoker_id,
                &invoker_name,
                &invoker_uid,
            )],
        )?;
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Client {
            client_id: presence.client_id,
            frame: frame.clone(),
        });
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id: presence.server_id,
            exclude_client_id: Some(presence.client_id),
            frame,
        });
        self.pending_query_notifications
            .push(TransportNotification::ClientMoved {
                presence: session_presence_from_blackteaweb_presence(presence),
                from_channel_id,
                reason_id,
                reason_message,
                invoker_id,
                invoker_name,
                invoker_uid,
            });
        Ok(())
    }

    pub(crate) fn queue_client_left_notification(
        &mut self,
        presence: &BlackTeaWebPresence,
        to_channel_id: Option<u32>,
        reason_id: u32,
        reason_message: String,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
        ban_time: Option<u32>,
    ) -> Result<()> {
        let frame = command_frame(
            "notifyclientleftview",
            vec![presence_left_view_row_for_invoker(
                presence,
                to_channel_id,
                reason_id,
                &reason_message,
                invoker_id,
                &invoker_name,
                &invoker_uid,
                ban_time,
            )],
        )?;
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Client {
            client_id: presence.client_id,
            frame: frame.clone(),
        });
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id: presence.server_id,
            exclude_client_id: Some(presence.client_id),
            frame,
        });
        self.pending_query_notifications
            .push(TransportNotification::ClientLeftView {
                presence: session_presence_from_blackteaweb_presence(presence),
                to_channel_id,
                reason_id,
                reason_message,
                invoker_id,
                invoker_name,
                invoker_uid,
                ban_time,
            });
        Ok(())
    }

    pub(crate) fn queue_permission_refresh(&mut self, command: &str) {
        let Some(server_id) = self.selected_server_id else {
            return;
        };
        let scope = permission_refresh_scope(command);
        if scope.is_empty() {
            return;
        }

        self.pending_permission_refreshes
            .push(BlackTeaWebPermissionRefresh { server_id, scope });
    }

    pub(crate) fn handle_text_frame(
        &mut self,
        payload: &str,
        runtime: &mut BaselineRuntime,
    ) -> Result<Vec<String>> {
        let frame = serde_json::from_str::<InboundFrame>(payload)
            .with_context(|| format!("invalid BlackTeaWeb JSON frame: {payload}"))?;

        match frame.kind.as_str() {
            "enable-raw-commands" => {
                self.raw_commands_enabled = true;
                Ok(Vec::new())
            }
            "command" => self.handle_command_frame(frame, runtime),
            "ping" => Ok(vec![pong_frame(frame.payload.as_ref())?]),
            "pong" => Ok(Vec::new()),
            _ => Ok(Vec::new()),
        }
    }

    pub(crate) fn handle_command_frame(
        &mut self,
        frame: InboundFrame,
        runtime: &mut BaselineRuntime,
    ) -> Result<Vec<String>> {
        let command = frame.command.unwrap_or_default();
        let connection_ip = self.connection_ip.clone();
        let rows = decode_rows(&frame.data);
        let return_code = rows
            .first()
            .and_then(|row| row.get("return_code"))
            .cloned()
            .or(frame.return_code)
            .unwrap_or_default();

        if self.login_phase == LoginPhase::Connected
            && let Some(response) = runtime.enforce_web_antiflood(
                &command,
                self.selected_server_id,
                self.current_channel_id,
                self.self_client_database_id(),
                Some(connection_ip.as_str()),
                &mut self.anti_flood_state,
            )
        {
            return Ok(vec![error_frame(
                &return_code,
                response.error_id,
                &response.message,
                None,
            )?]);
        }

        match command.as_str() {
            "handshakebegin" => {
                let mut auth_method = 1u32;
                if let Some(first_row) = rows.first() {
                    if let Some(am) = first_row
                        .get("authentication_method")
                        .and_then(|v| v.parse::<u32>().ok())
                    {
                        auth_method = am;
                    }
                    if let Some(pk) = first_row
                        .get("publicKey")
                        .or_else(|| first_row.get("public_key"))
                        .or_else(|| first_row.get("client_uid"))
                        .filter(|s| !s.is_empty())
                    {
                        self.identity_public_key = Some(pk.clone());
                        let uid = stable_web_client_unique_identifier(pk);
                        let cldbid = stable_web_client_database_id(&uid);
                        self.self_client_state
                            .insert("client_unique_identifier".to_string(), uid);
                        self.self_client_state
                            .insert("client_database_id".to_string(), cldbid.to_string());
                    }
                }
                if auth_method == 2 {
                    self.login_phase = LoginPhase::AwaitClientInit;
                    return Ok(vec![ok_frame(&return_code)?]);
                }
                self.login_phase = LoginPhase::AwaitIdentityProof;
                let row = row_map([
                    ("digest", String::from("SHA-256")),
                    ("message", String::from("BBBB")),
                ]);
                Ok(vec![
                    command_frame("handshakeidentityproof", vec![row])?,
                    ok_frame(&return_code)?,
                ])
            }
            "handshakeindentityproof" | "handshakeidentityproof" => self.handle_identity_proof(&return_code),
            "clientinit" => {
                tracing::info!("WebTransport session {} handling clientinit. Login phase: {:?}", self.client_id, self.login_phase);
                if self.login_phase != LoginPhase::AwaitClientInit {
                    tracing::warn!("WebTransport session {} rejected clientinit because phase is {:?}", self.client_id, self.login_phase);
                    return Ok(vec![error_frame(
                        &return_code,
                        ERROR_PROTOCOL_VIOLATION,
                        "clientinit not expected",
                        None,
                    )?]);
                }
                
                let existing_uid = self
                    .self_client_state
                    .get("client_unique_identifier")
                    .filter(|u| !u.is_empty() && !u.starts_with("compat-web-"))
                    .cloned();
                let existing_dbid = self.self_client_database_id();
                let row = rows.first().cloned().unwrap_or_default();
                let client_nickname = row.get("client_nickname").cloned().unwrap_or_else(|| String::from("WebClient"));
                tracing::info!("WebTransport session {} nickname: {}", self.client_id, client_nickname);
                
                for (k, v) in &row {
                    if k != "return_code" {
                        self.self_client_state.insert(k.clone(), v.clone());
                    }
                }
                self.self_client_state.insert("client_nickname".to_string(), client_nickname.clone());
                
                let unique_identifier = row
                    .get("client_unique_identifier")
                    .filter(|u| !u.is_empty())
                    .cloned()
                    .or(existing_uid)
                    .unwrap_or_else(|| format!("compat-web-{}", self.client_id));
                let resolved_dbid = existing_dbid
                    .filter(|_| unique_identifier != format!("compat-web-{}", self.client_id))
                    .unwrap_or(self.client_id + 1000);
                self.self_client_state.insert("client_database_id".to_string(), resolved_dbid.to_string());
                self.self_client_state.insert("client_unique_identifier".to_string(), unique_identifier.clone());
                
                let mut mock_session = crate::runtime::QuerySessionState {
                    client_id: self.client_id,
                    connection_ip: self.connection_ip.clone(),
                    authenticated_login: None,
                    selected_virtual_server_id: None,
                    current_channel_id: None,
                    virtual_mode: false,
                    notification_subscriptions: vec![],
                    actor_client_database_id_override: Some(resolved_dbid),
                    client_nickname: client_nickname.clone(),
                    client_away: false,
                    client_away_message: String::new(),
                    client_input_muted: false,
                    client_output_muted: false,
                    is_desktop_client: false,
                    whisper_targets: None,
                    ignored_clients: Vec::new(),
                    subscribed_channels: std::collections::BTreeSet::new(),
                    points: 0,
                    blocked_until_millis: 0,
                    last_points_decay_millis: 0,
                };
                
                let mut mock_request = crate::query::CommandRequest {
                    command: "clientinit".to_string(),
                    positional_args: vec![],
                    named_args: std::collections::BTreeMap::new(),
                    option_groups: vec![],
                    flags: std::collections::BTreeSet::new(),
                };
                mock_request.named_args.insert("client_nickname".to_string(), client_nickname.clone());
                
                let _bootstrap = match crate::transport::handlers::perform_client_bootstrap(
                    runtime, &mut mock_session, &mock_request, &unique_identifier, false
                ) {
                    Ok(data) => data,
                    Err(_) => return Ok(vec![error_frame(&return_code, 1024, "invalid serverID", None)?]),
                };
                
                self.selected_server_id = mock_session.selected_virtual_server_id;
                self.current_channel_id = mock_session.current_channel_id;
                self.login_phase = LoginPhase::Connected;
                self.sync_self_server_groups_from_runtime(runtime);
                
                let mut out = Vec::new();
                
                let server_info = runtime.web_server_init_info().unwrap();
                let initserver_row = crate::web::frames::build_initserver_row(&server_info, self.client_id, &client_nickname);
                tracing::info!("WebTransport session {} built initserver_row", self.client_id);
                out.push(command_frame("initserver", vec![initserver_row])?);
                
                let bootstrap = self.connected_bootstrap_frames(runtime)?;
                tracing::info!("WebTransport session {} built bootstrap frames: {}", self.client_id, bootstrap.len());
                out.extend(bootstrap);
                
                // The ok frame MUST be at the end of the response sequence to resolve the client's promise after data is received.
                out.push(ok_frame(&return_code)?);
                
                tracing::info!("WebTransport session {} returning {} frames", self.client_id, out.len());
                
                return Ok(out);
            }
            "whoami" => self.handle_whoami(&return_code),
            "servergetvariables" => self.handle_server_get_variables(&return_code, runtime),
            "serverinfo" => self.handle_server_info(&return_code, runtime),
            "servergrouplist" => self.handle_server_group_list(&return_code, runtime),
            "channelgrouplist" => self.handle_channel_group_list(&return_code, runtime),
            "channellist" => self.handle_channel_list(&return_code, runtime),
            "serverconnectioninfo" => self.handle_server_connection_info(&return_code, runtime),
            "permissionlist" => self.handle_permission_list(&return_code, runtime),
            "featurelist" => self.handle_feature_list(&return_code, runtime),
            "whispersessioninitialize" => {
                if let Ok((_ssrc, selection)) = parse_whisper_target_selection(&rows) {
                    if let Some(online_client) = runtime.store.online_clients.get_mut(&self.client_id) {
                        online_client.whisper_targets = Some(selection);
                    }
                }
                Ok(vec![ok_frame(&return_code)?])
            }
            "whispersessionreset" => {
                if let Some(online_client) = runtime.store.online_clients.get_mut(&self.client_id) {
                    online_client.whisper_targets = None;
                }
                Ok(vec![ok_frame(&return_code)?])
            }
            _ => {
                if self.login_phase != LoginPhase::Connected {
                    return Ok(vec![error_frame(
                        &return_code,
                        ERROR_NOT_CONNECTED,
                        "not connected",
                        None,
                    )?]);
                }

                if command == "clientupdate" {
                    if let Some(first_row) = rows.first() {
                        let _ = self.apply_self_client_updates(first_row);
                    }
                }

                let mut request = command_request_from_rows(&command, &rows);
                for flag in &frame.flags {
                    let trimmed = flag.trim_start_matches('-');
                    if !trimmed.is_empty() {
                        request.flags.insert(trimmed.to_string());
                    }
                }
                if request.command == "clientmove" && !request.named_args.contains_key("clid") {
                    request.named_args.insert("clid".to_string(), self.client_id.to_string());
                }

                let mut query_session = QuerySessionState {
                    client_id: self.client_id,
                    connection_ip: self.connection_ip.clone(),
                    authenticated_login: None,
                    selected_virtual_server_id: self.selected_server_id,
                    current_channel_id: Some(self.current_channel_id.unwrap_or(1)),
                    virtual_mode: false,
                    notification_subscriptions: vec![],
                    actor_client_database_id_override: self.self_client_database_id().or(Some(self.client_id + 1000)),
                    client_nickname: self.self_client_nickname(),
                    client_away: self.self_client_state.get("client_away").is_some_and(|v| v == "1"),
                    client_away_message: self.self_client_state.get("client_away_message").cloned().unwrap_or_default(),
                    client_input_muted: self.self_client_state.get("client_input_muted").is_some_and(|v| v == "1"),
                    client_output_muted: self.self_client_state.get("client_output_muted").is_some_and(|v| v == "1"),
                    is_desktop_client: false,
                    whisper_targets: None,
                    ignored_clients: Vec::new(),
                    subscribed_channels: std::collections::BTreeSet::new(),
                    points: 0,
                    blocked_until_millis: 0,
                    last_points_decay_millis: 0,
                };
                let before_query_session = query_session.clone();
                let (resp, notifs) = crate::transport::execute_request_with_notifications(
                    runtime,
                    &request,
                    &before_query_session,
                    &mut query_session,
                );
                if let Some(new_cid) = query_session.current_channel_id {
                    self.current_channel_id = Some(new_cid);
                }
                self.sync_self_server_groups_from_runtime(runtime);

                let scope = permission_refresh_scope(&request.command);
                if !scope.is_empty() {
                    if let Some(server_id) = self.selected_server_id {
                        self.pending_permission_refreshes.push(BlackTeaWebPermissionRefresh {
                            server_id,
                            scope,
                        });
                    }
                }

                if resp.error_id == 259 {
                    return Ok(vec![ok_frame(&return_code)?]);
                }
                if resp.error_id != 0 {
                    return Ok(vec![query_response_error_frame(&return_code, &resp)?]);
                }

                if !notifs.is_empty() {
                    self.queue_transport_notifications_for_all_web_sessions(runtime, &notifs);
                }

                match request.command.as_str() {
                    "ftgetfilelist" => query_notify_response_frames("notifyfilelist", &resp, &return_code, false),
                    "ftgetfileinfo" => query_notify_response_frames("notifyfileinfo", &resp, &return_code, false),
                    "ftlist"
                        if request.named_args.contains_key("cid")
                            || request.named_args.contains_key("path")
                            || request
                                .option_groups
                                .iter()
                                .any(|r| r.contains_key("cid") || r.contains_key("path")) =>
                    {
                        query_notify_response_frames("notifyfilelist", &resp, &return_code, false)
                    }
                    "ftlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyftlist", &resp, &return_code, false)
                    }
                    "ftinitupload" => query_notify_response_frames("notifystartupload", &resp, &return_code, false),
                    "ftinitdownload" => query_notify_response_frames("notifystartdownload", &resp, &return_code, false),
                    "messagelist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifymessagelist", &resp, &return_code, false)
                    }
                    "messageget" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifymessage", &resp, &return_code, false)
                    }
                    "complainlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifycomplainlist", &resp, &return_code, false)
                    }
                    "logview" | "logquery" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyserverlog", &resp, &return_code, false)
                    }
                    "bantriggerlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifybantriggerlist", &resp, &return_code, false)
                    }
                    "conversationfetch" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyconversationfetch", &resp, &return_code, false)
                    }
                    "conversationhistory" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyconversationhistory", &resp, &return_code, false)
                    }
                    "conversationmessagedelete" => {
                        if let Some(server_id) = self.selected_server_id {
                            if !resp.rows.is_empty() {
                                self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                    server_id,
                                    exclude_client_id: Some(self.client_id),
                                    frame: command_frame("notifyconversationmessagedelete", resp.rows.clone())?,
                                });
                            }
                        }
                        query_notify_response_frames("notifyconversationmessagedelete", &resp, &return_code, false)
                    }
                    "rtcsessiondescribe" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyrtcsessiondescription", &resp, &return_code, false)
                    }
                    "broadcastlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifybroadcasts", &resp, &return_code, true)
                    }
                    "broadcastvideo" | "broadcastaudio" | "rtcsessionreset" => {
                        if let Some(server_id) = self.selected_server_id {
                            let bcast_rows = runtime.active_broadcast_rows(server_id, None);
                            self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                server_id,
                                exclude_client_id: None,
                                frame: command_frame("notifybroadcasts", bcast_rows)?,
                            });
                        }
                        Ok(vec![ok_frame(&return_code)?])
                    }
                    "banlist" => query_notify_response_frames("notifybanlist", &resp, &return_code, false),
                    "tokenlist" | "privilegekeylist" => query_notify_response_frames("notifytokenlist", &resp, &return_code, false),
                    "tokenactionlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifytokenactionlist", &resp, &return_code, false)
                    }
                    "musicbotcreate" | "mbotcreate" => {
                        if let Some(server_id) = self.selected_server_id {
                            if let Some(bot_id) = resp
                                .rows
                                .first()
                                .and_then(|r| r.get("botid").or_else(|| r.get("bot_id")))
                                .and_then(|v| v.parse::<u64>().ok())
                            {
                                if let Some(payload) = runtime.music_bot_notify_payload_by_identifier(server_id, bot_id) {
                                    self.queue_music_bot_notify_payload(server_id, payload)?;
                                }
                            }
                        }
                        if self.raw_commands_enabled {
                            Ok(vec![raw_command_frame(None, &resp.rows)?, ok_frame(&return_code)?])
                        } else {
                            query_notify_response_frames("notifymusiccreated", &resp, &return_code, false)
                        }
                    }
                    "musicbotplayeraction" => {
                        if let Some(server_id) = self.selected_server_id {
                            if let Some(bot_id) = resp
                                .rows
                                .first()
                                .and_then(|r| r.get("botid").or_else(|| r.get("bot_id")))
                                .and_then(|v| v.parse::<u64>().ok())
                            {
                                if let Some(payload) = runtime.music_bot_notify_payload_by_identifier(server_id, bot_id) {
                                    self.queue_music_bot_notify_payload(server_id, payload)?;
                                }
                            }
                        }
                        let mut frames = Vec::new();
                        if !resp.rows.is_empty() {
                            if self.raw_commands_enabled {
                                frames.push(raw_command_frame(None, &resp.rows)?);
                            } else {
                                frames.push(command_frame("", resp.rows)?);
                            }
                        }
                        frames.push(ok_frame(&return_code)?);
                        Ok(frames)
                    }
                    "musicbotqueueadd" | "playlistsongadd" => {
                        if let Some(server_id) = self.selected_server_id {
                            if !resp.rows.is_empty() {
                                let add_rows = Self::playlist_song_add_notify_rows(&resp.rows);
                                self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                    server_id,
                                    exclude_client_id: None,
                                    frame: command_frame("notifyplaylistsongadd", add_rows)?,
                                });
                                let loaded_rows = Self::playlist_song_loaded_notify_rows(&resp.rows);
                                if !loaded_rows.is_empty() {
                                    self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                        server_id,
                                        exclude_client_id: None,
                                        frame: command_frame("notifyplaylistsongloaded", loaded_rows)?,
                                    });
                                }
                                if let Some(plid) = resp
                                    .rows
                                    .first()
                                    .and_then(|r| r.get("playlist_id").or_else(|| r.get("botid")))
                                    .and_then(|v| v.parse::<u32>().ok())
                                {
                                    if let Some(payload) = runtime.music_bot_notify_payload_by_playlist_id(server_id, plid) {
                                        self.queue_music_bot_notify_payload(server_id, payload)?;
                                    }
                                }
                            }
                        }
                        let mut frames = Vec::new();
                        if !resp.rows.is_empty() {
                            if self.raw_commands_enabled {
                                frames.push(raw_command_frame(None, &resp.rows)?);
                            } else {
                                frames.push(command_frame("", resp.rows)?);
                            }
                        }
                        frames.push(ok_frame(&return_code)?);
                        Ok(frames)
                    }
                    "musicbotqueueremove" | "playlistsongremove" => {
                        if let Some(server_id) = self.selected_server_id {
                            if !resp.rows.is_empty() {
                                self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                    server_id,
                                    exclude_client_id: None,
                                    frame: command_frame("notifyplaylistsongremove", resp.rows.clone())?,
                                });
                                if let Some(plid) = resp
                                    .rows
                                    .first()
                                    .and_then(|r| r.get("playlist_id").or_else(|| r.get("botid")))
                                    .and_then(|v| v.parse::<u32>().ok())
                                {
                                    if let Some(payload) = runtime.music_bot_notify_payload_by_playlist_id(server_id, plid) {
                                        self.queue_music_bot_notify_payload(server_id, payload)?;
                                    }
                                }
                            }
                        }
                        let mut frames = Vec::new();
                        if !resp.rows.is_empty() {
                            if self.raw_commands_enabled {
                                frames.push(raw_command_frame(None, &resp.rows)?);
                            } else {
                                frames.push(command_frame("", resp.rows)?);
                            }
                        }
                        frames.push(ok_frame(&return_code)?);
                        Ok(frames)
                    }
                    "musicbotqueuereorder" | "playlistsongreorder" => {
                        if let Some(server_id) = self.selected_server_id {
                            if !resp.rows.is_empty() {
                                self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                    server_id,
                                    exclude_client_id: None,
                                    frame: command_frame("notifyplaylistsongreorder", resp.rows.clone())?,
                                });
                                if let Some(plid) = resp
                                    .rows
                                    .first()
                                    .and_then(|r| r.get("playlist_id").or_else(|| r.get("botid")))
                                    .and_then(|v| v.parse::<u32>().ok())
                                {
                                    if let Some(payload) = runtime.music_bot_notify_payload_by_playlist_id(server_id, plid) {
                                        self.queue_music_bot_notify_payload(server_id, payload)?;
                                    }
                                }
                            }
                        }
                        let mut frames = Vec::new();
                        if !resp.rows.is_empty() {
                            if self.raw_commands_enabled {
                                frames.push(raw_command_frame(None, &resp.rows)?);
                            } else {
                                frames.push(command_frame("", resp.rows)?);
                            }
                        }
                        frames.push(ok_frame(&return_code)?);
                        Ok(frames)
                    }
                    "playlistcreate" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyplaylistcreated", &resp, &return_code, false)
                    }
                    "playlistdelete" => {
                        if let Some(server_id) = self.selected_server_id {
                            if !resp.rows.is_empty() {
                                self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                                    server_id,
                                    exclude_client_id: None,
                                    frame: command_frame("notifyplaylistdeleted", resp.rows.clone())?,
                                });
                            }
                        }
                        let mut frames = Vec::new();
                        if !resp.rows.is_empty() {
                            if self.raw_commands_enabled {
                                frames.push(raw_command_frame(None, &resp.rows)?);
                            } else {
                                frames.push(command_frame("", resp.rows)?);
                            }
                        }
                        frames.push(ok_frame(&return_code)?);
                        Ok(frames)
                    }
                    "musicbotlist" | "mbotlist" => query_notify_response_frames("notifymusicbotlist", &resp, &return_code, false),
                    "musicbotplayerinfo" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifymusicplayerinfo", &resp, &return_code, false)
                    }
                    "musicbotqueuelist" | "playlistsonglist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyplaylistsonglist", &resp, &return_code, false)
                    }
                    "playlistlist" => query_notify_response_frames("notifyplaylistlist", &resp, &return_code, false),
                    "playlistinfo" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyplaylistinfo", &resp, &return_code, false)
                    }
                    "playlistpermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyplaylistpermlist", &resp, &return_code, false)
                    }
                    "playlistclientlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyplaylistclientlist", &resp, &return_code, false)
                    }
                    "playlistclientpermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyplaylistclientpermlist", &resp, &return_code, false)
                    }
                    "querylist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyquerylist", &resp, &return_code, false)
                    }
                    "listfeaturesupport" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyfeaturesupport", &resp, &return_code, false)
                    }
                    "propertylist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifypropertylist", &resp, &return_code, false)
                    }
                    "clientgetvariables" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyclientupdated", &resp, &return_code, false)
                    }
                    "servergrouppermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyservergrouppermlist", &resp, &return_code, false)
                    }
                    "servergroupclientlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyservergroupclientlist", &resp, &return_code, false)
                    }
                    "channelgrouppermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifychannelgrouppermlist", &resp, &return_code, false)
                    }
                    "channelgroupclientlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifychannelgroupclientlist", &resp, &return_code, false)
                    }
                    "channelpermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifychannelpermlist", &resp, &return_code, false)
                    }
                    "clientpermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyclientpermlist", &resp, &return_code, false)
                    }
                    "channelclientpermlist" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifychannelclientpermlist", &resp, &return_code, false)
                    }
                    "permget" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifypermget", &resp, &return_code, false)
                    }
                    "permfind" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifypermfind", &resp, &return_code, false)
                    }
                    "permidgetbyname" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifypermidgetbyname", &resp, &return_code, false)
                    }
                    "servergroupsbyclientid" if !self.raw_commands_enabled => {
                        query_notify_response_frames("notifyservergroupsbyclientid", &resp, &return_code, false)
                    }
                    _ => {
                        let mut frames = Vec::new();
                        if !resp.rows.is_empty() {
                            if self.raw_commands_enabled {
                                frames.push(raw_command_frame(None, &resp.rows)?);
                            } else {
                                frames.push(command_frame("", resp.rows)?);
                            }
                        }
                        frames.push(ok_frame(&return_code)?);
                        Ok(frames)
                    }
                }
            }
        }
    }

    pub(crate) fn sync_self_server_groups_from_runtime(&mut self, runtime: &BaselineRuntime) {
        if let Some(online_client) = runtime.store.online_clients.get(&self.client_id) {
            let groups = if online_client.server_groups.is_empty() {
                String::from("8")
            } else {
                online_client
                    .server_groups
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            self.self_client_state
                .insert(String::from("client_servergroups"), groups);
        }
    }
}
