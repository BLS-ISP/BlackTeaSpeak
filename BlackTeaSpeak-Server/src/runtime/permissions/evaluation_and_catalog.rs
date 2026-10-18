use super::*;

impl BaselineRuntime {
    pub(crate) fn query_permission_actor_context(
        &self,
        session: &QuerySessionState,
    ) -> std::result::Result<PermissionActorContext, QueryResponse> {
        if let Some(client_database_id) = session.actor_client_database_id_override {
            let server_id = session
                .selected_virtual_server_id
                .or_else(|| {
                    self.store
                        .online_clients
                        .values()
                        .find(|client| client.database_id == client_database_id)
                        .map(|client| client.server_id)
                })
                .or_else(|| self.store.virtual_servers.keys().next().copied())
                .unwrap_or(1);
            let channel_id = session
                .current_channel_id
                .filter(|channel_id| self.channel_exists(server_id, *channel_id))
                .or_else(|| {
                    self.online_client_by_cldbid(server_id, client_database_id)
                        .map(|client| client.channel_id)
                })
                .or_else(|| self.default_channel_id_for_server(server_id))
                .unwrap_or(1);

            return Ok(PermissionActorContext {
                server_id,
                channel_id,
                client_database_id,
            });
        }

        let Some(login_name) = session.authenticated_login.as_ref() else {
            return Err(QueryResponse::error(521, "login required"));
        };
        let Some(account) = self.store.query_accounts.get(login_name) else {
            return Err(QueryResponse::error(768, "query account not found"));
        };
        let Some(client_database_id) = account.client_database_id else {
            return Err(QueryResponse::error(768, "query account not found"));
        };

        let server_id = session
            .selected_virtual_server_id
            .or(account.server_id)
            .or_else(|| self.store.virtual_servers.keys().next().copied())
            .unwrap_or(1);
        let channel_id = session
            .current_channel_id
            .filter(|channel_id| self.channel_exists(server_id, *channel_id))
            .or_else(|| self.default_channel_id_for_server(server_id))
            .unwrap_or(1);

        Ok(PermissionActorContext {
            server_id,
            channel_id,
            client_database_id,
        })
    }

    pub(crate) fn query_actor_effective_permissions(
        &self,
        session: &QuerySessionState,
    ) -> std::result::Result<
        (
            PermissionActorContext,
            BTreeMap<String, PermissionAssignment>,
        ),
        QueryResponse,
    > {
        let actor = self.query_permission_actor_context(session)?;
        let Some(actor_permissions) = self.effective_permissions_for_client(
            actor.server_id,
            actor.channel_id,
            actor.client_database_id,
        ) else {
            return Err(QueryResponse::error(768, "query account not found"));
        };

        Ok((actor, actor_permissions))
    }

    pub(crate) fn insufficient_permission_response(&self, permission_name: &str) -> QueryResponse {
        QueryResponse::error_with_fields(
            ERROR_INSUFFICIENT_PERMISSIONS,
            "insufficient client permissions",
            [(
                "failed_permid",
                self.permission_id_for_name(permission_name).to_string(),
            )],
        )
    }

