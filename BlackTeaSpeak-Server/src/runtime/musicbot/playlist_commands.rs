use std::collections::BTreeMap;
use crate::runtime::*;
use super::*;

impl BaselineRuntime {
    pub(crate) fn handle_playlistlist(
        &self,
        _request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let rows = self
            .store
            .music_bots
            .values()
            .filter(|bot| bot.server_id == server_id)
            .filter(|bot| {
                self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
                    .is_ok()
            })
            .map(|bot| self.music_bot_playlist_list_row(bot))
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_playlistinfo(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }

        QueryResponse::ok_row(self.music_bot_playlist_info_row(bot))
    }

    pub(crate) fn handle_playlistedit(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
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
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_modify_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };

        if let Some(title) = request.named_args.get("playlist_title") {
            bot.playlist_title = title.clone();
        }
        if let Some(description) = request.named_args.get("playlist_description") {
            bot.playlist_description = description.clone();
        }
        if let Some(delete_played) = request
            .named_args
            .get("playlist_flag_delete_played")
            .and_then(|value| parse_query_bool(value))
        {
            bot.playlist_flag_delete_played = delete_played;
        }
        if let Some(finished) = request
            .named_args
            .get("playlist_flag_finished")
            .and_then(|value| parse_query_bool(value))
        {
            bot.playlist_flag_finished = finished;
        }
        if let Some(replay_mode) = request
            .named_args
            .get("playlist_replay_mode")
            .and_then(|value| value.parse::<u32>().ok())
        {
            bot.playlist_replay_mode = replay_mode;
        }
        if let Some(max_songs) = request
            .named_args
            .get("playlist_max_songs")
            .and_then(|value| value.parse::<u32>().ok())
        {
            bot.playlist_max_songs = max_songs;
        }

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok()
    }

    pub(crate) fn handle_playlistsetsubscription(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_playlistsonglist(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }
        if bot.queue.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        let extract_metadata = request.flags.contains("extract-metadata");
        QueryResponse::ok_rows(
            bot.queue
                .iter()
                .map(|entry| {
                    let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
                    if extract_metadata && entry.loaded {
                        row.insert(String::from("song_metadata_title"), entry.title.clone());
                        row.insert(
                            String::from("song_metadata_description"),
                            entry.description.clone(),
                        );
                        row.insert(
                            String::from("song_metadata_thumbnail_url"),
                            if entry.thumbnail.is_empty() {
                                String::from("none")
                            } else {
                                entry.thumbnail.clone()
                            },
                        );
                        row.insert(
                            String::from("song_metadata_length"),
                            entry.length_seconds.to_string(),
                        );
                    }
                    row
                })
                .collect(),
        )
    }

    pub(crate) fn handle_playlistsongadd(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
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
                return QueryResponse::error(768, "playlist not found");
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

        let url_loader = entry.url_loader.clone();
        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            bot.next_song_id = song_id.saturating_add(1);
            if let Err(response) = Self::insert_music_queue_entry(bot, entry, previous_song_id) {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            Self::music_queue_entry_row(bot.playlist_id, entry)
        };

        let _ = url_loader;
        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(crate) fn handle_playlistsongremove(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
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
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_remove_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            let removed = match Self::remove_music_queue_entry(bot, song_id) {
                Ok(removed) => removed,
                Err(response) => return response,
            };
            Self::music_queue_entry_row(bot.playlist_id, &removed)
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(crate) fn handle_playlistsongreorder(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
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
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_move_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            if let Err(response) = Self::reorder_music_queue_entry(bot, song_id, previous_song_id)
            {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            Self::music_queue_entry_row(bot.playlist_id, entry)
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(crate) fn handle_playlistsongsetcurrent(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
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
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_move_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = Self::set_music_queue_current_song(bot, song_id) {
            return response;
        }

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok()
    }

    pub(crate) fn handle_playlistcreate(
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
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let bot_id = self
            .store
            .music_bots
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let playlist_id = self
            .store
            .music_bots
            .values()
            .map(|bot| bot.playlist_id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(bot_id);

        let title = request
            .named_args
            .get("playlist_title")
            .or_else(|| request.named_args.get("title"))
            .cloned()
            .unwrap_or_else(|| format!("Playlist {playlist_id}"));
        let description = request
            .named_args
            .get("playlist_description")
            .or_else(|| request.named_args.get("description"))
            .cloned()
            .unwrap_or_default();

        let playlist_holder = MusicBot {
            id: bot_id,
            server_id,
            client_database_id: actor.client_database_id,
            linked_client_id: None,
            nickname: String::new(),
            channel_id: 0,
            playlist_id,
            current_song_id: None,
            next_song_id: 1,
            state: MusicBotState::Stopped,
            player_volume: String::from("1"),
            playlist_title: title,
            playlist_description: description,
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

        self.store.music_bots.insert(bot_id, playlist_holder);

        let mut row = BTreeMap::new();
        row.insert(String::from("playlist_id"), playlist_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_playlistdelete(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
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
            return QueryResponse::error(768, "playlist not found");
        };

        let required_power = permission_value_or_default(
            &target_permissions,
            &["i_playlist_needed_delete_power"],
        );
        if required_power > 0 {
            let actor_power = permission_value_or_default(
                &actor_permissions,
                &["i_playlist_delete_power", "i_playlist_modify_power"],
            );
            if actor_power < required_power {
                return self.insufficient_permission_response("i_playlist_delete_power");
            }
        }

        let is_standalone = self
            .store
            .music_bots
            .get(&bot_id)
            .is_some_and(|bot| bot.linked_client_id.is_none());
        if is_standalone {
            self.store.music_bots.remove(&bot_id);
        } else if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
            bot.queue.clear();
            bot.current_song_id = None;
            bot.state = MusicBotState::Stopped;
            bot.playlist_title.clear();
            bot.playlist_description.clear();
            Self::normalize_music_bot_queue(bot);
            self.sync_music_bot_client_state(bot_id);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_musicbotplaylistassign(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(source_bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };

        if source_bot_id != bot_id {
            let Some(source) = self.store.music_bots.get(&source_bot_id).cloned() else {
                return QueryResponse::error(768, "playlist not found");
            };
            let Some(target) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            target.playlist_id = playlist_id;
            target.playlist_title = source.playlist_title;
            target.playlist_description = source.playlist_description;
            target.playlist_flag_delete_played = source.playlist_flag_delete_played;
            target.playlist_flag_finished = source.playlist_flag_finished;
            target.playlist_replay_mode = source.playlist_replay_mode;
            target.playlist_max_songs = source.playlist_max_songs;
            target.permissions = source.permissions;
            target.client_permissions = source.client_permissions;
            target.queue = source.queue;
            target.current_song_id = source.current_song_id;
            target.next_song_id = source.next_song_id;
            Self::normalize_music_bot_queue(target);
            if source.linked_client_id.is_none() {
                self.store.music_bots.remove(&source_bot_id);
            }
            self.sync_music_bot_client_state(bot_id);
        }

        QueryResponse::ok()
    }
}
