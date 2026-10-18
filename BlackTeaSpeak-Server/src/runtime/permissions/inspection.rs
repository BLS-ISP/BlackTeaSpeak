use super::*;
impl BaselineRuntime {
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
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let effective_permissions = if let Some(login_name) = session.authenticated_login.as_ref()
            && session.actor_client_database_id_override.is_none()
            && session.selected_virtual_server_id.is_none()
        {
            let Some(account) = self.store.query_accounts.get(login_name) else {
                return QueryResponse::error(768, "query account not found");
            };
            self.effective_permissions_for_account(account)
        } else {
            match self.query_actor_effective_permissions(session) {
                Ok((_, perms)) => perms,
                Err(response) => return response,
            }
        };
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
        if !session_has_permission_actor(session) {
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
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        QueryResponse::ok_rows(self.build_permission_rows())
    }

    pub(crate) fn handle_permoverview(
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

    pub(crate) fn handle_desktopgetmyperms(
        &self,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Ok((_actor, actor_permissions)) = self.query_actor_effective_permissions(session) else {
            return QueryResponse::error(768, "client not found");
        };

        let mut rows = Vec::new();
        for (perm_name, assignment) in actor_permissions {
            let mut row = BTreeMap::new();
            row.insert(
                String::from("permid"),
                self.permission_id_for_name(&perm_name).to_string(),
            );
            row.insert(String::from("permname"), perm_name.clone());
            row.insert(String::from("permsid"), perm_name);
            row.insert(String::from("permvalue"), assignment.value.to_string());
            row.insert(
                String::from("permskip"),
                if assignment.skipped {
                    String::from("1")
                } else {
                    String::from("0")
                },
            );
            row.insert(
                String::from("permnegated"),
                if assignment.negated {
                    String::from("1")
                } else {
                    String::from("0")
                },
            );
            rows.push(row);
        }
        QueryResponse::ok_rows(rows)
    }
}
