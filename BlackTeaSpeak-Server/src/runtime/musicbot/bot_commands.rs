use std::collections::BTreeMap;
use crate::runtime::*;
use super::*;

impl BaselineRuntime {
    pub(crate) fn handle_musicbotcreate(
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
        let target_channel_id = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
            .or(session.current_channel_id)
            .or_else(|| self.default_channel_id_for_server(server_id));
        let Some(target_channel_id) = target_channel_id else {
            return QueryResponse::error(768, "target channel not found");
        };
        if !self.channel_exists(server_id, target_channel_id) {
            return QueryResponse::error(768, "target channel not found");
        }

        let bot_id = self
            .store
            .music_bots
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let client_id = self.allocate_query_client_id();
        let client_database_id = self.allocate_client_database_id();
        let nickname = request
            .named_args
            .get("botname")
            .or_else(|| request.named_args.get("bot_name"))
            .or_else(|| request.named_args.get("name"))
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| format!("MusicBot {bot_id}"));
        let music_bot = MusicBot {
            id: bot_id,
            server_id,
            client_database_id,
            linked_client_id: Some(client_id),
            nickname: nickname.clone(),
            channel_id: target_channel_id,
            playlist_id: bot_id,
            current_song_id: None,
            next_song_id: 1,
            state: MusicBotState::Stopped,
            player_volume: String::from("1"),
            playlist_title: String::new(),
            playlist_description: String::new(),
            playlist_flag_delete_played: false,
            playlist_flag_finished: false,
            playlist_replay_mode: 0,
            playlist_max_songs: 0,
            permissions: BTreeMap::new(),
            client_permissions: Vec::new(),
            current_song_started_at_millis: None,
            current_song_progress_millis: 0,
            queue: Vec::new(),
        };

        self.store.online_clients.insert(
            client_id,
            OnlineClient {
                id: client_id,
                database_id: client_database_id,
                unique_identifier: format!("compat-musicbot-{client_database_id}"),
                nickname: nickname.clone(),
                away: false,
                away_message: String::new(),
                input_muted: false,
                output_muted: false,
                server_id,
                channel_id: target_channel_id,
                client_type: 0,
                version: String::from("BlackTeaSpeak 1.5.6 compat-musicbot"),
                platform: String::from("compat-rust"),
                country: String::from("ZZ"),
                connection_ip: String::new(),
                server_groups: vec![8],
                connected_at: current_unix_timestamp(),
                last_seen_at: current_unix_timestamp(),
                whisper_targets: None,
                extra_properties: Self::music_bot_extra_properties(&music_bot),
            },
        );
        self.store.music_bots.insert(bot_id, music_bot);

