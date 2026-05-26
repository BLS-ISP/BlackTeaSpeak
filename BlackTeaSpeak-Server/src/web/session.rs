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
    pub(crate) data: Vec<Map<String, Value>>,
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
            _ => Ok(vec![ok_frame(&return_code)?]),
        }
    }
    pub(crate) fn handle_identity_proof(&mut self, return_code: &str) -> Result<Vec<String>> {
        if self.login_phase != LoginPhase::AwaitIdentityProof {
            return Ok(vec![error_frame(
                return_code,
                ERROR_PROTOCOL_VIOLATION,
                "identity proof not expected",
                None,
            )?]);
        }

        self.login_phase = LoginPhase::AwaitClientInit;
        Ok(vec![ok_frame(return_code)?])
    }
    pub(crate) fn handle_whoami(&self, return_code: &str) -> Result<Vec<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        };

        let mut row = row_map([
            ("clid", self.client_id.to_string()),
            ("virtualserver_id", server_id.to_string()),
            ("client_nickname", self.self_client_nickname()),
        ]);
        if let Some(channel_id) = self.current_channel_id {
            row.insert(String::from("client_channel_id"), channel_id.to_string());
        }

        let data_frame = if self.raw_commands_enabled {
            raw_command_frame(None, &[row])?
        } else {
            command_frame("", vec![row])?
        };

        Ok(vec![data_frame, ok_frame(return_code)?])
    }

    pub(crate) fn handle_server_info(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        };
        let Some(row) = runtime.web_server_variables_row(server_id) else {
            return Ok(vec![error_frame(
                return_code,
                522,
                "virtual server selection required",
                None,
            )?]);
        };

        Ok(vec![
            command_frame("serverinfo", vec![row])?,
            ok_frame(return_code)?,
        ])
    }

    pub(crate) fn handle_server_get_variables(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        };
        let Some(row) = runtime.web_server_variables_row(server_id) else {
            return Ok(vec![error_frame(
                return_code,
                522,
                "virtual server selection required",
                None,
            )?]);
        };

        Ok(vec![
            command_frame("notifyserverupdated", vec![row])?,
            ok_frame(return_code)?,
        ])
    }

    pub(crate) fn handle_server_group_list(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        if self.login_phase != LoginPhase::Connected {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        }

        Ok(vec![
            command_frame("notifyservergrouplist", runtime.web_server_group_rows())?,
            ok_frame(return_code)?,
        ])
    }

    pub(crate) fn handle_channel_group_list(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        if self.login_phase != LoginPhase::Connected {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        }

        Ok(vec![
            command_frame("notifychannelgrouplist", runtime.web_channel_group_rows())?,
            ok_frame(return_code)?,
        ])
    }






    pub(crate) fn queue_music_bot_notify_payload(
        &mut self,
        server_id: u32,
        payload: MusicBotNotifyPayload,
    ) -> Result<()> {
        let mut update_row = row_map([("clid", payload.client_id.to_string())]);
        update_row.extend(payload.client_updates);

        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id,
            exclude_client_id: None,
            frame: command_frame("notifyclientupdated", vec![update_row])?,
        });
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id,
            exclude_client_id: None,
            frame: command_frame("notifymusicplayersongchange", vec![payload.song_change])?,
        });
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id,
            exclude_client_id: None,
            frame: command_frame("notifymusicstatusupdate", vec![payload.status_update])?,
        });
        Ok(())
    }

    pub(crate) fn playlist_song_add_notify_rows(rows: &[CommandRow]) -> Vec<CommandRow> {
        rows.iter()
            .map(|row| {
                let mut row = row.clone();
                row.insert(String::from("song_loaded"), String::from("0"));
                row
            })
            .collect()
    }

    pub(crate) fn playlist_song_loaded_notify_rows(rows: &[CommandRow]) -> Vec<CommandRow> {
        rows.iter()
            .filter(|row| row.get("song_id").is_some() && row.get("playlist_id").is_some())
            .filter(|row| row.get("song_loaded").map(String::as_str) == Some("1"))
            .map(|row| {
                row_map([
                    (
                        "playlist_id",
                        row.get("playlist_id").cloned().unwrap_or_default(),
                    ),
                    ("song_id", row.get("song_id").cloned().unwrap_or_default()),
                    ("success", String::from("1")),
                    (
                        "song_metadata",
                        row.get("song_metadata").cloned().unwrap_or_default(),
                    ),
                ])
            })
            .collect()
    }


    pub(crate) fn handle_channel_list(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        if self.login_phase != LoginPhase::Connected {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        }

        let mut outbound = self.connected_channel_list_frames(runtime)?;
        outbound.push(command_frame("channellistfinished", Vec::new())?);
        outbound.push(ok_frame(return_code)?);
        Ok(outbound)
    }

    pub(crate) fn handle_server_connection_info(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        };

        Ok(vec![
            command_frame(
                "notifyserverconnectioninfo",
                vec![runtime.web_connection_info_row(server_id)],
            )?,
            ok_frame(return_code)?,
        ])
    }

    pub(crate) fn handle_permission_list(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        if self.login_phase != LoginPhase::Connected {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        }

        Ok(vec![
            command_frame("notifypermissionlist", runtime.web_permission_rows())?,
            ok_frame(return_code)?,
        ])
    }

    pub(crate) fn handle_feature_list(
        &self,
        return_code: &str,
        runtime: &BaselineRuntime,
    ) -> Result<Vec<String>> {
        if self.login_phase != LoginPhase::Connected {
            return Ok(vec![error_frame(
                return_code,
                ERROR_NOT_CONNECTED,
                "not connected",
                None,
            )?]);
        }

        Ok(vec![
            command_frame("notifyfeaturesupport", runtime.web_feature_rows())?,
            ok_frame(return_code)?,
        ])
    }

    pub(crate) fn connected_channel_list_frames(&self, runtime: &BaselineRuntime) -> Result<Vec<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(Vec::new());
        };
        let visible_channel_ids = self.visible_channel_ids(runtime);

        Ok(vec![command_frame(
            "channellist",
            runtime.web_channel_rows_for_visibility(server_id, &visible_channel_ids),
        )?])
    }


    pub(crate) fn queue_plugin_frame_to_channel(&self, channel_id: u32, frame: String) -> Result<()> {
        let Some(sessions) = self.sessions.as_ref() else {
            return Ok(());
        };

        let recipients = {
            let sessions = sessions
                .lock()
                .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
            sessions
                .values()
                .filter(|session| {
                    session.presence.server_id == self.selected_server_id.unwrap_or(1)
                        && session.presence.channel_id == channel_id
                })
                .map(|session| Arc::clone(&session.pending_frames))
                .collect::<Vec<_>>()
        };

        for pending_frames in recipients {
            pending_frames
                .lock()
                .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?
                .push(frame.clone());
        }

        Ok(())
    }

    pub(crate) fn queue_plugin_frame_to_server(&mut self, server_id: u32, frame: String) -> Result<()> {
        let Some(sessions) = self.sessions.as_ref() else {
            self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                server_id,
                exclude_client_id: None,
                frame,
            });
            return Ok(());
        };

        let recipients = {
            let sessions = sessions
                .lock()
                .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
            sessions
                .values()
                .filter(|session| session.presence.server_id == server_id)
                .map(|session| Arc::clone(&session.pending_frames))
                .collect::<Vec<_>>()
        };

        for pending_frames in recipients {
            pending_frames
                .lock()
                .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?
                .push(frame.clone());
        }

        Ok(())
    }

    pub(crate) fn queue_plugin_frame_to_client(&mut self, client_id: u64, frame: String) -> Result<()> {
        let Some(sessions) = self.sessions.as_ref() else {
            self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Client { client_id, frame });
            return Ok(());
        };

        let pending_frames = {
            let sessions = sessions
                .lock()
                .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
            sessions.get(&client_id).map(|session| Arc::clone(&session.pending_frames))
        };

        if let Some(pending_frames) = pending_frames {
            pending_frames
                .lock()
                .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?
                .push(frame);
        }

        Ok(())
    }

    pub(crate) fn self_client_database_id(&self) -> Option<u64> {
        self.self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
    }

    pub(crate) fn web_query_session(&self) -> Option<QuerySessionState> {
        let actor_client_database_id_override = self.self_client_database_id()?;

        Some(QuerySessionState {
            actor_client_database_id_override: Some(actor_client_database_id_override),
            selected_virtual_server_id: self.selected_server_id,
            current_channel_id: Some(self.current_channel_id.unwrap_or(1)),
            connection_ip: self.connection_ip.clone(),
            ..QuerySessionState::default()
        })
    }

    pub(crate) fn file_transfer_registry(&self) -> Option<Arc<FileTransferRegistry>> {
        self.file_transfers.as_ref().map(Arc::clone)
    }

    pub(crate) fn current_actor_avatar_id(&self) -> Option<String> {
        self.self_client_state
            .get("client_unique_identifier")
            .and_then(|value| actor_avatar_id_from_unique_identifier(value))
    }
    pub(crate) fn connected_needed_permission_frame(
        &self,
        runtime: &BaselineRuntime,
    ) -> Result<Option<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(None);
        };
        let Some(client_database_id) = self.self_client_database_id() else {
            return Ok(None);
        };
        let channel_id = self.current_channel_id.unwrap_or(1);
        let Some(permission_rows) =
            runtime.web_client_needed_permission_rows(server_id, channel_id, client_database_id)
        else {
            return Ok(None);
        };

        Ok(Some(command_frame(
            "notifyclientneededpermissions",
            permission_rows,
        )?))
    }

    pub(crate) fn visible_channel_ids(&self, runtime: &BaselineRuntime) -> BTreeSet<u32> {
        let Some(server_id) = self.selected_server_id else {
            return BTreeSet::new();
        };
        let Some(client_database_id) = self.self_client_database_id() else {
            return BTreeSet::new();
        };

        runtime.web_visible_channel_ids_for_client(
            server_id,
            client_database_id,
            self.current_channel_id,
        )
    }

    pub(crate) fn connected_bootstrap_frames(&self, runtime: &BaselineRuntime) -> Result<Vec<String>> {
        let Some(server_id) = self.selected_server_id else {
            return Ok(Vec::new());
        };
        let visible_channel_ids = self.visible_channel_ids(runtime);

        let mut frames = vec![
            command_frame("notifyservergrouplist", runtime.web_server_group_rows())?,
            command_frame("notifychannelgrouplist", runtime.web_channel_group_rows())?,
        ];
        if let Some(frame) = self.connected_needed_permission_frame(runtime)? {
            frames.push(frame);
        }
        frames.push(command_frame(
            "channellist",
            runtime.web_channel_rows_for_visibility(server_id, &visible_channel_ids),
        )?);
        let mut visible_clients = Vec::new();
        visible_clients.push(self.self_enter_view_row());
        visible_clients.extend(runtime.web_visible_client_rows_excluding_in_channels(
            server_id,
            Some(self.client_id),
            &visible_channel_ids,
        ));
        frames.push(command_frame("notifycliententerview", visible_clients)?);
        frames.push(command_frame("channellistfinished", Vec::new())?);
        Ok(frames)
    }

    pub(crate) fn self_enter_view_row(&self) -> CommandRow {
        let mut row = self.self_client_state.clone();
        row.insert(String::from("clid"), self.client_id.to_string());
        row.insert(String::from("cfid"), String::from("0"));
        row.insert(
            String::from("ctid"),
            self.current_channel_id.unwrap_or(1).to_string(),
        );
        row.insert(String::from("reasonid"), String::from("2"));
        row
    }

    pub(crate) fn apply_self_client_updates(&mut self, row: &CommandRow) -> CommandRow {
        let mut updates = CommandRow::new();
        for key in [
            "client_nickname",
            "client_flag_avatar",
            "client_away",
            "client_away_message",
            "client_input_hardware",
            "client_output_hardware",
            "client_input_muted",
            "client_output_muted",
        ] {
            let Some(value) = row.get(key) else {
                continue;
            };

            let value = normalize_client_update_value(key, value);
            if key == "client_nickname" && value.is_empty() {
                continue;
            }
            if self.set_self_client_value(key, value.clone()) {
                updates.insert(String::from(key), value);
            }
        }

        if row.contains_key("client_away")
            && self
                .self_client_state
                .get("client_away")
                .is_some_and(|value| value == "0")
            && !row.contains_key("client_away_message")
            && self.set_self_client_value("client_away_message", String::new())
        {
            updates.insert(String::from("client_away_message"), String::new());
        }

        updates
    }

    pub(crate) fn set_self_client_value(&mut self, key: &str, value: String) -> bool {
        if self
            .self_client_state
            .get(key)
            .is_some_and(|current| current == &value)
        {
            return false;
        }

        self.self_client_state
            .insert(String::from(key), value.clone());
        if key == "client_nickname" {
            self.accepted_nickname = Some(value);
        }
        true
    }

    pub(crate) fn self_client_nickname(&self) -> String {
        self.self_client_state
            .get("client_nickname")
            .cloned()
            .unwrap_or_else(|| String::from("BlackTeaWeb User"))
    }

    pub(crate) fn self_server_group_ids(&self) -> Vec<u32> {
        self.self_client_state
            .get("client_servergroups")
            .map(|value| {
                value
                    .split(',')
                    .filter_map(|entry| entry.parse::<u32>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    pub(crate) fn sync_rtc_presence(&self) -> Result<()> {
        Ok(())
    }

    pub(crate) fn remove_from_rtc(&self) -> Result<()> {
        Ok(())
    }

    pub(crate) fn presence(&self) -> Option<BlackTeaWebPresence> {
        Some(BlackTeaWebPresence {
            client_id: self.client_id,
            server_id: self.selected_server_id?,
            channel_id: self.current_channel_id.unwrap_or(1),
            client_state: self.self_client_state.clone(),
        })
    }
}

pub(crate) fn blackteaweb_disconnect_kind(
    close_frame_received: bool,
    ping_timeout_triggered: bool,
) -> BlackTeaWebDisconnectKind {
    if ping_timeout_triggered || !close_frame_received {
        BlackTeaWebDisconnectKind::TimedOut
    } else {
        BlackTeaWebDisconnectKind::LeftServer
    }
}

pub(crate) fn blackteaweb_disconnect_reason(kind: BlackTeaWebDisconnectKind) -> (u32, &'static str) {
    match kind {
        BlackTeaWebDisconnectKind::LeftServer => (8, "left server"),
        BlackTeaWebDisconnectKind::TimedOut => (3, ""),
    }
}

pub(crate) fn handle_client(
    stream: TcpStream,
    runtime: Arc<Mutex<BaselineRuntime>>,
    tls_config: Arc<ServerConfig>,
    file_transfers: Arc<FileTransferRegistry>,
    sessions: SharedBlackTeaWebSessions,
    
    connection_id: u64,
) -> Result<()> {
    let connection_ip = stream
        .peer_addr()
        .map(|peer_addr| peer_addr.ip().to_string())
        .unwrap_or_default();
    if blackteaweb_trace_enabled() {
        match stream.peer_addr() {
            Ok(peer_addr) => eprintln!("[blackteaweb:{connection_id}] accepted {peer_addr}"),
            Err(error) => eprintln!(
                "[blackteaweb:{connection_id}] accepted client with unknown peer address: {error:#}"
            ),
        }
    }
    stream
        .set_nonblocking(false)
        .context("failed to switch BlackTeaWeb client socket to blocking mode")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .context("failed to set BlackTeaWeb client read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .context("failed to set BlackTeaWeb client write timeout")?;

    let tls_stream = StreamOwned::new(
        ServerConnection::new(Arc::clone(&tls_config))
            .context("failed to create TLS server connection")?,
        stream,
    );
    let mut websocket =
        accept(tls_stream).context("failed to accept BlackTeaWeb websocket connection")?;
    let mut session = BlackTeaWebSessionHandler::new_with_connection_ip(connection_id, connection_ip);
    session.set_file_transfers(file_transfers);
    session.set_sessions(Arc::clone(&sessions));
    let pending_frames = Arc::new(Mutex::new(Vec::new()));
    let mut close_frame_received = false;
    let mut ping_timeout_triggered = false;
    let mut last_activity = Instant::now();

    loop {
        flush_pending_frames(&mut websocket, &pending_frames)?;
        match websocket.read() {
            Ok(Message::Text(text)) => {
                last_activity = Instant::now();
                trace_blackteaweb_frame(connection_id, "in", text.as_ref());
                let before_presence = session.presence();
                let mut outbound = {
                    let mut runtime = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    runtime.mark_client_seen(session.client_id);
                    session.handle_text_frame(text.as_ref(), &mut runtime)?
                };
                let after_presence = session.presence();

                if let Some(after_presence) = after_presence.as_ref() {
                    let visible_channel_ids = {
                        let runtime = runtime
                            .lock()
                            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                        session.visible_channel_ids(&runtime)
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
                    let runtime = runtime
                        .lock()
                        .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
                    broadcast_permission_refreshes(
                        &sessions,
                        &runtime,
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
                    websocket
                        .send(Message::Text(message.into()))
                        .context("failed to write BlackTeaWeb websocket frame")?;
                }
            }
            Ok(Message::Ping(payload)) => {
                last_activity = Instant::now();
                if blackteaweb_trace_enabled() {
                    eprintln!("[blackteaweb:{connection_id}] ping {} bytes", payload.len());
                }
                if let Ok(mut rt) = runtime.lock() {
                    rt.mark_client_seen(session.client_id);
                }
                websocket
                    .send(Message::Pong(payload))
                    .context("failed to answer websocket ping")?;
            }
            Ok(Message::Close(frame)) => {
                close_frame_received = true;
                if blackteaweb_trace_enabled() {
                    eprintln!("[blackteaweb:{connection_id}] close frame: {frame:?}");
                }
                break;
            }
            Ok(_) => {
                last_activity = Instant::now();
            }
            Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => {
                if blackteaweb_trace_enabled() {
                    eprintln!("[blackteaweb:{connection_id}] websocket closed");
                }
                break;
            }
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                if last_activity.elapsed() >= TEAWEB_IDLE_TIMEOUT {
                    ping_timeout_triggered = true;
                    let _ = websocket.send(Message::Close(Some(CloseFrame {
                        code: CloseCode::Normal,
                        reason: TEAWEB_TIMEOUT_CLOSE_REASON.into(),
                    })));
                    break;
                }
                continue;
            }
            Err(error) => return Err(error).context("BlackTeaWeb websocket processing failed"),
        }
    }

    let disconnect_kind = blackteaweb_disconnect_kind(close_frame_received, ping_timeout_triggered);
    let (disconnect_reason_id, disconnect_reason_message) =
        blackteaweb_disconnect_reason(disconnect_kind);

    let disconnect_presence = session.presence();
    unregister_session(&sessions, session.client_id)?;
    session.remove_from_rtc()?;
    {
        let mut runtime = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        runtime.remove_session_client(session.client_id, disconnect_reason_id, disconnect_reason_message.to_string());
    }
    let disconnect_cleanup_notifications = {
        let mut runtime = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        match disconnect_presence.as_ref() {
            Some(presence) => _cleanup_notifications(
                presence.server_id,
                runtime.cleanup_temporary_channels(presence.server_id, &[presence.channel_id]),
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
        let runtime = runtime
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb runtime lock poisoned"))?;
        let cleanup_frames = visibility_aware_transport_broadcasts(
            &sessions,
            &runtime,
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

pub(crate) fn blackteaweb_trace_enabled() -> bool {
    matches!(
        std::env::var("TEASPEAK_TEAWEB_TRACE").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "True" | "yes" | "on")
    )
}

pub(crate) fn trace_blackteaweb_frame(connection_id: u64, direction: &str, payload: &str) {
    if blackteaweb_trace_enabled() {
        eprintln!("[blackteaweb:{connection_id}] {direction} {payload}");
    }
}

pub(crate) fn list_session_presences(
    sessions: &SharedBlackTeaWebSessions,
    server_id: u32,
    exclude_client_id: Option<u64>,
) -> Result<Vec<BlackTeaWebPresence>> {
    let sessions = sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
    let mut presences = sessions
        .values()
        .filter(|session| {
            session.presence.server_id == server_id
                && exclude_client_id != Some(session.presence.client_id)
        })
        .map(|session| session.presence.clone())
        .collect::<Vec<_>>();
    presences.sort_by(|left, right| {
        left.channel_id
            .cmp(&right.channel_id)
            .then_with(|| left.client_id.cmp(&right.client_id))
    });
    Ok(presences)
}

pub(crate) fn register_or_update_session(
    sessions: &SharedBlackTeaWebSessions,
    presence: BlackTeaWebPresence,
    client_database_id: u64,
    visible_channel_ids: BTreeSet<u32>,
    pending_frames: SharedPendingFrames,
) -> Result<()> {
    let mut lock = sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
    
    let existing_wtransport = lock.get(&presence.client_id).and_then(|s| s.wtransport_session.clone());
    
    lock.insert(
        presence.client_id,
        RegisteredBlackTeaWebSession {
            presence,
            client_database_id,
            visible_channel_ids,
            pending_frames,
            wtransport_session: existing_wtransport,
        },
    );
    Ok(())
}

pub(crate) fn unregister_session(sessions: &SharedBlackTeaWebSessions, client_id: u64) -> Result<()> {
    sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?
        .remove(&client_id);
    Ok(())
}

pub(crate) fn blackteaweb_presence_from_snapshot(snapshot: OnlineClientSnapshot) -> BlackTeaWebPresence {
    BlackTeaWebPresence {
        client_id: snapshot.id,
        server_id: snapshot.server_id,
        channel_id: snapshot.channel_id,
        client_state: row_map([
            ("client_nickname", snapshot.nickname),
            ("client_unique_identifier", snapshot.unique_identifier),
            ("client_type", snapshot.client_type.to_string()),
            ("client_type_exact", snapshot.client_type_exact.to_string()),
            ("client_database_id", snapshot.database_id.to_string()),
            (
                "client_servergroups",
                snapshot
                    .server_groups
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            ("client_version", snapshot.version),
            ("client_platform", snapshot.platform),
            ("client_country", snapshot.country),
            (
                "client_away",
                if snapshot.away {
                    String::from("1")
                } else {
                    String::from("0")
                },
            ),
            ("client_away_message", snapshot.away_message),
            (
                "client_input_muted",
                if snapshot.input_muted {
                    String::from("1")
                } else {
                    String::from("0")
                },
            ),
            (
                "client_output_muted",
                if snapshot.output_muted {
                    String::from("1")
                } else {
                    String::from("0")
                },
            ),
            ("connection_client_ip", snapshot.connection_ip),
        ]),
    }
}

pub(crate) fn blackteaweb_presence_from_transport_presence(presence: &SessionPresence) -> BlackTeaWebPresence {
    BlackTeaWebPresence {
        client_id: presence.client_id,
        server_id: presence.server_id,
        channel_id: presence.channel_id,
        client_state: row_map([
            ("client_nickname", presence.login_name.clone()),
            (
                "client_unique_identifier",
                presence.unique_identifier.clone(),
            ),
            ("client_type", presence.client_type.to_string()),
            ("client_type_exact", presence.client_type.to_string()),
            ("client_database_id", String::from("0")),
            ("client_servergroups", String::new()),
            ("client_version", String::new()),
            ("client_platform", String::new()),
            ("client_country", String::new()),
            ("connection_client_ip", String::new()),
        ]),
    }
}

pub(crate) fn session_presence_from_blackteaweb_presence(presence: &BlackTeaWebPresence) -> SessionPresence {
    SessionPresence {
        client_id: presence.client_id,
        login_name: presence
            .client_state
            .get("client_nickname")
            .cloned()
            .unwrap_or_else(|| String::from("BlackTeaWeb User")),
        unique_identifier: presence
            .client_state
            .get("client_unique_identifier")
            .cloned()
            .unwrap_or_else(|| format!("compat-web-{}", presence.client_id)),
        client_type: presence
            .client_state
            .get("client_type")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0),
        server_id: presence.server_id,
        channel_id: presence.channel_id,
    }
}

pub(crate) fn parse_whisper_target_selection(rows: &[CommandRow]) -> Result<(u32, crate::models::WhisperTargetSelection)> {
    let Some(first_row) = rows.first() else {
        return Err(anyhow!("missing whispersessioninitialize payload"));
    };
    let Some(ssrc) = first_row
        .get("ssrc")
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return Err(anyhow!("missing whisper ssrc"));
    };

    let mut selection = crate::models::WhisperTargetSelection::default();
    for row in rows {
        let row_ssrc = row
            .get("ssrc")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(ssrc);
        if row_ssrc != ssrc {
            return Err(anyhow!("whisper ssrc must stay consistent"));
        }

        let target_type = row
            .get("type")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or_default();
        let target = row
            .get("target")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_default();
        let id = row
            .get("id")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_default();

        match target_type {
            WHISPER_TARGET_SELF => {
                if !(target == 0 && id == 0) {
                    return Err(anyhow!("invalid echo whisper target"));
                }
                selection.echo_self = true;
            }
            WHISPER_TARGET_CHANNEL => {
                let channel_id = u32::try_from(target)
                    .map_err(|_| anyhow!("invalid whisper channel target"))?;
                if channel_id == 0 {
                    return Err(anyhow!("invalid whisper channel target"));
                }
                selection.channel_ids.insert(channel_id);
            }
            WHISPER_TARGET_CLIENT => {
                if target == 0 {
                    return Err(anyhow!("invalid whisper client target"));
                }
                selection.client_ids.insert(target);
            }
            WHISPER_TARGET_SERVER_GROUP => {
                let group_id = u32::try_from(target)
                    .map_err(|_| anyhow!("invalid whisper server-group target"))?;
                if group_id == 0 {
                    return Err(anyhow!("invalid whisper server-group target"));
                }
                selection.server_group_ids.insert(group_id);
            }
            other => return Err(anyhow!("unsupported whisper target {other}")),
        }
    }

    if selection.is_empty() {
        return Err(anyhow!("missing whisper target"));
    }

    Ok((ssrc, selection))
}

pub(crate) fn flush_pending_frames(
    websocket: &mut tungstenite::WebSocket<StreamOwned<ServerConnection, TcpStream>>,
    pending_frames: &SharedPendingFrames,
) -> Result<()> {
    for frame in drain_pending_frames(pending_frames)? {
        websocket
            .send(Message::Text(frame.into()))
            .context("failed to flush queued BlackTeaWeb websocket frame")?;
    }
    Ok(())
}

pub(crate) fn drain_pending_frames(pending_frames: &SharedPendingFrames) -> Result<Vec<String>> {
    let mut pending_frames = pending_frames
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?;
    Ok(pending_frames.drain(..).collect())
}

pub(crate) fn insert_frames_before_error(outbound: &mut Vec<String>, mut extra_frames: Vec<String>) {
    if extra_frames.is_empty() {
        return;
    }

    let insert_at = outbound
        .iter()
        .rposition(|frame| frame.contains("\"command\":\"error\""))
        .unwrap_or(outbound.len());
    outbound.splice(insert_at..insert_at, extra_frames.drain(..));
}

