use super::*;
impl BaselineRuntime {
    pub(crate) fn handle_clientaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["cldbid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(target_permissions) = self.effective_permissions_for_client(
            actor.server_id,
            actor.channel_id,
            client_database_id,
        ) else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(account) = self.query_account_by_cldbid_mut(client_database_id) {
            for parsed_assignment in &parsed_assignments {
                account.permissions.insert(
                    parsed_assignment.name.clone(),
                    parsed_assignment.assignment.clone(),
                );
            }
        } else {
            self.ensure_client_permission_target_mut(actor.server_id, client_database_id);
            let target = self.store.client_permissions.iter_mut().find(|t| t.client_database_id == client_database_id && t.server_id == actor.server_id).unwrap();
            for parsed_assignment in &parsed_assignments {
                target.permissions.insert(
                    parsed_assignment.name.clone(),
                    parsed_assignment.assignment.clone(),
                );
            }
            let _ = self.db.save_client_permission_target(target);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["cldbid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(target_permissions) = self.effective_permissions_for_client(
            actor.server_id,
            actor.channel_id,
            client_database_id,
        ) else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(account) = self.query_account_by_cldbid_mut(client_database_id) {
            for permission_name in &permission_names {
                account.permissions.remove(permission_name);
            }
        } else if let Some(target_index) = self
            .store
            .client_permissions
            .iter()
            .position(|target| target.client_database_id == client_database_id)
        {
            let remove_target = {
                let target = &mut self.store.client_permissions[target_index];
                for permission_name in &permission_names {
                    target.permissions.remove(permission_name);
                }
                let _ = self.db.save_client_permission_target(target);
                target.permissions.is_empty()
            };
            if remove_target {
                self.store
                    .client_permissions
                    .retain(|candidate| candidate.client_database_id != client_database_id);
            }
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(1024, "virtualserver is not selected");
        };

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let permissions = if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            &account.permissions
        } else if let Some(target) = self.client_permission_target(server_id, client_database_id) {
            &target.permissions
        } else if self.client_database_id_exists(client_database_id) {
            return QueryResponse::ok_rows(Vec::new());
        } else {
            return QueryResponse::error(768, "client not found");
        };

        let rows = permissions
            .iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("cldbid"), client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_channelclientaddperm(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let parsed_assignments =
            match self.parse_permission_assignments(request, &["cid", "cldbid"]) {
                Ok(parsed_assignments) => parsed_assignments,
                Err(response) => return response,
            };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) =
            self.effective_permissions_for_client(actor.server_id, channel_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let target_index = self
            .store
            .channel_client_permissions
            .iter()
            .position(|target| {
                target.channel_id == channel_id && target.client_database_id == client_database_id
            });
        let target = if let Some(target_index) = target_index {
            &mut self.store.channel_client_permissions[target_index]
        } else {
            self.store
                .channel_client_permissions
                .push(ChannelClientPermissionTarget {
                    channel_id,
                    client_database_id,
                    permissions: BTreeMap::new(),
                });
            self.store
                .channel_client_permissions
                .last_mut()
                .expect("newly inserted channel-client permission target should exist")
        };

        for parsed_assignment in parsed_assignments {
            target
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_channel_client_permission_target(target);

        QueryResponse::ok()
    }

    pub(crate) fn handle_playlistpermlist(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
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
        if let Err(response) = self.ensure_playlist_permission_list_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let mut permissions = target_permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("playlist_id"), playlist_id.to_string());
            rows.push(row);
        }