        let mut row = BTreeMap::new();
        row.insert(String::from("botid"), bot_id.to_string());
        row.insert(String::from("bot_id"), client_database_id.to_string());
        row.insert(String::from("clid"), client_id.to_string());
        row.insert(String::from("cid"), target_channel_id.to_string());
        row.insert(String::from("bot_name"), nickname);
        row.insert(
            String::from("client_database_id"),
            client_database_id.to_string(),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_musicbotdelete(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let linked_client_id = self
            .store
            .music_bots
            .get(&bot_id)
            .and_then(|bot| bot.linked_client_id);

        if let Some(mut removed_bot) = self.store.music_bots.remove(&bot_id) {
            Self::stop_music_bot_playback(&mut removed_bot);
        }
        if let Some(client_id) = linked_client_id {
            self.store.online_clients.remove(&client_id);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_musicbotqueueadd(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(url) = request
            .named_args
            .get("url")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return QueryResponse::error(512, "url is required");
        };
        let (invoker_database_id, actor_permissions) =
            match self.query_actor_effective_permissions(session) {
                Ok((actor, actor_permissions)) => (actor.client_database_id, actor_permissions),
                Err(response) => return response,
            };
        let requested_loader = request
            .named_args
            .get("type")
            .or_else(|| request.named_args.get("invoker"))
            .map(String::as_str);

        let (previous_song_id, song_id, target_permissions) = {
            let Some(bot) = self.store.music_bots.get(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            (
                request
                    .named_args
                    .get("previous")
                    .or_else(|| request.named_args.get("song_previous_song_id"))
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or_else(|| bot.queue.last().map(|song| song.id).unwrap_or(0)),
                bot.next_song_id.max(1),
                bot.permissions.clone(),
            )
        };
        if let Err(response) =
            self.ensure_playlist_song_add_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }
        let entry = self.build_music_queue_entry(song_id, url, requested_loader, invoker_database_id);

        let play_now = request
            .named_args
            .get("play_now")
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let url_loader = entry.url_loader.clone();
        let (response_row, queued_behind_other, queue_pos, bot_clid) = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            bot.next_song_id = song_id.saturating_add(1);
            if let Err(response) = Self::insert_music_queue_entry(bot, entry, previous_song_id) {
                return response;
            }
            if play_now {
                bot.current_song_id = Some(song_id);
                bot.state = MusicBotState::Playing;
                Self::reset_music_bot_playback_progress(bot);
            }
            let queued_behind = bot.current_song_id != Some(song_id);
            let q_pos = bot.queue.len();
            let clid = bot.linked_client_id;
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
            row.insert(String::from("bot_id"), bot.client_database_id.to_string());
            row.insert(String::from("botid"), bot.id.to_string());
            (row, queued_behind, q_pos, clid)
        };

        let _ = url_loader;
        self.sync_music_bot_client_state(bot_id);
        if queued_behind_other
            && let Some(clid) = bot_clid
        {
            crate::runtime::musicbot::audio_engine::send_bot_channel_message_locked(
                self,
                server_id,
                clid,
                &format!(
                    "➕ Zur Warteschlange hinzugefügt (Position #{}): {}",
                    queue_pos, url
                ),
            );
        }
        QueryResponse::ok_row(response_row)
    }

    pub(crate) fn handle_musicbotqueuelist(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "music bot not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }
        if bot.queue.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        QueryResponse::ok_rows(
            bot.queue
                .iter()
                .map(|entry| {
                    let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
                    row.insert(String::from("bot_id"), bot.client_database_id.to_string());
                    row.insert(String::from("botid"), bot.id.to_string());
                    row
                })
                .collect(),
        )
    }

    pub(crate) fn handle_musicbotqueueremove(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "music bot not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_remove_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            let removed = match Self::remove_music_queue_entry(bot, song_id) {
                Ok(removed) => removed,
                Err(response) => return response,
            };
            let mut row = Self::music_queue_entry_row(bot.playlist_id, &removed);
            row.insert(String::from("bot_id"), bot.client_database_id.to_string());
            row.insert(String::from("botid"), bot.id.to_string());
            row
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(crate) fn handle_musicbotqueuereorder(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };
        let Some(previous_song_id) = request
            .named_args
            .get("song_previous_song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_previous_song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "music bot not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_move_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            if let Err(response) = Self::reorder_music_queue_entry(bot, song_id, previous_song_id)
            {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
            row.insert(String::from("bot_id"), bot.client_database_id.to_string());
            row.insert(String::from("botid"), bot.id.to_string());
            row
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(crate) fn handle_musicbotplayeraction(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let raw_action = request
            .named_args
            .get("action")
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_else(|| String::from("0"));
        let action = match raw_action.as_str() {
            "0" | "stop" => 0,
            "1" | "play" => 1,
            "2" | "pause" => 2,
            "3" | "next" | "forward" => 3,
            "4" | "prev" | "previous" | "rewind" => 4,
            "5" | "volume" => 5,
            "6" | "select" | "jump" => 6,
            "7" | "remove" | "delete" => 7,
            "8" | "clear" => 8,
            "9" | "seek" => 9,
            "10" | "repeat" | "replay_mode" => 10,
            "11" | "shuffle" => 11,
            "12" | "move_up" | "moveup" => 12,
            "13" | "move_down" | "movedown" => 13,
            "14" | "summon" | "move_to_channel" => 14,
            "15" | "rename" => 15,
            other => other.parse::<u32>().unwrap_or(999),
        };

        if action == 14 {
            let target_cid = request
                .named_args
                .get("value")
                .or_else(|| request.named_args.get("cid"))
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or_else(|| session.current_channel_id.unwrap_or(1));
            if let Some(bot_clid) = self.store.music_bots.get(&bot_id).and_then(|b| b.linked_client_id) {
                let old_cid = self
                    .store
                    .online_clients
                    .get(&bot_clid)
                    .map(|c| c.channel_id)
                    .unwrap_or(1);
                if old_cid != target_cid {
                    if let Some(client) = self.store.online_clients.get_mut(&bot_clid) {
                        client.channel_id = target_cid;
                    }
                    if let Some(snap) = self.online_client_snapshot(server_id, bot_clid) {
                        self.broadcast_event(
                            server_id,
                            &crate::transport::TransportNotification::ClientMoved {
                                presence: crate::transport::SessionPresence {
                                    client_id: snap.id,
                                    login_name: snap.nickname,
                                    unique_identifier: snap.unique_identifier,
                                    client_type: snap.client_type_exact,
                                    server_id,
                                    channel_id: target_cid,
                                },
                                from_channel_id: old_cid,
                                reason_id: 1,
                                reason_message: String::new(),
                                invoker_id: session.client_id,
                                invoker_name: session.effective_nickname(),
                                invoker_uid: String::new(),
                            },
                        );
                    }
                }
            }
            self.sync_music_bot_client_state(bot_id);
            let mut row = BTreeMap::new();
            row.insert(String::from("botid"), bot_id.to_string());
            row.insert(String::from("cid"), target_cid.to_string());
            return QueryResponse::ok_row(row);
        }

        if action == 15 {
            if let Some(new_name) = request
                .named_args
                .get("value")
                .or_else(|| request.named_args.get("name"))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                if let Some(bot_clid) = self.store.music_bots.get(&bot_id).and_then(|b| b.linked_client_id)
                    && let Some(client) = self.store.online_clients.get_mut(&bot_clid)
                {
                    client.nickname = new_name;
                }
                self.sync_music_bot_client_state(bot_id);
            }
            let mut row = BTreeMap::new();
            row.insert(String::from("botid"), bot_id.to_string());
            return QueryResponse::ok_row(row);
        }

        let (response_bot_id, response_bot_dbid, response_state, response_queue_position) = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };

            let previous_song_id = bot.current_song_id;

            match action {
                0 => {
                    bot.state = MusicBotState::Stopped;
                    Self::stop_music_bot_playback(bot);
                    crate::runtime::musicbot::audio_engine::request_bot_seek(bot.id, 0);
                }
                1 => {
                    if bot.current_song_id.is_none() {
                        bot.current_song_id = bot.queue.first().map(|song| song.id);
                    }
                    if bot.current_song_id.is_some() {
                        bot.state = MusicBotState::Playing;
                        if previous_song_id != bot.current_song_id {
                            Self::reset_music_bot_playback_progress(bot);
                        } else {
                            Self::resume_music_bot_playback(bot);
                        }
                    }
                }
                2 => {
                    bot.state = if bot.current_song_id.is_some() {
                        MusicBotState::Paused
                    } else {
                        MusicBotState::Stopped
                    };
                    if matches!(bot.state, MusicBotState::Paused) {
                        Self::pause_music_bot_playback(bot);
                    } else {
                        Self::stop_music_bot_playback(bot);
                    }
                }
                3 => {
                    let next_song_id = bot
                        .current_song_id
                        .and_then(|current_song_id| {
                            bot.queue
                                .iter()
                                .position(|song| song.id == current_song_id)
                                .and_then(|index| {
                                    bot.queue.get(index + 1).or_else(|| bot.queue.first())
                                })
                                .map(|song| song.id)
                        })
                        .or_else(|| bot.queue.first().map(|song| song.id));
                    bot.current_song_id = next_song_id;
                    bot.state = if bot.current_song_id.is_some() {
                        MusicBotState::Playing
                    } else {
                        MusicBotState::Stopped
                    };
                    if bot.current_song_id.is_some() {
                        Self::reset_music_bot_playback_progress(bot);
                        crate::runtime::musicbot::audio_engine::request_bot_seek(bot.id, 0);
                    } else {
                        Self::stop_music_bot_playback(bot);
                    }
                }
                4 => {
                    bot.current_song_id = bot
                        .current_song_id
                        .and_then(|current_song_id| {
                            bot.queue
                                .iter()
                                .position(|song| song.id == current_song_id)
                                .and_then(|index| {
                                    index
                                        .checked_sub(1)
                                        .and_then(|prev| bot.queue.get(prev))
                                        .or_else(|| bot.queue.last())
                                })
                                .map(|song| song.id)
                        })
                        .or_else(|| bot.queue.first().map(|song| song.id));
                    bot.state = if bot.current_song_id.is_some() {
                        MusicBotState::Playing
                    } else {
                        MusicBotState::Stopped
                    };
                    if bot.current_song_id.is_some() {
                        Self::reset_music_bot_playback_progress(bot);
                        crate::runtime::musicbot::audio_engine::request_bot_seek(bot.id, 0);
                    } else {
                        Self::stop_music_bot_playback(bot);
                    }
                }
                5 => {
                    if let Some(units_ms) = request
                        .named_args
                        .get("units")
                        .and_then(|v| v.trim().parse::<i64>().ok())
                    {
                        let current_ms = i64::from(Self::music_bot_current_song_replay_millis(bot));
                        let target_ms = current_ms.saturating_add(units_ms).max(0) as u32;
                        let max_ms = Self::music_bot_current_song_max_millis(bot);
                        let clamped_ms = if max_ms > 1000 {
                            target_ms.min(max_ms.saturating_sub(500))
                        } else {
                            target_ms
                        };
                        if bot.current_song_id.is_some() {
                            bot.current_song_progress_millis = clamped_ms;
                            if matches!(bot.state, MusicBotState::Playing) {
                                bot.current_song_started_at_millis =
                                    Some(current_unix_timestamp_millis());
                            }
                            crate::runtime::musicbot::audio_engine::request_bot_seek(
                                bot.id,
                                clamped_ms / 1000,
                            );
                        }
                    } else if let Some(vol) = request
                        .named_args
                        .get("value")
                        .or_else(|| request.named_args.get("volume"))
                        .or_else(|| request.named_args.get("player_volume"))
                    {
                        let normalized = Self::format_music_bot_volume_factor(
                            Self::parse_music_bot_volume_factor(vol),
                        );
                        if bot.player_volume != normalized {
                            bot.player_volume = normalized;
                            if matches!(bot.state, MusicBotState::Playing) {
                                let replay_ms = Self::music_bot_current_song_replay_millis(bot);
                                bot.current_song_progress_millis = replay_ms;
                                bot.current_song_started_at_millis =
                                    Some(current_unix_timestamp_millis());
                                crate::runtime::musicbot::audio_engine::request_bot_seek(
                                    bot.id,
                                    replay_ms / 1000,
                                );
                            }
                        }
                    }
                }
                6 => {
                    if let Some(units_ms) = request
                        .named_args
                        .get("units")
                        .and_then(|v| v.trim().parse::<i64>().ok())
                    {
                        let current_ms = i64::from(Self::music_bot_current_song_replay_millis(bot));
                        let target_ms = current_ms.saturating_sub(units_ms).max(0) as u32;
                        let max_ms = Self::music_bot_current_song_max_millis(bot);
                        let clamped_ms = if max_ms > 1000 {
                            target_ms.min(max_ms.saturating_sub(500))
                        } else {
                            target_ms
                        };
                        if bot.current_song_id.is_some() {
                            bot.current_song_progress_millis = clamped_ms;
                            if matches!(bot.state, MusicBotState::Playing) {
                                bot.current_song_started_at_millis =
                                    Some(current_unix_timestamp_millis());
                            }
                            crate::runtime::musicbot::audio_engine::request_bot_seek(
                                bot.id,
                                clamped_ms / 1000,
                            );
                        }
                    } else if let Some(target_sid) = request
                        .named_args
                        .get("value")
                        .or_else(|| request.named_args.get("song_id"))
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        && bot.queue.iter().any(|s| s.id == target_sid)
                    {
                        bot.current_song_id = Some(target_sid);
                        bot.state = MusicBotState::Playing;
                        Self::reset_music_bot_playback_progress(bot);
                        crate::runtime::musicbot::audio_engine::request_bot_seek(bot.id, 0);
                    }
                }
                7 => {
                    if let Some(target_sid) = request
                        .named_args
                        .get("value")
                        .or_else(|| request.named_args.get("song_id"))
                        .and_then(|v| v.trim().parse::<u32>().ok())
                    {
                        let _ = Self::remove_music_queue_entry(bot, target_sid);
                    }
                }
                8 => {
                    bot.queue.clear();
                    bot.current_song_id = None;
                    bot.state = MusicBotState::Stopped;
                    Self::stop_music_bot_playback(bot);
                }
                9 => {
                    if let Some(target_ms) = request
                        .named_args
                        .get("value")
                        .or_else(|| request.named_args.get("position_ms"))
                        .and_then(|v| v.trim().parse::<u32>().ok())
                    {
                        let max_ms = Self::music_bot_current_song_max_millis(bot);
                        let clamped_ms = if max_ms > 1000 {
                            target_ms.min(max_ms.saturating_sub(500))
                        } else {
                            target_ms
                        };
                        if bot.current_song_id.is_none() {
                            bot.current_song_id = bot.queue.first().map(|s| s.id);
                        }
                        if bot.current_song_id.is_some() {
                            bot.current_song_progress_millis = clamped_ms;
                            bot.current_song_started_at_millis =
                                Some(current_unix_timestamp_millis());
                            bot.state = MusicBotState::Playing;
                            crate::runtime::musicbot::audio_engine::request_bot_seek(
                                bot.id,
                                clamped_ms / 1000,
                            );
                        }
                    }
                }
                10 => {
                    let next_mode = request
                        .named_args
                        .get("value")
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        .unwrap_or_else(|| (bot.playlist_replay_mode + 1) % 3);
                    bot.playlist_replay_mode = next_mode % 3;
                }
                11 => {
                    if bot.queue.len() > 1 {
                        let cur_id = bot.current_song_id;
                        let mut current_entry = None;
                        if let Some(cid) = cur_id
                            && let Some(pos) = bot.queue.iter().position(|s| s.id == cid)
                        {
                            current_entry = Some(bot.queue.remove(pos));
                        }
                        // Deterministic pseudo-shuffle using current timestamp millis
                        let mut seed = current_unix_timestamp_millis().max(1);
                        let len = bot.queue.len();
                        for i in (1..len).rev() {
                            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                            let j = (seed as usize) % (i + 1);
                            bot.queue.swap(i, j);
                        }
                        if let Some(cur) = current_entry {
                            bot.queue.insert(0, cur);
                        }
                    }
                }
                12 => {
                    if let Some(target_sid) = request
                        .named_args
                        .get("value")
                        .or_else(|| request.named_args.get("song_id"))
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        && let Some(idx) = bot.queue.iter().position(|s| s.id == target_sid)
                        && idx > 0
                    {
                        bot.queue.swap(idx, idx - 1);
                    }
                }
                13 => {
                    if let Some(target_sid) = request
                        .named_args
                        .get("value")
                        .or_else(|| request.named_args.get("song_id"))
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        && let Some(idx) = bot.queue.iter().position(|s| s.id == target_sid)
                        && idx + 1 < bot.queue.len()
                    {
                        bot.queue.swap(idx, idx + 1);
                    }
                }
                _ => return QueryResponse::error(512, "unsupported music bot action"),
            }

            Self::normalize_music_bot_queue(bot);

            (
                bot.id,
                bot.client_database_id,
                bot.state.as_str().to_string(),
                Self::music_bot_queue_position(bot),
            )
        };

        self.sync_music_bot_client_state(bot_id);

        let mut row = BTreeMap::new();
        row.insert(String::from("botid"), response_bot_id.to_string());
        row.insert(String::from("bot_id"), response_bot_dbid.to_string());
        row.insert(String::from("state"), response_state);
        row.insert(
            String::from("queue_position"),
            response_queue_position.to_string(),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_musicbotplayerinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "music bot not found");
        };

