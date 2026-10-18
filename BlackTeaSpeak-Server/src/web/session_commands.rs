use super::*;
use std::collections::BTreeSet;
use std::io;
use std::sync::Arc;
use anyhow::Result;

use crate::file_transfer::FileTransferRegistry;
use crate::runtime::{BaselineRuntime, MusicBotNotifyPayload, QuerySessionState};

impl BlackTeaWebSessionHandler {
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
        update_row.extend(
            payload
                .client_updates
                .into_iter()
                .filter(|(k, _)| !k.starts_with('_')),
        );

        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id,
            exclude_client_id: None,
            frame: command_frame("notifyclientupdated", vec![update_row])?,
        });
        self.pending_broadcasts.push(BlackTeaWebFrameBroadcast::Server {
            server_id,
            exclude_client_id: None,
            frame: command_frame("notifymusicplayerinfo", vec![payload.player_info])?,
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
