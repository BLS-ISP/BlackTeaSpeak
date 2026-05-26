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
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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

    pub(crate) fn handle_permfind(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let mut rows = Vec::new();

        for permission_name in permission_names {
            let permission_id = self.permission_id_for_name(&permission_name);
            for group in self.store.server_groups.values() {
                if group.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("0"));
                    row.insert(String::from("id1"), group.id.to_string());
                    row.insert(String::from("id2"), String::from("0"));
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        0_u32,
                        u64::from(group.id),
                        0_u64,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for channels in self.store.channels.values() {
                for channel in channels {
                    if channel.permissions.contains_key(&permission_name) {
                        let mut row = BTreeMap::new();
                        row.insert(String::from("t"), String::from("2"));
                        row.insert(String::from("id1"), channel.id.to_string());
                        row.insert(String::from("id2"), String::from("0"));
                        row.insert(String::from("p"), permission_id.to_string());
                        row.insert(String::from("permsid"), permission_name.clone());
                        rows.push((
                            2_u32,
                            u64::from(channel.id),
                            0_u64,
                            permission_id,
                            permission_name.clone(),
                            row,
                        ));
                    }
                }
            }

            for group in self.store.channel_groups.values() {
                if group.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("3"));
                    row.insert(String::from("id1"), String::from("0"));
                    row.insert(String::from("id2"), group.id.to_string());
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        3_u32,
                        0_u64,
                        u64::from(group.id),
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for target in &self.store.channel_client_permissions {
                if target.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("4"));
                    row.insert(String::from("id1"), target.channel_id.to_string());
                    row.insert(String::from("id2"), target.client_database_id.to_string());
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        4_u32,
                        u64::from(target.channel_id),
                        target.client_database_id,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for account in self.store.query_accounts.values() {
                if account.permissions.contains_key(&permission_name)
                    && let Some(client_database_id) = account.client_database_id
                {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("1"));
                    row.insert(String::from("id1"), client_database_id.to_string());
                    row.insert(String::from("id2"), String::from("0"));
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        1_u32,
                        client_database_id,
                        0_u64,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for target in &self.store.client_permissions {
                if target.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("1"));
                    row.insert(String::from("id1"), target.client_database_id.to_string());
                    row.insert(String::from("id2"), String::from("0"));
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        1_u32,
                        target.client_database_id,
                        0_u64,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }
        }

        rows.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
                .then_with(|| left.4.cmp(&right.4))
        });

        QueryResponse::ok_rows(rows.into_iter().map(|(_, _, _, _, _, row)| row).collect())
    }

    pub(crate) fn handle_permget(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let Some(account) = self.store.query_accounts.get(login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let effective_permissions = self.effective_permissions_for_account(account);
        let mut rows = Vec::new();

        for permission_name in permission_names {
            let Some(assignment) = effective_permissions.get(&permission_name) else {
                return QueryResponse::error(
                    768,
                    format!("permission {} not assigned", permission_name),
                );
            };
            let mut row = self.render_permission_row(&permission_name, assignment, true);
            row.remove("permnegated");
            row.remove("permskip");
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_permidgetbyname(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let mut rows = Vec::new();

        for permission_name in permission_names {
            if !self.knows_permission_name(&permission_name) {
                return QueryResponse::error(
                    768,
                    format!("permission {} not found", permission_name),
                );
            }

            let mut row = BTreeMap::new();
            row.insert(String::from("permsid"), permission_name.clone());
            row.insert(
                String::from("permid"),
                self.permission_id_for_name(&permission_name).to_string(),
            );
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_permissionlist(&self, session: &QuerySessionState) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        QueryResponse::ok_rows(self.build_permission_rows())
    }

    pub(crate) fn handle_permoverview(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
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
        let (server_group_ids, client_permissions) = if let Some(account) =
            self.query_account_by_cldbid(client_database_id)
        {
            (account.server_groups.clone(), account.permissions.clone())
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            (
                client.server_groups.clone(),
                self.client_permission_target(server_id, client_database_id)
                    .map(|target| target.permissions.clone())
                    .unwrap_or_default(),
            )
        } else if let Some(target) = self.client_permission_target(server_id, client_database_id) {
            (Vec::new(), target.permissions.clone())
        } else {
            return QueryResponse::error(768, "client not found");
        };
        let permission_filter = match self.permoverview_requested_permissions(request) {
            Ok(permission_filter) => permission_filter,
            Err(response) => return response,
        };

        let mut rows = Vec::new();
        if let Some(channel) = self.channel_by_id(server_id, channel_id) {
            for (permission_name, assignment) in &channel.permissions {
                if !self.permission_filter_matches(&permission_filter, permission_name) {
                    continue;
                }
                rows.push(self.render_permoverview_row(
                    2,
                    u64::from(channel.id),
                    0,
                    permission_name,
                    assignment,
                ));
            }
        }

        for group_id in &server_group_ids {
            if let Some(group) = self.store.server_groups.get(group_id) {
                for (permission_name, assignment) in &group.permissions {
                    if !self.permission_filter_matches(&permission_filter, permission_name) {
                        continue;
                    }
                    rows.push(self.render_permoverview_row(
                        0,
                        u64::from(group.id),
                        0,
                        permission_name,
                        assignment,
                    ));
                }
            }
        }

        for (permission_name, assignment) in &client_permissions {
            if !self.permission_filter_matches(&permission_filter, permission_name) {
                continue;
            }
            rows.push(self.render_permoverview_row(
                1,
                client_database_id,
                0,
                permission_name,
                assignment,
            ));
        }

        if let Some(group) = self.effective_channel_group(channel_id, client_database_id) {
            for (permission_name, assignment) in &group.permissions {
                if !self.permission_filter_matches(&permission_filter, permission_name) {
                    continue;
                }
                rows.push(self.render_permoverview_row(
                    3,
                    0,
                    u64::from(group.id),
                    permission_name,
                    assignment,
                ));
            }
        }

        if let Some(target) = self.store.channel_client_permissions.iter().find(|target| {
            target.channel_id == channel_id && target.client_database_id == client_database_id
        }) {
            for (permission_name, assignment) in &target.permissions {
                if !self.permission_filter_matches(&permission_filter, permission_name) {
                    continue;
                }
                rows.push(self.render_permoverview_row(
                    4,
                    u64::from(channel_id),
                    client_database_id,
                    permission_name,
                    assignment,
                ));
            }
        }

        rows.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
        });

        QueryResponse::ok_rows(rows.into_iter().map(|(_, _, _, _, row)| row).collect())
    }

    pub(crate) fn handle_channelpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
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