        let _ = actor;
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_playlistclientlist(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_list_allowed(
            &actor_permissions,
            &bot.permissions,
        ) {
            return response;
        }

        let mut rows = bot
            .client_permissions
            .iter()
            .map(|target| {
                let mut row = BTreeMap::new();
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row.insert(String::from("cldbid"), target.client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("playlist_id"), playlist_id.to_string());
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_playlistclientpermlist(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_list_allowed(
            &actor_permissions,
            &bot.permissions,
        ) {
            return response;
        }

        let mut rows = if let Some(target) = bot
            .client_permissions
            .iter()
            .find(|target| target.client_database_id == client_database_id)
        {
            let mut permissions = target.permissions.iter().collect::<Vec<_>>();
            permissions.sort_by(|(left_name, _), (right_name, _)| {
                self.permission_id_for_name(left_name)
                    .cmp(&self.permission_id_for_name(right_name))
                    .then_with(|| left_name.cmp(right_name))
            });

            permissions
                .into_iter()
                .map(|(permission_name, assignment)| {
                    let mut row = self.render_permission_row(
                        permission_name,
                        assignment,
                        request.flags.contains("permsid"),
                    );
                    row.insert(String::from("playlist_id"), playlist_id.to_string());
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    row
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("playlist_id"), playlist_id.to_string());
            row.insert(String::from("cldbid"), client_database_id.to_string());
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_playlistaddperm(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["playlist_id"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
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
        if let Err(response) = self.ensure_playlist_permission_modify_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let include_permission_name = request.flags.contains("permsid");
        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        for parsed_assignment in &parsed_assignments {
            bot.permissions.insert(
                parsed_assignment.name.clone(),
                parsed_assignment.assignment.clone(),
            );
        }
        BaselineRuntime::normalize_music_bot_queue(bot);

        let rows = parsed_assignments
            .iter()
            .map(|parsed_assignment| {
                let mut row = self.render_permission_row(
                &parsed_assignment.name,
                &parsed_assignment.assignment,
                    include_permission_name,
                );
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_playlistclientaddperm(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let parsed_assignments = match self.parse_permission_assignments(
            request,
            &["playlist_id", "cldbid"],
        ) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
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
        if let Err(response) = self.ensure_playlist_permission_modify_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let include_permission_name = request.flags.contains("permsid");
        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let target_index = bot
            .client_permissions
            .iter()
            .position(|target| target.client_database_id == client_database_id);
        let target = if let Some(target_index) = target_index {
            &mut bot.client_permissions[target_index]
        } else {
            bot.client_permissions.push(PlaylistClientPermissionTarget {
                client_database_id,
                permissions: BTreeMap::new(),
            });
            bot.client_permissions
                .last_mut()
                .expect("newly inserted playlist-client permission target should exist")
        };

        for parsed_assignment in &parsed_assignments {
            target.permissions.insert(
                parsed_assignment.name.clone(),
                parsed_assignment.assignment.clone(),
            );
        }
        BaselineRuntime::normalize_music_bot_queue(bot);

        let rows = parsed_assignments
            .iter()
            .map(|parsed_assignment| {
                let mut row = self.render_permission_row(
                &parsed_assignment.name,
                &parsed_assignment.assignment,
                    include_permission_name,
                );
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row.insert(String::from("cldbid"), client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_playlistdelperm(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let permission_names =
            match self.parse_requested_permission_names(request, &["playlist_id"]) {
                Ok(permission_names) => permission_names,
                Err(response) => return response,
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
        if let Err(response) = self.ensure_playlist_permission_modify_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        for permission_name in &permission_names {
            bot.permissions.remove(permission_name);
        }
        BaselineRuntime::normalize_music_bot_queue(bot);

        QueryResponse::ok()
    }

    pub(crate) fn handle_playlistdelclientperm(
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
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let permission_names =
            match self.parse_requested_permission_names(request, &["playlist_id", "cldbid"]) {
                Ok(permission_names) => permission_names,
                Err(response) => return response,
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
        if let Err(response) = self.ensure_playlist_permission_modify_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Some(target) = bot
            .client_permissions
            .iter_mut()
            .find(|target| target.client_database_id == client_database_id)
        {
            for permission_name in &permission_names {
                target.permissions.remove(permission_name);
            }
        }
        BaselineRuntime::normalize_music_bot_queue(bot);

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelclientdelperm(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let permission_names =
            match self.parse_requested_permission_names(request, &["cid", "cldbid"]) {
                Ok(permission_names) => permission_names,
                Err(response) => return response,
            };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) =
            self.effective_permissions_for_client(actor.server_id, channel_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(target_index) =
            self.store
                .channel_client_permissions
                .iter()
                .position(|target| {
                    target.channel_id == channel_id
                        && target.client_database_id == client_database_id
                })
        {
            let remove_target = {
                let target = &mut self.store.channel_client_permissions[target_index];
                for permission_name in permission_names {
                    target.permissions.remove(&permission_name);
                }
                let _ = self.db.save_channel_client_permission_target(target);
                target.permissions.is_empty()
            };
            if remove_target {
                self.store.channel_client_permissions.remove(target_index);
            }
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelclientpermlist(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }

        let Some(target) = self.store.channel_client_permissions.iter().find(|target| {
            target.channel_id == channel_id && target.client_database_id == client_database_id
        }) else {
            return QueryResponse::ok_rows(Vec::new());
        };

        let mut permissions = target.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("cid"), channel_id.to_string());
                row.insert(String::from("cldbid"), client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_channeladdperm(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["cid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| channel.permissions.clone())
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_channel_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(channel) = self
            .store
            .channels
            .get_mut(&server_id)
            .and_then(|channels| channels.iter_mut().find(|channel| channel.id == channel_id))
        else {
            return QueryResponse::error(768, "channel not found");
        };

        for parsed_assignment in parsed_assignments {
            channel
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_channel(actor.server_id, channel);

        QueryResponse::ok()
    }

    pub(crate) fn handle_channeldelperm(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["cid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| channel.permissions.clone())
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_channel_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(channel) = self
            .store
            .channels
            .get_mut(&server_id)
            .and_then(|channels| channels.iter_mut().find(|channel| channel.id == channel_id))
        else {
            return QueryResponse::error(768, "channel not found");
        };

        for permission_name in permission_names {
            channel.permissions.remove(&permission_name);
        }
        let _ = self.db.save_channel(actor.server_id, channel);

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelpermlist(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(channel) = self.channel_by_id(server_id, channel_id) else {
            return QueryResponse::error(768, "channel not found");
        };

        let mut permissions = channel.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("cid"), channel_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

}
