use super::*;
impl BaselineRuntime {
    pub(crate) fn handle_servergrouplist(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        QueryResponse::ok_rows(self.web_server_group_rows())
    }

    pub(crate) fn handle_servergroupsbyclientid(
        &self,
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
        let actor = match self.query_permission_actor_context(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(rows) = self.web_server_groups_by_client_rows(actor.server_id, client_database_id) else {
            return QueryResponse::error(768, "client not found");
        };

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_servergroupclientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        if !self.store.server_groups.contains_key(&group_id) {
            return QueryResponse::error(768, "server group not found");
        }

        let include_names = request.flags.contains("names");
        let mut rows_by_cldbid = BTreeMap::new();

        for account in self
            .store
            .query_accounts
            .values()
            .filter(|account| account.server_groups.contains(&group_id))
        {
            if let Some(client_database_id) = account.client_database_id {
                let mut row = BTreeMap::new();
                row.insert(String::from("cldbid"), client_database_id.to_string());
                if include_names {
                    row.insert(String::from("name"), account.login_name.clone());
                    row.insert(String::from("client_nickname"), account.login_name.clone());
                    row.insert(
                        String::from("client_unique_identifier"),
                        format!("query-account-{}", client_database_id),
                    );
                }
                rows_by_cldbid.insert(client_database_id, row);
            }
        }

        for client in self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_groups.contains(&group_id))
        {
            let mut row = BTreeMap::new();
            row.insert(String::from("cldbid"), client.database_id.to_string());
            if include_names {
                row.insert(String::from("name"), client.nickname.clone());
                row.insert(String::from("client_nickname"), client.nickname.clone());
                row.insert(
                    String::from("client_unique_identifier"),
                    client.unique_identifier.clone(),
                );
            }
            rows_by_cldbid.insert(client.database_id, row);
        }

        QueryResponse::ok_rows(rows_by_cldbid.into_values().collect())
    }