    pub(crate) fn ensure_playlist_view_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_view_power", "i_client_music_play_power"],
            &["i_playlist_needed_view_power"],
            "i_playlist_view_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(crate) fn ensure_playlist_modify_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_modify_power", "i_client_music_play_power"],
            &["i_playlist_needed_modify_power"],
            "i_playlist_modify_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(crate) fn ensure_playlist_permission_modify_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        if permission_power_max_or_default(actor_permissions, &["b_permission_modify_power_ignore"])
            > 0
        {
            return Ok(());
        }

        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &[
                "i_playlist_permission_modify_power",
                "i_permission_modify_power",
                "i_client_music_play_power",
            ],
            &["i_playlist_needed_permission_modify_power"],
            "i_playlist_permission_modify_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(crate) fn ensure_playlist_song_add_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_song_add_power", "i_client_music_play_power"],
            &["i_playlist_song_needed_add_power"],
            "i_playlist_song_add_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(crate) fn ensure_playlist_song_move_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_song_move_power", "i_client_music_play_power"],
            &["i_playlist_song_needed_move_power"],
            "i_playlist_song_move_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(crate) fn ensure_playlist_song_remove_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_song_remove_power", "i_client_music_play_power"],
            &["i_playlist_song_needed_remove_power"],
            "i_playlist_song_remove_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(crate) fn ensure_playlist_permission_list_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        if permission_power_max_or_default(
            actor_permissions,
            &["b_virtualserver_playlist_permission_list"],
        ) > 0
        {
            return Ok(());
        }

        self.ensure_playlist_view_allowed(actor_permissions, target_permissions)
    }

    pub(crate) fn parse_permission_assignments(
        &self,
        request: &CommandRequest,
        global_keys: &[&str],
    ) -> std::result::Result<Vec<ParsedPermissionAssignment>, QueryResponse> {
        let groups = self.permission_option_groups(request, global_keys);
        let mut parsed_assignments = Vec::new();

        for group in groups {
            let Some(permission_name) = self.resolve_permission_name_from_args(&group) else {
                return Err(QueryResponse::error(512, "permid or permsid is required"));
            };
            let Some(value) = group
                .get("permvalue")
                .and_then(|value| value.parse::<i64>().ok())
            else {
                return Err(QueryResponse::error(512, "permvalue is required"));
            };
            let negated = group
                .get("permnegated")
                .and_then(|value| parse_query_bool(value))
                .unwrap_or(false);
            let skipped = group
                .get("permskip")
                .and_then(|value| parse_query_bool(value))
                .unwrap_or(false);
            parsed_assignments.push(ParsedPermissionAssignment {
                name: permission_name,
                assignment: PermissionAssignment {
                    value,
                    negated,
                    skipped,
                },
            });
        }

        if parsed_assignments.is_empty() {
            return Err(QueryResponse::error(
                512,
                "no permission assignments provided",
            ));
        }

        Ok(parsed_assignments)
    }

    pub(crate) fn parse_requested_permission_names(
        &self,
        request: &CommandRequest,
        global_keys: &[&str],
    ) -> std::result::Result<Vec<String>, QueryResponse> {
        let groups = self.permission_option_groups(request, global_keys);
        let mut permission_names = Vec::new();

        for group in groups {
            let Some(permission_name) = self.resolve_permission_name_from_args(&group) else {
                return Err(QueryResponse::error(512, "permid or permsid is required"));
            };
            permission_names.push(permission_name);
        }

        if permission_names.is_empty() {
            return Err(QueryResponse::error(512, "no permissions requested"));
        }

        Ok(permission_names)
    }

    pub(crate) fn resolve_permission_name_from_args(&self, args: &BTreeMap<String, String>) -> Option<String> {
        if let Some(permission_name) = args.get("permsid") {
            return Some(permission_name.clone());
        }

        args.get("permid")
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|permission_id| self.permission_name_for_id(permission_id))
    }

    pub(crate) fn permission_id_for_name(&self, permission_name: &str) -> u32 {
        if let Some(rest) = permission_name.strip_prefix("i_needed_modify_power_") {
            let b_name = format!("b_{}", rest);
            let i_name = format!("i_{}", rest);
            let s_name = format!("s_{}", rest);
            if let Some(base_id) = self
                .permission_catalog
                .get(&b_name)
                .map(|e| e.id)
                .or_else(|| self.permission_catalog.get(&i_name).map(|e| e.id))
                .or_else(|| self.permission_catalog.get(&s_name).map(|e| e.id))
                .or_else(|| self.web_permission_base_ids.get(&b_name).copied())
                .or_else(|| self.web_permission_base_ids.get(&i_name).copied())
                .or_else(|| self.web_permission_base_ids.get(&s_name).copied())
            {
                return base_id | 0x8000;
            }
        }
        if let Some(entry) = self.permission_catalog.get(permission_name) {
            return entry.id;
        }
        if let Some(id) = self.web_permission_base_ids.get(permission_name) {
            return *id;
        }
        synthetic_permission_id(permission_name)
    }

    pub(crate) fn permission_description_for_name(&self, permission_name: &str) -> String {
        self.permission_catalog
            .get(permission_name)
            .map(|entry| entry.description.clone())
            .unwrap_or_else(|| describe_permission_name(permission_name))
    }

    pub(crate) fn build_web_permission_layout(&self) -> WebPermissionLayout {
        let known_permission_names = self.all_known_permission_names();
        if self.web_permission_base_ids.is_empty() {
            let ids_by_name = known_permission_names
                .into_iter()
                .map(|permission_name| {
                    let permission_id = self.permission_id_for_name(&permission_name);
                    (permission_name, permission_id)
                })
                .collect::<BTreeMap<_, _>>();
            let group_markers = ids_by_name
                .values()
                .copied()
                .max()
                .map(|max_id| vec![max_id])
                .unwrap_or_default();
            return WebPermissionLayout {
                ids_by_name,
                group_markers,
            };
        }

        let mut ids_by_name = known_permission_names
            .iter()
            .filter_map(|permission_name| {
                self.web_permission_base_ids
                    .get(permission_name)
                    .copied()
                    .map(|permission_id| (permission_name.clone(), permission_id))
            })
            .collect::<BTreeMap<_, _>>();

        let mut next_id = self
            .web_permission_base_ids
            .values()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut fallback_permissions = known_permission_names
            .into_iter()
            .filter(|permission_name| !ids_by_name.contains_key(permission_name))
            .collect::<Vec<_>>();
        fallback_permissions.sort_by(|left_name, right_name| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        for permission_name in fallback_permissions {
            ids_by_name.insert(permission_name, next_id);
            next_id = next_id.saturating_add(1);
        }

        let max_id = ids_by_name.values().copied().max().unwrap_or(0);
        let mut group_markers = TEAWEB_PERMISSION_GROUP_ENDS
            .iter()
            .copied()
            .filter(|group_id_end| *group_id_end <= max_id)
            .collect::<Vec<_>>();
        if max_id > 0 && group_markers.last().copied() != Some(max_id) {
            group_markers.push(max_id);
        }

        WebPermissionLayout {
            ids_by_name,
            group_markers,
        }
    }

    pub(crate) fn permission_name_for_id(&self, permission_id: u32) -> Option<String> {
        if permission_id >= 0x8000 {
            let base_id = permission_id & 0x7FFF;
            if let Some(base_name) = self.permission_name_for_id(base_id) {
                if let Some(rest) = base_name
                    .strip_prefix("b_")
                    .or_else(|| base_name.strip_prefix("i_"))
                {
                    return Some(format!("i_needed_modify_power_{}", rest));
                }
            }
        }

        self.permission_catalog
            .iter()
            .find_map(|(permission_name, entry)| {
                (entry.id == permission_id).then(|| permission_name.clone())
            })
            .or_else(|| {
                self.web_permission_base_ids
                    .iter()
                    .find_map(|(permission_name, id)| {
                        (*id == permission_id).then(|| permission_name.clone())
                    })
            })
            .or_else(|| {
                let layout = self.build_web_permission_layout();
                layout
                    .ids_by_name
                    .into_iter()
                    .find_map(|(permission_name, id)| {
                        (id == permission_id).then_some(permission_name)
                    })
            })
            .or_else(|| {
                self.store
                    .server_groups
                    .values()
                    .flat_map(|group| group.permissions.keys())
                    .chain(self.store.channels.values().flat_map(|channels| {
                        channels
                            .iter()
                            .flat_map(|channel| channel.permissions.keys())
                    }))
                    .chain(
                        self.store
                            .channel_groups
                            .values()
                            .flat_map(|group| group.permissions.keys()),
                    )
                    .chain(
                        self.store
                            .channel_client_permissions
                            .iter()
                            .flat_map(|target| target.permissions.keys()),
                    )
                    .chain(
                        self.store
                            .query_accounts
                            .values()
                            .flat_map(|account| account.permissions.keys()),
                    )
                    .chain(
                        self.store
                            .music_bots
                            .values()
                            .flat_map(|bot| bot.permissions.keys()),
                    )
                    .chain(
                        self.store.music_bots.values().flat_map(|bot| {
                            bot.client_permissions
                                .iter()
                                .flat_map(|target| target.permissions.keys())
                        }),
                    )
                    .find(|permission_name| {
                        synthetic_permission_id(permission_name) == permission_id
                    })
                    .cloned()
            })
    }

    pub(crate) fn all_known_permission_names(&self) -> Vec<String> {
        let mut permission_names = self
            .permission_catalog
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        permission_names.extend(
            self.store
                .server_groups
                .values()
                .flat_map(|group| group.permissions.keys().cloned()),
        );
        permission_names.extend(
            self.store
                .channel_groups
                .values()
                .flat_map(|group| group.permissions.keys().cloned()),
        );
        permission_names.extend(self.store.channels.values().flat_map(|channels| {
            channels
                .iter()
                .flat_map(|channel| channel.permissions.keys().cloned())
        }));
        permission_names.extend(
            self.store
                .channel_client_permissions
                .iter()
                .flat_map(|target| target.permissions.keys().cloned()),
        );
        permission_names.extend(
            self.store
                .query_accounts
                .values()
                .flat_map(|account| account.permissions.keys().cloned()),
        );
        permission_names.extend(
            self.store
                .music_bots
                .values()
                .flat_map(|bot| bot.permissions.keys().cloned()),
        );
        permission_names.extend(self.store.music_bots.values().flat_map(|bot| {
            bot.client_permissions
                .iter()
                .flat_map(|target| target.permissions.keys().cloned())
        }));
        permission_names.into_iter().collect()
    }

    pub(crate) fn knows_permission_name(&self, permission_name: &str) -> bool {
        if let Some(rest) = permission_name.strip_prefix("i_needed_modify_power_") {
            let b_name = format!("b_{}", rest);
            let i_name = format!("i_{}", rest);
            if self.permission_catalog.contains_key(&b_name)
                || self.permission_catalog.contains_key(&i_name)
                || self.web_permission_base_ids.contains_key(&b_name)
                || self.web_permission_base_ids.contains_key(&i_name)
            {
                return true;
            }
        }
        self.permission_catalog.contains_key(permission_name)
            || self.web_permission_base_ids.contains_key(permission_name)
            || self
                .store
                .server_groups
                .values()
                .any(|group| group.permissions.contains_key(permission_name))
            || self
                .store
                .channel_groups
                .values()
                .any(|group| group.permissions.contains_key(permission_name))
            || self.store.channels.values().any(|channels| {
                channels
                    .iter()
                    .any(|channel| channel.permissions.contains_key(permission_name))
            })
            || self
                .store
                .channel_client_permissions
                .iter()
                .any(|target| target.permissions.contains_key(permission_name))
            || self
                .store
                .query_accounts
                .values()
                .any(|account| account.permissions.contains_key(permission_name))
            || self
                .store
                .music_bots
                .values()
                .any(|bot| bot.permissions.contains_key(permission_name))
            || self.store.music_bots.values().any(|bot| {
                bot.client_permissions
                    .iter()
                    .any(|target| target.permissions.contains_key(permission_name))
            })
    }

    pub(crate) fn effective_permissions_for_account(
        &self,
        account: &QueryAccount,
    ) -> BTreeMap<String, PermissionAssignment> {
        let mut effective_permissions = BTreeMap::<String, PermissionAssignment>::new();

        for group_id in &account.server_groups {
            if let Some(group) = self.store.server_groups.get(group_id) {
                for (permission_name, assignment) in &group.permissions {
                    let should_replace = match effective_permissions.get(permission_name) {
                        Some(current_assignment) => assignment.value >= current_assignment.value,
                        None => true,
                    };
                    if should_replace {
                        effective_permissions.insert(permission_name.clone(), assignment.clone());
                    }
                }
            }
        }

        for (permission_name, assignment) in &account.permissions {
            effective_permissions.insert(permission_name.clone(), assignment.clone());
        }

        effective_permissions
    }

    pub(crate) fn effective_permissions_for_client(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<BTreeMap<String, PermissionAssignment>> {
        let resolved_channel_id = if self.query_account_by_cldbid(client_database_id).is_some()
        {
            self.online_client_by_cldbid(server_id, client_database_id)
                .map(|client| client.channel_id)
                .unwrap_or(channel_id)
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            client.channel_id
        } else if self.client_permission_target(server_id, client_database_id).is_some()
            || self.store.clients.contains_key(&client_database_id)
        {
            channel_id
        } else {
            return None;
        };

        self.effective_permissions_for_client_in_channel_context(
            server_id,
            resolved_channel_id,
            client_database_id,
        )
    }

    pub(crate) fn effective_permissions_for_client_in_channel_context(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<BTreeMap<String, PermissionAssignment>> {
        let mut effective_permissions = BTreeMap::<String, PermissionAssignment>::new();

        // 1. Server Groups (Tier 1)
        let mut server_group_perms = Vec::new();
        if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            for group_id in &account.server_groups {
                if let Some(group) = self.store.server_groups.get(group_id) {
                    server_group_perms.push(&group.permissions);
                }
            }
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            for group_id in &client.server_groups {
                if let Some(group) = self.store.server_groups.get(group_id) {
                    server_group_perms.push(&group.permissions);
                }
            }
        } else if let Some(raw_groups) = self
            .store
            .custom_client_properties
            .get(&(0, client_database_id))
            .and_then(|props| props.get("server_groups"))
        {
            for group_id in raw_groups
                .split(',')
                .filter_map(|part| part.trim().parse::<u32>().ok())
            {
                if let Some(group) = self.store.server_groups.get(&group_id) {
                    server_group_perms.push(&group.permissions);
                }
            }
        }

        // Merge Server Groups
        let mut combined_sg_perms: BTreeMap<String, Vec<&PermissionAssignment>> = BTreeMap::new();
        for group_perms in server_group_perms {
            for (name, assignment) in group_perms {
                combined_sg_perms.entry(name.clone()).or_default().push(assignment);
            }
        }
        for (name, assignments) in combined_sg_perms {
            let is_negated = assignments.iter().any(|a| a.negated);
            let is_skipped = assignments.iter().any(|a| a.skipped);
            let value = if is_negated {
                assignments.iter().map(|a| a.value).min().unwrap()
            } else {
                assignments.iter().map(|a| a.value).max().unwrap()
            };
            effective_permissions.insert(name, PermissionAssignment {
                value,
                negated: is_negated,
                skipped: is_skipped,
            });
        }

        // 2. Client Permissions (Tier 2)
        if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            for (permission_name, assignment) in &account.permissions {
                effective_permissions.insert(permission_name.clone(), assignment.clone());
            }
        } else if let Some(target) = self.client_permission_target(server_id, client_database_id) {
            for (permission_name, assignment) in &target.permissions {
                effective_permissions.insert(permission_name.clone(), assignment.clone());
            }
        }

        // Check b_client_skip_channelgroup_permissions
        let global_skip = effective_permissions
            .get("b_client_skip_channelgroup_permissions")
            .map(|a| a.value != 0)
            .unwrap_or(false);

        if !global_skip {
            // 3. Channel Permissions (Tier 3)
            if let Some(channel) = self.channel_by_id(server_id, channel_id) {
                for (permission_name, assignment) in &channel.permissions {
                    if let Some(existing) = effective_permissions.get(permission_name) {
                        if existing.skipped {
                            continue; // Tier 1/2 skip flag prevents override
                        }
                    }
                    effective_permissions.insert(permission_name.clone(), assignment.clone());
                }
            }

            // 4. Channel Groups (Tier 4)
            if let Some(group) = self.effective_channel_group(channel_id, client_database_id) {
                for (permission_name, assignment) in &group.permissions {
                    if let Some(existing) = effective_permissions.get(permission_name) {
                        if existing.skipped {
                            continue; // Tier 1/2 skip flag prevents override
                        }
                    }
                    effective_permissions.insert(permission_name.clone(), assignment.clone());
                }
            }

            // 5. Channel Client Permissions (Tier 5)
            if let Some(target) = self.store.channel_client_permissions.iter().find(|target| {
                target.channel_id == channel_id && target.client_database_id == client_database_id
            }) {
                for (permission_name, assignment) in &target.permissions {
                    if let Some(existing) = effective_permissions.get(permission_name) {
                        if existing.skipped {
                            continue; // Tier 1/2 skip flag prevents override
                        }
                    }
                    effective_permissions.insert(permission_name.clone(), assignment.clone());
                }
            }
        }

        Some(effective_permissions)
    }

    pub fn web_client_can_view_channel(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> bool {
        let Some(effective_permissions) = self.effective_permissions_for_client_in_channel_context(
            server_id,
            channel_id,
            client_database_id,
        ) else {
            return false;
        };

        let has_explicit_info_view = effective_permissions.contains_key("b_channel_info_view");
        let has_explicit_view_power = effective_permissions.contains_key("b_channel_ignore_view_power")
            || effective_permissions.contains_key("i_channel_view_power")
            || effective_permissions.contains_key("i_channel_needed_view_power");

        if !has_explicit_info_view && !has_explicit_view_power {
            return true;
        }

        if has_explicit_info_view
            && permission_value_or_default(&effective_permissions, &["b_channel_info_view"]) < 1
        {
            return false;
        }

        if permission_value_or_default(&effective_permissions, &["b_channel_ignore_view_power"]) > 0 {
            return true;
        }

        permission_power_max_or_default(&effective_permissions, &["i_channel_view_power"])
            >= permission_value_or_default(&effective_permissions, &["i_channel_needed_view_power"])
    }

    pub(crate) fn build_web_needed_permission_rows(
        &self,
        permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> Vec<BTreeMap<String, String>> {
        let layout = self.build_web_permission_layout();
        let mut sorted_permissions = permissions.iter().collect::<Vec<_>>();
        sorted_permissions.sort_by(|(left_name, _), (right_name, _)| {
            layout
                .ids_by_name
                .get(*left_name)
                .copied()
                .unwrap_or_else(|| self.permission_id_for_name(left_name))
                .cmp(
                    &layout
                        .ids_by_name
                        .get(*right_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(right_name)),
                )
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = sorted_permissions
            .iter()
            .map(|(permission_name, assignment)| {
                self.render_permission_row_with_id(
                    layout
                        .ids_by_name
                        .get(*permission_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(permission_name)),
                    permission_name,
                    assignment,
                    false,
                )
            })
            .collect::<Vec<_>>();

        let ignore_modify =
            permission_value_or_default(permissions, &["b_permission_modify_power_ignore"]) > 0;
        let default_grant_power =
            permission_value_or_default(permissions, &["i_permission_modify_power"]);
        let grant_fallback = if default_grant_power > 0 {
            default_grant_power
        } else if ignore_modify {
            75
        } else {
            0
        };

        if grant_fallback > 0 {
            for (permission_name, _) in &sorted_permissions {
                if permission_name.starts_with("i_needed_modify_power_") {
                    continue;
                }
                let base_id = layout
                    .ids_by_name
                    .get(*permission_name)
                    .copied()
                    .unwrap_or_else(|| self.permission_id_for_name(permission_name));
                if base_id >= 0x8000 {
                    continue;
                }
                let grant_val = permission_name
                    .strip_prefix("b_")
                    .or_else(|| permission_name.strip_prefix("i_"))
                    .and_then(|rest| {
                        permissions
                            .get(&format!("i_needed_modify_power_{}", rest))
                            .map(|a| a.value)
                    })
                    .unwrap_or(grant_fallback);
                if grant_val > 0 {
                    rows.push(self.render_permission_row_with_id(
                        base_id | 0x8000,
                        permission_name,
                        &PermissionAssignment {
                            value: grant_val,
                            negated: false,
                            skipped: false,
                        },
                        false,
                    ));
                }
            }
        }

        if let Some(first_row) = rows.first_mut() {
            first_row.insert(String::from("relative"), String::from("0"));
        } else {
            let mut row = BTreeMap::new();
            row.insert(String::from("relative"), String::from("0"));
            rows.push(row);
        }

        rows
    }

    pub(crate) fn render_permission_row(
        &self,
        permission_name: &str,
        assignment: &PermissionAssignment,
        include_permission_name: bool,
    ) -> BTreeMap<String, String> {
        self.render_permission_row_with_id(
            self.permission_id_for_name(permission_name),
            permission_name,
            assignment,
            include_permission_name,
        )
    }

    pub(crate) fn render_permission_row_with_id(
        &self,
        permission_id: u32,
        permission_name: &str,
        assignment: &PermissionAssignment,
        include_permission_name: bool,
    ) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("permid"), permission_id.to_string());
        row.insert(String::from("permvalue"), assignment.value.to_string());
        row.insert(
            String::from("permnegated"),
            if assignment.negated {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("permskip"),
            if assignment.skipped {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        if include_permission_name {
            row.insert(String::from("permsid"), permission_name.to_string());
        }
        row
    }

    pub(crate) fn build_web_permission_assignment_rows(
        &self,
        permissions: &BTreeMap<String, PermissionAssignment>,
        subject_fields: &[(&str, String)],
    ) -> Vec<BTreeMap<String, String>> {
        let layout = self.build_web_permission_layout();
        let resolve_id = |perm_name: &str| -> u32 {
            if perm_name.starts_with("i_needed_modify_power_") {
                self.permission_id_for_name(perm_name)
            } else {
                layout
                    .ids_by_name
                    .get(perm_name)
                    .copied()
                    .unwrap_or_else(|| self.permission_id_for_name(perm_name))
            }
        };
        let mut sorted_permissions = permissions.iter().collect::<Vec<_>>();
        sorted_permissions.sort_by(|(left_name, _), (right_name, _)| {
            resolve_id(left_name)
                .cmp(&resolve_id(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = sorted_permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row_with_id(
                    resolve_id(permission_name),
                    permission_name,
                    assignment,
                    false,
                );
                for (key, value) in subject_fields {
                    row.insert((*key).to_string(), value.clone());
                }
                row
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            let mut row = BTreeMap::new();
            for (key, value) in subject_fields {
                row.insert((*key).to_string(), value.clone());
            }
            rows.push(row);
        }

        rows
    }

    pub(crate) fn render_permoverview_row(
        &self,
        target_type: u32,
        id1: u64,
        id2: u64,
        permission_name: &str,
        assignment: &PermissionAssignment,
    ) -> (u32, u64, u64, u32, BTreeMap<String, String>) {
        let permission_id = self.permission_id_for_name(permission_name);
        let mut row = BTreeMap::new();
        row.insert(String::from("t"), target_type.to_string());
        row.insert(String::from("id1"), id1.to_string());
        row.insert(String::from("id2"), id2.to_string());
        row.insert(String::from("p"), permission_id.to_string());
        row.insert(String::from("v"), assignment.value.to_string());
        row.insert(
            String::from("n"),
            if assignment.negated {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("s"),
            if assignment.skipped {
                String::from("1")
            } else {
                String::from("0")
            },
        );

        (target_type, id1, id2, permission_id, row)
    }
}
