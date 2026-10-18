use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_clientpoke(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        if target_client_id == session.client_id {
            return QueryResponse::error(512, "cannot poke yourself");
        }
        if self.online_client_identity(server_id, target_client_id).is_none() {
            return QueryResponse::error(768, "target client not found");
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientkick(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(reason_id) = request
            .named_args
            .get("reasonid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "reasonid is required");
        };
        let reason_message = request.named_args.get("reasonmsg").cloned().unwrap_or_default();
        if reason_message.len() > 40 {
            return QueryResponse::error(512, "reasonmsg is too long");
        }

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let (target_snapshot, target_permissions) =
            match self.target_client_snapshot_and_permissions(server_id, target_client_id) {
                Ok(target) => target,
                Err(response) => return response,
            };

        match reason_id {
            4 => {
                if let Some(response) = self.check_target_client_power(
                    &actor_permissions,
                    &target_permissions,
                    &["i_client_kick_from_channel_power"],
                    &["i_client_needed_kick_from_channel_power"],
                    "i_client_kick_from_channel_power",
                ) {
                    return response;
                }

                let Some(default_channel_id) = self.default_channel_id_for_server(server_id) else {
                    return QueryResponse::error(768, "target channel not found");
                };
                if target_snapshot.channel_id != default_channel_id {
                    self.clear_client_media_broadcasts(server_id, target_client_id);
                }
                let Some(target_client) = self.store.online_clients.get_mut(&target_client_id) else {
                    return QueryResponse::error(768, "target client not found");
                };
                target_client.channel_id = default_channel_id;
                if let Some(bot_id) = self.music_bot_id_by_client(server_id, target_client_id) {
                    self.sync_music_bot_client_state(bot_id);
                }
                QueryResponse::ok()
            }
            5 => {
                if let Some(response) = self.check_target_client_power(
                    &actor_permissions,
                    &target_permissions,
                    &["i_client_kick_from_server_power"],
                    &["i_client_needed_kick_from_server_power"],
                    "i_client_kick_from_server_power",
                ) {
                    return response;
                }

                self.remove_session_client_without_left_event(target_snapshot.id);
                QueryResponse::ok()
            }
            _ => QueryResponse::error(512, "reasonid must be 4 or 5"),
        }
    }

    pub(crate) fn handle_clientfind(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(pattern) = request.named_args.get("pattern") else {
            return QueryResponse::error(512, "pattern is required");
        };
        let pattern = pattern.to_ascii_lowercase();

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .filter(|client| client.nickname.to_ascii_lowercase().contains(&pattern))
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("client_nickname"), client.nickname.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientgetids(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| {
                client.server_id == server_id && client.unique_identifier == *client_uid
            })
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("cluid"), client.unique_identifier.clone());
                row.insert(String::from("name"), client.nickname.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientgetdbidfromuid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some((resolved_uid, client_database_id, _)) =
            self.lookup_client_identity_by_uid(server_id, client_uid)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetnamefromdbid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let Some((client_uid, resolved_database_id, name)) =
            self.lookup_client_identity_by_dbid(server_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), resolved_database_id.to_string());
        row.insert(String::from("cluid"), client_uid);
        row.insert(String::from("name"), name);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetnamefromuid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some((resolved_uid, client_database_id, name)) =
            self.lookup_client_identity_by_uid(server_id, client_uid)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        row.insert(String::from("name"), name);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetuidfromclid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(client) = self.online_client_by_id_in_server(server_id, client_id) else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("clid"), client.id.to_string());
        row.insert(String::from("cluid"), client.unique_identifier.clone());
        row.insert(String::from("nickname"), client.nickname.clone());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .map(|client| self.render_client_row(client, request, false))
            .collect::<Vec<_>>();
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        }

        let Some(client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(client) = self.store.online_clients.get(&client_id) else {
            return QueryResponse::error(768, "client not found");
        };

        QueryResponse::ok_row(self.render_client_row(client, request, true))
    }

    pub(crate) fn handle_clientupdate(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if request.named_args.is_empty() {
            return QueryResponse::error(512, "at least one client property is required");
        }

        self.sync_session_client(session, "clientupdate");

        let mut applied = false;

        if let Some(client_nickname) = request.named_args.get("client_nickname") {
            session.client_nickname = client_nickname.clone();
            applied = true;
        }

        if let Some(client_away) = request.named_args.get("client_away") {
            let Some(client_away) = parse_query_bool(client_away) else {
                return QueryResponse::error(512, "client_away must be 0 or 1");
            };
            session.client_away = client_away;
            applied = true;
        }

        if let Some(client_away_message) = request.named_args.get("client_away_message") {
            session.client_away_message = client_away_message.clone();
            applied = true;
        }

        if let Some(client_input_muted) = request.named_args.get("client_input_muted") {
            let Some(client_input_muted) = parse_query_bool(client_input_muted) else {
                return QueryResponse::error(512, "client_input_muted must be 0 or 1");
            };
            session.client_input_muted = client_input_muted;
            applied = true;
        }

        if let Some(client_output_muted) = request.named_args.get("client_output_muted") {
            let Some(client_output_muted) = parse_query_bool(client_output_muted) else {
                return QueryResponse::error(512, "client_output_muted must be 0 or 1");
            };
            session.client_output_muted = client_output_muted;
            applied = true;
        }

        let target_online_client_id = if self.store.online_clients.contains_key(&session.client_id) {
            Some(session.client_id)
        } else if let (Some(server_id), Some(cldbid)) = (
            session.selected_virtual_server_id,
            session.actor_client_database_id_override,
        ) {
            self.online_client_id_by_database_id(server_id, cldbid)
        } else {
            None
        };

        let mut extra_updates = BTreeMap::new();
        if let Some(client_flag_avatar) = request.named_args.get("client_flag_avatar") {
            extra_updates.insert(String::from("client_flag_avatar"), client_flag_avatar.clone());
            if let Some(online_id) = target_online_client_id
                && let Some(online_client) = self.store.online_clients.get(&online_id)
            {
                let db_id = online_client.database_id;
                if let Some(client) = self.store.clients.get_mut(&db_id) {
                    client.client_flag_avatar = client_flag_avatar.clone();
                    let _ = self.store.db.update_client_avatar(&client.unique_identifier, client_flag_avatar);
                }
            }
            applied = true;
        }
        if let Some(talk_request) = request.named_args.get("client_talk_request") {
            let requested = runtime_bool_flag(talk_request)
                || talk_request.parse::<i64>().is_ok_and(|v| v > 0);
            extra_updates.insert(
                String::from("client_talk_request"),
                if requested {
                    String::from("1")
                } else {
                    String::from("0")
                },
            );
            if !requested && !request.named_args.contains_key("client_talk_request_msg") {
                extra_updates.insert(String::from("client_talk_request_msg"), String::new());
            }
            applied = true;
        }
        if let Some(talk_request_msg) = request.named_args.get("client_talk_request_msg") {
            extra_updates.insert(
                String::from("client_talk_request_msg"),
                talk_request_msg.clone(),
            );
            if !request.named_args.contains_key("client_talk_request") && !talk_request_msg.is_empty() {
                extra_updates.insert(String::from("client_talk_request"), String::from("1"));
            }
            applied = true;
        }
        if let Some(description) = request.named_args.get("client_description") {
            extra_updates.insert(String::from("client_description"), description.clone());
            if let Some(online_id) = target_online_client_id
                && let Some(online_client) = self.store.online_clients.get(&online_id)
            {
                let db_id = online_client.database_id;
                if let Some(client) = self.store.clients.get_mut(&db_id) {
                    client.description = description.clone();
                }
            }
            applied = true;
        }
        if let Some(channel_commander) = request.named_args.get("client_is_channel_commander") {
            let enabled = runtime_bool_flag(channel_commander);
            extra_updates.insert(
                String::from("client_is_channel_commander"),
                if enabled {
                    String::from("1")
                } else {
                    String::from("0")
                },
            );
            applied = true;
        }
        for passthrough_key in [
            "client_outputonly_muted",
            "client_input_hardware",
            "client_output_hardware",
            "client_input_deactivated",
            "client_is_recording",
            "client_nickname_phonetic",
            "client_meta_data",
            "client_flag_talking",
            "client_badges",
        ] {
            if let Some(val) = request.named_args.get(passthrough_key) {
                extra_updates.insert(String::from(passthrough_key), val.clone());
                applied = true;
            }
        }

        if let Some(online_id) = target_online_client_id
            && let Some(online_client) = self.store.online_clients.get_mut(&online_id)
        {
            for (k, v) in &extra_updates {
                online_client.extra_properties.insert(k.clone(), v.clone());
            }
        }

        if !applied {
            return QueryResponse::error(512, "no supported client properties provided");
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientedit(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(target_snapshot) = self.online_client_snapshot(server_id, target_client_id) else {
            return QueryResponse::error(768, "target client not found");
        };

        if request.named_args.contains_key("client_is_talker") {
            let channel_perms = self
                .effective_permissions_for_client_in_channel_context(
                    server_id,
                    target_snapshot.channel_id,
                    actor.client_database_id,
                )
                .unwrap_or_else(|| actor_permissions.clone());
            if let Err(permission_name) = check_required_permission(
                &channel_perms,
                &[
                    "b_client_set_flag_talker",
                    "b_channel_modify_needed_talk_power",
                    "i_channel_modify_power",
                    "b_permission_modify_power_ignore",
                ],
                "b_client_set_flag_talker",
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }

        if request.named_args.contains_key("client_description")
            && target_snapshot.database_id != actor.client_database_id
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &[
                    "b_client_modify_description",
                    "b_permission_modify_power_ignore",
                ],
                "b_client_modify_description",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let mut changes = BTreeMap::new();
        let Some(target_client) = self.store.online_clients.get_mut(&target_client_id) else {
            return QueryResponse::error(768, "target client not found");
        };

        if let Some(nickname) = request.named_args.get("client_nickname") {
            target_client.nickname = nickname.clone();
            changes.insert(String::from("client_nickname"), nickname.clone());
            if target_client_id == session.client_id {
                session.client_nickname = nickname.clone();
            }
        }
        if let Some(description) = request.named_args.get("client_description") {
            target_client
                .extra_properties
                .insert(String::from("client_description"), description.clone());
            changes.insert(String::from("client_description"), description.clone());
            let db_id = target_client.database_id;
            if let Some(db_client) = self.store.clients.get_mut(&db_id) {
                db_client.description = description.clone();
            }
        }
        if let Some(is_talker) = request.named_args.get("client_is_talker") {
            let talker_flag = runtime_bool_flag(is_talker);
            let val_str = if talker_flag {
                String::from("1")
            } else {
                String::from("0")
            };
            target_client
                .extra_properties
                .insert(String::from("client_is_talker"), val_str.clone());
            changes.insert(String::from("client_is_talker"), val_str);
            if talker_flag {
                target_client
                    .extra_properties
                    .insert(String::from("client_talk_request"), String::from("0"));
                target_client
                    .extra_properties
                    .insert(String::from("client_talk_request_msg"), String::new());
                changes.insert(String::from("client_talk_request"), String::from("0"));
                changes.insert(String::from("client_talk_request_msg"), String::new());
            }
        }
        if let Some(is_commander) = request.named_args.get("client_is_channel_commander") {
            let commander_flag = runtime_bool_flag(is_commander);
            let val_str = if commander_flag {
                String::from("1")
            } else {
                String::from("0")
            };
            target_client
                .extra_properties
                .insert(String::from("client_is_channel_commander"), val_str.clone());
            changes.insert(String::from("client_is_channel_commander"), val_str);
        }
        if let Some(is_prio) = request.named_args.get("client_is_priority_speaker") {
            let prio_flag = runtime_bool_flag(is_prio);
            let val_str = if prio_flag {
                String::from("1")
            } else {
                String::from("0")
            };
            target_client
                .extra_properties
                .insert(String::from("client_is_priority_speaker"), val_str.clone());
            changes.insert(String::from("client_is_priority_speaker"), val_str);
        }
        if let Some(version) = request.named_args.get("client_version") {
            target_client.version = version.clone();
            changes.insert(String::from("client_version"), version.clone());
        }
        if let Some(platform) = request.named_args.get("client_platform") {
            target_client.platform = platform.clone();
            changes.insert(String::from("client_platform"), platform.clone());
        }
        if let Some(country) = request.named_args.get("client_country") {
            target_client.country = country.clone();
            target_client
                .extra_properties
                .insert(String::from("client_country"), country.clone());
            changes.insert(String::from("client_country"), country.clone());
        }
        if let Some(vol) = request.named_args.get("player_volume") {
            let normalized = Self::format_music_bot_volume_factor(
                Self::parse_music_bot_volume_factor(vol),
            );
            target_client
                .extra_properties
                .insert(String::from("player_volume"), normalized.clone());
            changes.insert(String::from("player_volume"), normalized);
        }
        for key in [
            "client_bot_type",
            "client_uptime_mode",
            "client_flag_notify_song_change",
            "client_disabled",
        ] {
            if let Some(val) = request.named_args.get(key) {
                let trimmed = val.trim().to_string();
                target_client
                    .extra_properties
                    .insert(String::from(key), trimmed.clone());
                changes.insert(String::from(key), trimmed);
            }
        }

        if let Some(bot_id) = self.music_bot_id_by_client(server_id, target_client_id) {
            let mut seek_target_secs = None;
            if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                if let Some(vol) = changes.get("player_volume") {
                    if bot.player_volume != *vol {
                        bot.player_volume = vol.clone();
                        if matches!(bot.state, MusicBotState::Playing) {
                            let replay_ms = Self::music_bot_current_song_replay_millis(bot);
                            bot.current_song_progress_millis = replay_ms;
                            bot.current_song_started_at_millis =
                                Some(current_unix_timestamp_millis());
                            seek_target_secs = Some(replay_ms / 1000);
                        }
                    }
                }
                if let Some(nick) = changes.get("client_nickname") {
                    bot.nickname = nick.clone();
                }
            }
            if let Some(secs) = seek_target_secs {
                crate::runtime::musicbot::audio_engine::request_bot_seek(bot_id, secs);
            }
            let _ = self.sync_music_bot_client_properties(bot_id);
        }

        if changes.is_empty() {
            return QueryResponse::error(512, "no supported clientedit properties provided");
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientmove(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };

        let Some(target_channel) = self.channel_by_id(server_id, target_channel_id).cloned() else {
            return QueryResponse::error(768, "target channel not found");
        };

        let requested_client_id = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|&id| id != 0);

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let actor_target_channel_perms = self
            .effective_permissions_for_client_in_channel_context(
                server_id,
                target_channel_id,
                actor.client_database_id,
            )
            .unwrap_or_else(|| actor_permissions.clone());

        let moving_client_id = requested_client_id.unwrap_or(session.client_id);
        let is_moving_other_client = if let Some(req_clid) = requested_client_id {
            if req_clid == session.client_id {
                false
            } else if let Some(target_online) = self.store.online_clients.get(&req_clid) {
                target_online.database_id != actor.client_database_id
            } else {
                true
            }
        } else {
            false
        };

        if is_moving_other_client {
            let req_clid = requested_client_id.unwrap();
            let (_target_snapshot, target_permissions) =
                match self.target_client_snapshot_and_permissions(server_id, req_clid) {
                    Ok(target) => target,
                    Err(response) => return response,
                };
            if let Some(response) = self.check_target_client_power(
                &actor_permissions,
                &target_permissions,
                &["i_client_move_power"],
                &["i_client_needed_move_power"],
                "i_client_move_power",
            ) {
                return response;
            }
        } else if let Some(req_clid) = requested_client_id
            && req_clid != session.client_id
            && !self.store.online_clients.contains_key(&req_clid)
        {
            return QueryResponse::error(768, "target client not found");
        }

        let current_channel_of_target = self
            .store
            .online_clients
            .get(&moving_client_id)
            .map(|c| c.channel_id)
            .or(if !is_moving_other_client {
                session.current_channel_id
            } else {
                None
            });

        let already_in_target_channel = current_channel_of_target == Some(target_channel_id);

        if !already_in_target_channel {
            // 1. Check i_channel_needed_join_power vs i_channel_join_power
            let needed_join_power = permission_value_or_default(
                &actor_target_channel_perms,
                &["i_channel_needed_join_power"],
            );
            let ignore_join_power = permission_value_or_default(
                &actor_target_channel_perms,
                &[
                    "b_channel_join_ignore_join_power",
                    "b_permission_modify_power_ignore",
                ],
            ) > 0;
            if needed_join_power > 0 && !ignore_join_power {
                let join_power = permission_power_max_or_default(
                    &actor_target_channel_perms,
                    &["i_channel_join_power"],
                );
                if join_power < needed_join_power {
                    return self.insufficient_permission_response("i_channel_join_power");
                }
            }

            // 2. Check channel password (cpw)
            let channel_requires_password =
                target_channel.flag_password && !target_channel.password.is_empty();
            if channel_requires_password {
                let provided_cpw = request
                    .named_args
                    .get("cpw")
                    .map(String::as_str)
                    .unwrap_or("");
                let ignore_password = permission_value_or_default(
                    &actor_target_channel_perms,
                    &["b_channel_join_ignore_password"],
                ) > 0;
                if !ignore_password && provided_cpw != target_channel.password {
                    return QueryResponse::error(781, "invalid channel password");
                }
            }

            // 3. Check channel_maxclients and channel_maxfamilyclients
            let ignore_maxclients = permission_value_or_default(
                &actor_target_channel_perms,
                &["b_channel_join_ignore_maxclients"],
            ) > 0;
            if !ignore_maxclients {
                if target_channel.maxclients >= 0 {
                    let current_count =
                        self.client_count_in_channel(server_id, target_channel_id) as i32;
                    if current_count >= target_channel.maxclients {
                        return QueryResponse::error(777, "channel maxclient reached");
                    }
                }
                let mut ancestor_cid = Some(target_channel_id);
                while let Some(check_cid) = ancestor_cid {
                    let Some(check_ch) = self.channel_by_id(server_id, check_cid) else {
                        break;
                    };
                    let max_family = check_ch.maxfamilyclients;
                    let next_parent = (check_ch.parent_id != 0).then_some(check_ch.parent_id);
                    if max_family >= 0 {
                        let current_family_count =
                            self.family_client_count_in_channel(server_id, check_cid) as i32;
                        if current_family_count >= max_family {
                            return QueryResponse::error(778, "channel maxfamily reached");
                        }
                    }
                    ancestor_cid = next_parent;
                }
            }
        }

        if is_moving_other_client || requested_client_id.is_some_and(|id| id != session.client_id) {
            let req_clid = requested_client_id.unwrap();
            let old_channel_id = self.store.online_clients.get(&req_clid).map(|c| c.channel_id);
            if old_channel_id.is_none() {
                return QueryResponse::error(768, "target client not found");
            }
            if old_channel_id != Some(target_channel_id) {
                self.clear_client_media_broadcasts(server_id, req_clid);
            }
            let Some(target_client) = self.store.online_clients.get_mut(&req_clid) else {
                return QueryResponse::error(768, "target client not found");
            };
            if target_client.channel_id != target_channel_id {
                target_client
                    .extra_properties
                    .remove("client_is_talker");
                target_client
                    .extra_properties
                    .remove("client_talk_request");
                target_client
                    .extra_properties
                    .remove("client_talk_request_msg");
            }
            target_client.channel_id = target_channel_id;
            if let Some(bot_id) = self.music_bot_id_by_client(server_id, req_clid) {
                self.sync_music_bot_client_state(bot_id);
            }
            if !is_moving_other_client {
                session.current_channel_id = Some(target_channel_id);
            }
            return QueryResponse::ok();
        }

        let old_self_channel = self.store.online_clients.get(&session.client_id).map(|c| c.channel_id);
        if old_self_channel.is_some_and(|old_cid| old_cid != target_channel_id) {
            self.clear_client_media_broadcasts(server_id, session.client_id);
        }
        if let Some(target_client) = self.store.online_clients.get_mut(&session.client_id) {
            if target_client.channel_id != target_channel_id {
                target_client
                    .extra_properties
                    .remove("client_is_talker");
                target_client
                    .extra_properties
                    .remove("client_talk_request");
                target_client
                    .extra_properties
                    .remove("client_talk_request_msg");
            }
            target_client.channel_id = target_channel_id;
        }
        session.current_channel_id = Some(target_channel_id);
        QueryResponse::ok()
    }

    pub(crate) fn handle_clientsetserverquerylogin(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let Some(current_login) = session.authenticated_login.clone() else {
            return QueryResponse::error(521, "login required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_create_modify_serverquery_login"],
            "b_client_create_modify_serverquery_login",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(new_login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        if self.store.query_accounts.contains_key(&new_login_name) {
            return QueryResponse::error(769, "target query account already exists");
        }

        let Some(mut account) = self.store.query_accounts.remove(&current_login) else {
            return QueryResponse::error(768, "current query account not found");
        };

        account.login_name = new_login_name.clone();
        account.password = format!("generated-{}", new_login_name);
        session.authenticated_login = Some(new_login_name.clone());
        self.store
            .query_accounts
            .insert(new_login_name.clone(), account.clone());
        if let Some(snapshot) = self.session_snapshots.remove(&current_login) {
            self.session_snapshots
                .insert(new_login_name.clone(), snapshot);
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), new_login_name);
        row.insert(String::from("client_login_password"), account.password);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetvariables(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let mut requested_clids = Vec::new();
        for group in &request.option_groups {
            if let Some(clid) = group.get("clid").and_then(|v| v.parse::<u64>().ok()) {
                requested_clids.push(clid);
            }
        }
        if requested_clids.is_empty() {
            let clid = request
                .named_args
                .get("clid")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(session.client_id);
            requested_clids.push(clid);
        }

        let mut rows = Vec::new();
        for clid in requested_clids {
            if let Some(row) = self.web_client_variables_row(server_id, clid) {
                rows.push(row);
            }
        }
        if rows.is_empty() {
            return QueryResponse::error(512, "invalid clientID");
        }
        QueryResponse::ok_rows(rows)
    }

}