    pub(crate) fn handle_servergroupadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_servergroup_create"],
            "b_virtualserver_servergroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let group_type = match request.named_args.get("type") {
            Some(value) => match value.parse::<u32>() {
                Ok(group_type) => group_type,
                Err(_) => return QueryResponse::error(512, "type must be an integer"),
            },
            None => 1,
        };

        let group_id = self.next_server_group_id();
        let new_group = ServerGroup {
            id: group_id,
            name: group_name,
            group_type,
            icon_id: 0,
            save_db: true,
            permissions: BTreeMap::new(),
        };
        let _ = self.db.save_server_group(0, &new_group);
        self.store.server_groups.insert(group_id, new_group);

        let mut row = BTreeMap::new();
        row.insert(String::from("sgid"), group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_servergroupaddclient(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        match self.web_add_server_group_client(
            actor.server_id,
            actor.channel_id,
            actor.client_database_id,
            group_id,
            client_database_id,
        ) {
            Ok(()) => QueryResponse::ok(),
            Err(WebServerGroupMutationError::InvalidGroup) => {
                QueryResponse::error(768, "server group not found")
            }
            Err(WebServerGroupMutationError::InvalidClient) => {
                QueryResponse::error(768, "client not found")
            }
            Err(WebServerGroupMutationError::PermissionDenied {
                failed_permission_id,
            }) => QueryResponse::error_with_fields(
                ERROR_INSUFFICIENT_PERMISSIONS,
                "insufficient client permissions",
                [("failed_permid", failed_permission_id.to_string())],
            ),
        }
    }

    pub(crate) fn handle_servergroupdelclient(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        match self.web_del_server_group_client(
            actor.server_id,
            actor.channel_id,
            actor.client_database_id,
            group_id,
            client_database_id,
        ) {
            Ok(()) => QueryResponse::ok(),
            Err(WebServerGroupMutationError::InvalidGroup) => {
                QueryResponse::error(768, "server group not found")
            }
            Err(WebServerGroupMutationError::InvalidClient) => {
                QueryResponse::error(768, "client not found")
            }
            Err(WebServerGroupMutationError::PermissionDenied {
                failed_permission_id,
            }) => QueryResponse::error_with_fields(
                ERROR_INSUFFICIENT_PERMISSIONS,
                "insufficient client permissions",
                [("failed_permid", failed_permission_id.to_string())],
            ),
        }
    }

    pub(crate) fn handle_servergroupdel(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_servergroup_delete"],
            "b_virtualserver_servergroup_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let force = request
            .named_args
            .get("force")
            .and_then(|value| parse_query_bool(value))
            .unwrap_or(false);
        if !self.store.server_groups.contains_key(&group_id) {
            return QueryResponse::error(768, "server group not found");
        }
        if self.server_group_in_use(group_id) && !force {
            return QueryResponse::error(
                512,
                "server group has assigned clients; set force=1 to delete",
            );
        }

        self.store.server_groups.remove(&group_id);
        for account in self.store.query_accounts.values_mut() {
            account
                .server_groups
                .retain(|existing_group_id| *existing_group_id != group_id);
        }
        self.normalize_query_account_groups();
        self.normalize_online_client_groups();

        QueryResponse::ok()
    }

    pub(crate) fn handle_servergroupaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["sgid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        for parsed_assignment in parsed_assignments {
            group
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_server_group(0, group);

        QueryResponse::ok()
    }

    pub(crate) fn handle_servergroupautoaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(sgtype) = request
            .named_args
            .get("sgtype")
            .and_then(|value| value.parse::<i64>().ok())
        else {
            return QueryResponse::error(512, "sgtype is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["sgtype"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let target_group_ids = self.server_group_ids_by_auto_type(sgtype);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_permission_modify_power_ignore"],
            "b_permission_modify_power_ignore",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        for group_id in target_group_ids {
            if let Some(group) = self.store.server_groups.get_mut(&group_id) {
                for parsed_assignment in &parsed_assignments {
                    group.permissions.insert(
                        parsed_assignment.name.clone(),
                        parsed_assignment.assignment.clone(),
                    );
                }
            }
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_servergroupautodelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(sgtype) = request
            .named_args
            .get("sgtype")
            .and_then(|value| value.parse::<i64>().ok())
        else {
            return QueryResponse::error(512, "sgtype is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["sgtype"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let target_group_ids = self.server_group_ids_by_auto_type(sgtype);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_permission_modify_power_ignore"],
            "b_permission_modify_power_ignore",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        for group_id in target_group_ids {
            if let Some(group) = self.store.server_groups.get_mut(&group_id) {
                for permission_name in &permission_names {
                    group.permissions.remove(permission_name);
                }
            }
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_servergroupcopy(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(source_group_id) = request
            .named_args
            .get("ssgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "ssgid is required");
        };
        let Some(target_group_id) = request
            .named_args
            .get("tsgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "tsgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_servergroup_create"],
            "b_virtualserver_servergroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(source_group) = self.store.server_groups.get(&source_group_id).cloned() else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &source_group.permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if target_group_id != 0 {
            let Some(target_group_permissions) = self
                .store
                .server_groups
                .get(&target_group_id)
                .map(|group| group.permissions.clone())
            else {
                return QueryResponse::error(768, "server group not found");
            };
            if let Err(permission_name) = check_group_modify_allowed(
                &actor_permissions,
                &target_group_permissions,
                &[
                    "i_server_group_needed_modify_power",
                    "i_group_needed_modify_power",
                ],
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }

        let resulting_group_id = if target_group_id == 0 {
            let Some(group_name) = request.named_args.get("name").cloned() else {
                return QueryResponse::error(512, "name is required when tsgid=0");
            };
            let group_type = request
                .named_args
                .get("type")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(source_group.group_type);
            let new_group_id = self.next_server_group_id();
            self.store.server_groups.insert(
                new_group_id,
                ServerGroup {
                    id: new_group_id,
                    name: group_name,
                    group_type,
                    icon_id: source_group.icon_id,
                    save_db: source_group.save_db,
                    permissions: source_group.permissions.clone(),
                },
            );
            new_group_id
        } else {
            let Some(target_group) = self.store.server_groups.get_mut(&target_group_id) else {
                return QueryResponse::error(768, "server group not found");
            };
            target_group.icon_id = source_group.icon_id;
            target_group.save_db = source_group.save_db;
            target_group.permissions = source_group.permissions.clone();
            target_group_id
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("sgid"), resulting_group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_servergroupdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["sgid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        for permission_name in permission_names {
            group.permissions.remove(&permission_name);
        }
        let _ = self.db.save_server_group(0, group);

        QueryResponse::ok()
    }

    pub(crate) fn handle_servergrouprename(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        group.name = group_name;
        QueryResponse::ok()
    }

    pub(crate) fn handle_servergrouppermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let Some(group) = self.store.server_groups.get(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        let mut permissions = group.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                )
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub fn ensure_initial_admin_token(&mut self, server_id: u32) -> (String, bool) {
        if let Some(existing) = self
            .store
            .tokens
            .values()
            .find(|t| {
                t.server_id == server_id
                    && (t.max_uses == 0 || t.uses < t.max_uses)
                    && t.actions.iter().any(|a| a.action_type == 2 && (a.action_id1 == 6 || a.action_id1 == 9))
            })
        {
            return (existing.token.clone(), false);
        }

        let token_id = self.next_token_id();
        let created_at = current_unix_timestamp();
        let token_value = format!("compat{:08x}{:016x}", token_id, created_at);
        let action_id_1 = self.next_token_action_id();
        let action_id_2 = self.next_token_action_id();
        let token_obj = PrivilegeToken {
            id: token_id,
            server_id,
            token: token_value.clone(),
            description: String::from("Initial Server Admin Privilege Key"),
            max_uses: 1,
            uses: 0,
            created_at,
            owner_login: String::from("serveradmin"),
            expired_at: None,
            actions: vec![
                TokenAction {
                    id: action_id_1,
                    action_type: 2,
                    action_id1: 9,
                    action_id2: 0,
                    action_text: String::new(),
                },
                TokenAction {
                    id: action_id_2,
                    action_type: 2,
                    action_id1: 6,
                    action_id2: 0,
                    action_text: String::new(),
                },
            ],
        };
        let _ = self.db.save_token(&token_obj);
        self.store.tokens.insert(token_id, token_obj);
        self.persist_state_best_effort();
        (token_value, true)
    }

    pub fn reset_permissions_and_generate_admin_token(&mut self, server_id: u32) -> String {
        let admin_permissions = build_named_permission_map(&self.specs, "Admin Server Query", "QUERY");
        let guest_permissions = build_named_permission_map(&self.specs, "Guest Server Query", "QUERY");
        let server_admin_permissions = build_named_permission_map(&self.specs, "Server Admin", "SERVER");
        let channel_admin_permissions = build_named_permission_map(&self.specs, "Channel Admin", "CHANNEL");
        let channel_operator_permissions = build_named_permission_map(&self.specs, "Operator", "CHANNEL");
        let channel_guest_permissions = build_named_permission_map(&self.specs, "Guest", "CHANNEL");

        let seeded = seed_store(
            &admin_permissions,
            &guest_permissions,
            &server_admin_permissions,
            &channel_admin_permissions,
            &channel_operator_permissions,
            &channel_guest_permissions,
        );

        self.store.server_groups = seeded.server_groups;
        self.store.channel_groups = seeded.channel_groups;
        self.store.channel_group_assignments = seeded.channel_group_assignments;
        self.store.client_permissions.clear();
        self.store.channel_client_permissions.clear();

        if let Some(channels) = self.store.channels.get_mut(&server_id) {
            for channel in channels {
                channel.permissions.clear();
            }
        }

        // Restore serveradmin query account and reset other query accounts to default groups
        self.store.query_accounts.insert(
            String::from("serveradmin"),
            QueryAccount {
                login_name: String::from("serveradmin"),
                password: String::from("serveradmin"),
                server_id: Some(1),
                client_database_id: Some(1),
                server_groups: vec![6],
                permissions: BTreeMap::new(),
            },
        );
        for (login, account) in &mut self.store.query_accounts {
            if login != "serveradmin" {
                account.server_groups = vec![7];
                account.permissions.clear();
            }
        }

        // Reset all online clients on this server to Normal/Guest (sgid=8)
        for client in self.store.online_clients.values_mut() {
            if client.server_id == server_id {
                client.server_groups = vec![8];
            }
        }
        self.store
            .custom_client_properties
            .retain(|(sid, _), _| *sid != 0);

        self.ensure_web_server_group_assignment_permission_basis();

        // Sync SQLite database
        let _ = self.db.clear_all_permissions_and_groups();
        for group in self.store.server_groups.values() {
            let _ = self.db.save_server_group(0, group);
        }
        for group in self.store.channel_groups.values() {
            let _ = self.db.save_channel_group(0, group);
        }
        for assignment in &self.store.channel_group_assignments {
            let _ = self.db.save_channel_group_assignment(server_id, assignment);
        }

        // Create a fresh Server Admin Privilege Key
        let token_id = self.next_token_id();
        let created_at = current_unix_timestamp();
        let token_value = format!("compat{:08x}{:016x}", token_id, created_at);
        let action_id_1 = self.next_token_action_id();
        let action_id_2 = self.next_token_action_id();
        let token_obj = PrivilegeToken {
            id: token_id,
            server_id,
            token: token_value.clone(),
            description: String::from("Emergency PermReset Server Admin Token"),
            max_uses: 1,
            uses: 0,
            created_at,
            owner_login: String::from("serveradmin"),
            expired_at: None,
            actions: vec![
                TokenAction {
                    id: action_id_1,
                    action_type: 2,
                    action_id1: 9,
                    action_id2: 0,
                    action_text: String::new(),
                },
                TokenAction {
                    id: action_id_2,
                    action_type: 2,
                    action_id1: 6,
                    action_id2: 0,
                    action_text: String::new(),
                },
            ],
        };
        let _ = self.db.save_token(&token_obj);
        self.store.tokens.insert(token_id, token_obj);
        self.persist_state_best_effort();

        token_value
    }

    pub(crate) fn handle_permreset(&mut self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let server_id = session.selected_virtual_server_id.unwrap_or(1);

        // Allow local/query `serveradmin` login unconditionally so a locked-out admin can always recover!
        let is_serveradmin_recovery = session
            .authenticated_login
            .as_deref()
            .is_some_and(|login| login == "serveradmin");

        if !is_serveradmin_recovery {
            let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
                Ok(actor) => actor,
                Err(response) => return response,
            };
            if let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &[
                    "b_virtualserver_permission_reset",
                    "b_permission_modify_power_ignore",
                ],
                "b_virtualserver_permission_reset",
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }

        let token = self.reset_permissions_and_generate_admin_token(server_id);
        let mut row = BTreeMap::new();
        row.insert(String::from("token"), token);
        QueryResponse::ok_rows(vec![row])
    }
}
