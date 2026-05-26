use super::*;
impl BaselineRuntime {
    pub(crate) fn handle_servergrouplist(&self, session: &QuerySessionState) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let rows = self
            .store
            .server_groups
            .values()
            .map(|group| {
                let mut row = BTreeMap::new();
                row.insert(String::from("sgid"), group.id.to_string());
                row.insert(String::from("name"), group.name.clone());
                row.insert(String::from("type"), group.group_type.to_string());
                row.insert(String::from("iconid"), group.icon_id.to_string());
                row.insert(
                    String::from("savedb"),
                    if group.save_db {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_servergroupsbyclientid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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

        let rows = self
            .store
            .query_accounts
            .values()
            .filter(|account| account.server_groups.contains(&group_id))
            .filter_map(|account| {
                account.client_database_id.map(|client_database_id| {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    if request.flags.contains("names") {
                        row.insert(String::from("name"), account.login_name.clone());
                        row.insert(
                            String::from("client_unique_identifier"),
                            format!("query-account-{}", client_database_id),
                        );
                    }
                    row
                })
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_servergroupadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
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
        self.store.server_groups.insert(
            group_id,
            ServerGroup {
                id: group_id,
                name: group_name,
                group_type,
                icon_id: 0,
                save_db: true,
                permissions: BTreeMap::new(),
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("sgid"), group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_servergroupaddclient(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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
        if session.authenticated_login.is_none() {
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
}

impl BaselineRuntime {
    pub(crate) fn handle_channelgroupadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
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
            &["b_virtualserver_channelgroup_create"],
            "b_virtualserver_channelgroup_create",
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

        let group_id = self.next_channel_group_id();
        self.store.channel_groups.insert(
            group_id,
            ChannelGroup {
                id: group_id,
                name: group_name,
                group_type,
                icon_id: 0,
                save_db: true,
                permissions: BTreeMap::new(),
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("cgid"), group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_channelgroupaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["cgid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.channel_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        for parsed_assignment in parsed_assignments {
            group
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_channel_group(0, group);

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelgroupclientlist(
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
        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let valid_channel_ids = channels
            .iter()
            .map(|channel| channel.id)
            .collect::<BTreeSet<_>>();

        let channel_id_filter = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok());
        let client_database_id_filter = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok());
        let group_id_filter = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok());

        if let Some(channel_id) = channel_id_filter
            && !valid_channel_ids.contains(&channel_id)
        {
            return QueryResponse::error(768, "channel not found");
        }
        if let Some(group_id) = group_id_filter
            && !self.store.channel_groups.contains_key(&group_id)
        {
            return QueryResponse::error(768, "channel group not found");
        }

        let rows = self
            .store
            .channel_group_assignments
            .iter()
            .filter(|assignment| valid_channel_ids.contains(&assignment.channel_id))
            .filter(|assignment| {
                channel_id_filter.is_none_or(|channel_id| assignment.channel_id == channel_id)
            })
            .filter(|assignment| {
                client_database_id_filter.is_none_or(|client_database_id| {
                    assignment.client_database_id == client_database_id
                })
            })
            .filter(|assignment| {
                group_id_filter.is_none_or(|group_id| assignment.channel_group_id == group_id)
            })
            .map(|assignment| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cid"), assignment.channel_id.to_string());
                row.insert(
                    String::from("cldbid"),
                    assignment.client_database_id.to_string(),
                );
                row.insert(
                    String::from("cgid"),
                    assignment.channel_group_id.to_string(),
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_channelgroupcopy(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(source_group_id) = request
            .named_args
            .get("scgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "scgid is required");
        };
        let Some(target_group_id) = request
            .named_args
            .get("tcgid")
            .or_else(|| request.named_args.get("tsgid"))
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "tcgid or tsgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_channelgroup_create"],
            "b_virtualserver_channelgroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(source_group) = self.store.channel_groups.get(&source_group_id).cloned() else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &source_group.permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if target_group_id != 0 {
            let Some(target_group_permissions) = self
                .store
                .channel_groups
                .get(&target_group_id)
                .map(|group| group.permissions.clone())
            else {
                return QueryResponse::error(768, "channel group not found");
            };
            if let Err(permission_name) = check_group_modify_allowed(
                &actor_permissions,
                &target_group_permissions,
                &[
                    "i_channel_group_needed_modify_power",
                    "i_group_needed_modify_power",
                ],
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }

        let resulting_group_id = if target_group_id == 0 {
            let Some(group_name) = request.named_args.get("name").cloned() else {
                return QueryResponse::error(512, "name is required when tcgid=0");
            };
            let group_type = request
                .named_args
                .get("type")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(source_group.group_type);
            let new_group_id = self.next_channel_group_id();
            self.store.channel_groups.insert(
                new_group_id,
                ChannelGroup {
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
            let Some(target_group) = self.store.channel_groups.get_mut(&target_group_id) else {
                return QueryResponse::error(768, "channel group not found");
            };
            target_group.icon_id = source_group.icon_id;
            target_group.save_db = source_group.save_db;
            target_group.permissions = source_group.permissions.clone();
            target_group_id
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cgid"), resulting_group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_channelgroupdel(
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
            &["b_virtualserver_channelgroup_delete"],
            "b_virtualserver_channelgroup_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let force = request
            .named_args
            .get("force")
            .and_then(|value| parse_query_bool(value))
            .unwrap_or(false);
        if !self.store.channel_groups.contains_key(&group_id) {
            return QueryResponse::error(768, "channel group not found");
        }
        if self.channel_group_in_use(group_id) && !force {
            return QueryResponse::error(
                512,
                "channel group has assigned clients; set force=1 to delete",
            );
        }

        self.store.channel_groups.remove(&group_id);
        self.store
            .channel_group_assignments
            .retain(|assignment| assignment.channel_group_id != group_id);
        self.normalize_channel_group_assignments();

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelgroupdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["cgid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.channel_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        for permission_name in permission_names {
            group.permissions.remove(&permission_name);
        }
        let _ = self.db.save_channel_group(0, group);

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelgrouplist(&self, session: &QuerySessionState) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let rows = self
            .store
            .channel_groups
            .values()
            .map(|group| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cgid"), group.id.to_string());
                row.insert(String::from("name"), group.name.clone());
                row.insert(String::from("type"), group.group_type.to_string());
                row.insert(String::from("iconid"), group.icon_id.to_string());
                row.insert(
                    String::from("savedb"),
                    if group.save_db {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_channelgrouppermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let Some(group) = self.store.channel_groups.get(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
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

    pub(crate) fn handle_channelgrouprename(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let Some(group) = self.store.channel_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        group.name = group_name;
        QueryResponse::ok()
    }

    pub(crate) fn handle_setclientchannelgroup(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };

        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        if !channels.iter().any(|channel| channel.id == channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }

        let current_group_id = self
            .store
            .channel_group_assignments
            .iter()
            .find(|assignment| {
                assignment.channel_id == channel_id
                    && assignment.client_database_id == client_database_id
            })
            .map(|assignment| assignment.channel_group_id);

        if current_group_id == Some(group_id) {
            return QueryResponse::ok();
        }

        if let Some(existing_group_id) = current_group_id {
            let Some(current_group_permissions) = self
                .store
                .channel_groups
                .get(&existing_group_id)
                .map(|group| group.permissions.clone())
            else {
                return QueryResponse::error(768, "channel group not found");
            };
            if let Err(permission_name) = check_channel_group_membership_change(
                &actor_permissions,
                &current_group_permissions,
                false,
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }
        if let Err(permission_name) = check_channel_group_membership_change(
            &actor_permissions,
            &target_group_permissions,
            true,
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(assignment) =
            self.store
                .channel_group_assignments
                .iter_mut()
                .find(|assignment| {
                    assignment.channel_id == channel_id
                        && assignment.client_database_id == client_database_id
                })
        {
            assignment.channel_group_id = group_id;
        } else {
            self.store
                .channel_group_assignments
                .push(ChannelGroupAssignment {
                    channel_id,
                    client_database_id,
                    channel_group_id: group_id,
                });
        }
        self.normalize_channel_group_assignments();

        QueryResponse::ok()
    }
}