        QueryResponse::ok_row(Self::music_bot_playerinfo_row(bot))
    }

    pub(crate) fn handle_musicbotsetsubscription(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let requested_id = request
            .named_args
            .get("botid")
            .or_else(|| request.named_args.get("bot_id"))
            .and_then(|value| value.parse::<u64>().ok());
        if requested_id == Some(0) {
            return QueryResponse::ok();
        }
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "music bot not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_musicbotlist(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let local_files_json = serde_json::Value::Array(
            self.discover_local_music_files()
                .into_iter()
                .map(|(name, url, source, size)| {
                    serde_json::json!({
                        "name": name,
                        "url": url,
                        "source": source,
                        "size": size,
                    })
                })
                .collect(),
        )
        .to_string();

        let rows = self
            .store
            .music_bots
            .values()
            .filter(|bot| bot.server_id == server_id && bot.linked_client_id.is_some())
            .map(|bot| {
                let mut row = Self::music_bot_playerinfo_row(bot);
                let name = self.music_bot_client_name(bot);
                let channel_id = bot
                    .linked_client_id
                    .and_then(|clid| self.store.online_clients.get(&clid))
                    .map(|client| client.channel_id)
                    .unwrap_or(1);
                let queue_json = serde_json::Value::Array(
                    bot.queue
                        .iter()
                        .map(|entry| {
                            serde_json::json!({
                                "song_id": entry.id,
                                "title": entry.title,
                                "url": entry.url,
                                "loader": entry.url_loader,
                                "duration_secs": entry.length_seconds,
                                "is_current": bot.current_song_id == Some(entry.id),
                                "live_stream": entry.live_stream,
                                "thumbnail": entry.thumbnail,
                            })
                        })
                        .collect(),
                )
                .to_string();
                row.insert(String::from("botid"), bot.id.to_string());
                row.insert(String::from("name"), name.clone());
                row.insert(String::from("bot_name"), name);
                row.insert(String::from("cid"), channel_id.to_string());
                row.insert(
                    String::from("replay_mode"),
                    bot.playlist_replay_mode.to_string(),
                );
                row.insert(String::from("queue_json"), queue_json);
                row.insert(String::from("local_files_json"), local_files_json.clone());
                if let Some(clid) = bot.linked_client_id {
                    row.insert(String::from("clid"), clid.to_string());
                    row.insert(String::from("client_id"), clid.to_string());
                }
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

}
