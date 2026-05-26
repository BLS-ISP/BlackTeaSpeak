use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_help(&self, request: &CommandRequest) -> QueryResponse {
        if let Some(command_name) = request.positional_args.first() {
            if let Some(command) = self.specs.get_command(command_name) {
                let mut row = BTreeMap::new();
                row.insert(String::from("command"), command.name.clone());
                row.insert(String::from("category"), command.category.clone());
                row.insert(
                    String::from("description"),
                    normalize_text(&command.description),
                );
                row.insert(
                    String::from("implemented"),
                    self.is_command_implemented(&command.name).to_string(),
                );
                if !command.usage.is_empty() {
                    row.insert(
                        String::from("usage"),
                        normalize_text(&command.usage.join(" | ")),
                    );
                }
                if !command.permissions.is_empty() {
                    row.insert(String::from("permissions"), command.permissions.join(","));
                }
                return QueryResponse::ok_row(row);
            }
            return QueryResponse::error(768, format!("command {} not found", command_name));
        }

        let rows = self
            .specs
            .baseline_profile
            .essential_commands
            .iter()
            .map(|command| {
                let mut row = BTreeMap::new();
                row.insert(String::from("command"), command.name.clone());
                row.insert(String::from("category"), command.category.clone());
                row.insert(
                    String::from("implemented"),
                    self.is_command_implemented(&command.name).to_string(),
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_login(
        &self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let username = request
            .named_args
            .get("client_login_name")
            .cloned()
            .or_else(|| request.positional_args.first().cloned());
        let password = request
            .named_args
            .get("client_login_password")
            .cloned()
            .or_else(|| request.positional_args.get(1).cloned());

        let (username, password) = match (username, password) {
            (Some(username), Some(password)) => (username, password),
            _ => return QueryResponse::error(512, "missing login credentials"),
        };

        match self.store.query_accounts.get(&username) {
            Some(account) if account.password == password => {
                session.reset_client_state();
                session.authenticated_login = Some(account.login_name.clone());
                self.restore_session_from_snapshot(&account.login_name, account.server_id, session);
                QueryResponse::ok()
            }
            _ => QueryResponse::error(520, "authentication failed"),
        }
    }

    pub(crate) fn handle_logout(&self, session: &mut QuerySessionState) -> QueryResponse {
        session.reset_client_state();
        session.authenticated_login = None;
        session.selected_virtual_server_id = None;
        session.current_channel_id = None;
        session.virtual_mode = false;
        session.notification_subscriptions.clear();
        QueryResponse::ok()
    }

    pub(crate) fn handle_quit(&self) -> QueryResponse {
        QueryResponse::ok()
    }

    pub(crate) fn handle_sendtextmessage(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        match self.text_message_target(request, session) {
            Ok(target) => {
                let sender = self.query_session_participant(session);
                if target.target_mode == 1 {
                    let Some(target_client_id) = target.target_client_id else {
                        return QueryResponse::error(
                            512,
                            "target is required for private text messages",
                        );
                    };
                    let Some((target_unique_id, target_database_id, target_name)) =
                        self.online_client_identity(target.server_id, target_client_id)
                    else {
                        return QueryResponse::error(768, "target client not found");
                    };
                    self.record_private_message(
                        target.server_id,
                        sender,
                        ConversationParticipant {
                            database_id: target_database_id,
                            unique_identifier: target_unique_id,
                            nickname: target_name,
                        },
                        target.message.clone(),
                    );
                } else {
                    self.record_text_message(
                        &target,
                        sender.database_id,
                        sender.unique_identifier,
                        sender.nickname,
                    );
                }
                QueryResponse::ok()
            }
            Err((error_id, message)) => QueryResponse::error(error_id, message),
        }
    }

    pub(crate) fn handle_banclient(
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
        let requested_ban_time = request
            .named_args
            .get("time")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let ban_reason = request.named_args.get("banreason").cloned().unwrap_or_default();
        if ban_reason.len() > 40 {
            return QueryResponse::error(512, "banreason is too long");
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

        if let Some(response) = self.check_target_client_power(
            &actor_permissions,
            &target_permissions,
            &["i_client_ban_power"],
            &["i_client_needed_ban_power"],
            "i_client_ban_power",
        ) {
            return response;
        }

        let max_ban_time =
            permission_value_or_default(&actor_permissions, &["i_client_ban_max_bantime"]);
        if max_ban_time > 0 && i64::from(requested_ban_time) > max_ban_time {
            return self.insufficient_permission_response("i_client_ban_max_bantime");
        }

        let ban_id = self.register_active_ban(&target_snapshot, requested_ban_time, ban_reason.clone());
        self.remove_session_client(target_snapshot.id, 6, ban_reason);

        let mut row = BTreeMap::new();
        row.insert(String::from("banid"), ban_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_querylist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let current_login = session.authenticated_login.as_deref();
        let current_actor_database_id = session.actor_client_database_id_override;

        let requested_server_id = match request.named_args.get("server_id") {
            Some(value) => match value.parse::<i32>() {
                Ok(server_id) => server_id,
                Err(_) => return QueryResponse::error(512, "server_id must be an integer"),
            },
            None => -1,
        };
        let list_all_servers = requested_server_id < 0;

        let rows = self
            .store
            .query_accounts
            .values()
            .filter(|account| {
                list_all_servers || account.server_id == Some(requested_server_id as u32)
            })
            .map(|account| {
                let mut row = BTreeMap::new();
                row.insert(
                    String::from("client_bounded_server"),
                    account.server_id.unwrap_or(0).to_string(),
                );
                row.insert(
                    String::from("client_login_name"),
                    account.login_name.clone(),
                );
                row.insert(
                    String::from("client_unique_identifier"),
                    self.query_account_unique_identifier(account),
                );
                row.insert(
                    String::from("flag_all"),
                    if list_all_servers {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row.insert(
                    String::from("flag_own"),
                    if current_login.is_some_and(|login| account.login_name == login)
                        || current_actor_database_id
                            .is_some_and(|database_id| {
                                account.client_database_id == Some(database_id)
                            })
                    {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row.insert(String::from("server_id"), requested_server_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_hostinfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("instance_uptime"), String::from("0"));
        row.insert(
            String::from("host_timestamp_utc"),
            current_unix_timestamp().to_string(),
        );
        row.insert(
            String::from("virtualservers_running_total"),
            self.store.virtual_servers.len().to_string(),
        );
        row.insert(
            String::from("virtualservers_online_total"),
            self.store.virtual_servers.len().to_string(),
        );
        row.insert(
            String::from("connection_packets_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_packets_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_sent"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_received"),
            String::from("0"),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_instanceinfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let default_server_port = self
            .store
            .virtual_servers
            .values()
            .min_by_key(|server| server.id)
            .map(|server| server.port)
            .unwrap_or(9987);

        let mut row = BTreeMap::new();
        row.insert(
            String::from("serverinstance_database_version"),
            String::from("11"),
        );
        row.insert(
            String::from("serverinstance_filetransfer_port"),
            String::from("30303"),
        );
        row.insert(
            String::from("serverinstance_template_guest_serverquery_group"),
            if self.store.server_groups.contains_key(&7) {
                String::from("7")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("serverinstance_template_admin_serverquery_group"),
            if self.store.server_groups.contains_key(&6) {
                String::from("6")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("serverinstance_template_serveradmin_group"),
            self.store.server_groups.values().find(|g| g.name == "Server Admin").map(|g| g.id).unwrap_or(0).to_string(),
        );
        row.insert(
            String::from("serverinstance_default_virtualserver_port"),
            default_server_port.to_string(),
        );
        row.insert(
            String::from("serverinstance_query_port"),
            String::from("10101"),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_listfeaturesupport(&self) -> QueryResponse {
        QueryResponse::ok_rows(self.build_feature_rows())
    }

    pub(crate) fn handle_bindinglist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let subsystem = request
            .named_args
            .get("subsystem")
            .map(String::as_str)
            .unwrap_or("voice");
        if !matches!(subsystem, "voice" | "query" | "filetransfer") {
            return QueryResponse::error(512, "unsupported subsystem");
        }

        let rows = ["0.0.0.0", "0::0"]
            .into_iter()
            .map(|ip| {
                let mut row = BTreeMap::new();
                row.insert(String::from("ip"), String::from(ip));
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_propertylist(&self, request: &CommandRequest) -> QueryResponse {
        let include_all = request.flags.is_empty() || request.flags.contains("all");
        let rows = property_catalog()
            .into_iter()
            .filter(|(_, _, property_type)| {
                include_all
                    || (request.flags.contains("server") && *property_type == "SERVER")
                    || (request.flags.contains("channel") && *property_type == "CHANNEL")
                    || (request.flags.contains("client") && *property_type == "CLIENT")
                    || (request.flags.contains("instance") && *property_type == "INSTANCE")
                    || (request.flags.contains("group") && *property_type == "GROUP")
                    || (request.flags.contains("connection") && *property_type == "CONNECTION")
            })
            .map(|(name, flags, property_type)| {
                let mut row = BTreeMap::new();
                row.insert(String::from("flags"), flags.to_string());
                row.insert(String::from("name"), String::from(name));
                row.insert(String::from("type"), String::from(property_type));
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_use(
        &self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let sid = request
            .named_args
            .get("sid")
            .and_then(|value| value.parse::<u32>().ok())
            .or_else(|| {
                request
                    .positional_args
                    .first()
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .or_else(|| {
                request
                    .named_args
                    .get("port")
                    .and_then(|value| value.parse::<u16>().ok())
                    .and_then(|port| {
                        self.store
                            .virtual_servers
                            .values()
                            .find(|server| server.port == port)
                            .map(|server| server.id)
                    })
            });

        match sid.and_then(|server_id| {
            self.store
                .virtual_servers
                .get(&server_id)
                .map(|_| server_id)
        }) {
            Some(server_id) => {
                session.selected_virtual_server_id = Some(server_id);
                session.current_channel_id = self.default_channel_id_for_server(server_id);
                session.virtual_mode = request.flags.contains("virtual");
                QueryResponse::ok()
            }
            None => QueryResponse::error(768, "virtual server not found"),
        }
    }

    pub(crate) fn handle_version(&self) -> QueryResponse {
        let mut row = BTreeMap::new();
        row.insert(
            String::from("version"),
            self.specs.build_version.build_version.clone(),
        );
        row.insert(
            String::from("build"),
            self.specs.build_version.build_index.to_string(),
        );
        row.insert(String::from("platform"), String::from("compat-rust"));
        row.insert(
            String::from("build_name"),
            self.specs.build_version.build_name.clone(),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_whoami(&self, session: &QuerySessionState) -> QueryResponse {
        let mut row = BTreeMap::new();
        let current_account = session
            .authenticated_login
            .as_ref()
            .and_then(|login| self.store.query_accounts.get(login));
        row.insert(
            String::from("client_login_name"),
            session
                .authenticated_login
                .clone()
                .unwrap_or_else(|| String::from("anonymous")),
        );
        row.insert(String::from("clid"), session.client_id.to_string());
        row.insert(
            String::from("virtualserver_id"),
            session
                .selected_virtual_server_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| String::from("0")),
        );
        if let Some(channel_id) = session.current_channel_id {
            row.insert(String::from("client_channel_id"), channel_id.to_string());
        }
        row.insert(
            String::from("virtual"),
            if session.virtual_mode {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("notify_subscription_count"),
            session.notification_subscriptions.len().to_string(),
        );
        if let Some(account) = current_account {
            row.insert(
                String::from("permission_count"),
                self.effective_permissions_for_account(account)
                    .len()
                    .to_string(),
            );
            if !account.server_groups.is_empty() {
                row.insert(
                    String::from("client_servergroups"),
                    account
                        .server_groups
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        } else {
            row.insert(String::from("permission_count"), String::from("0"));
        }
        if let Some(client_database_id) =
            current_account.and_then(|account| account.client_database_id)
        {
            row.insert(
                String::from("client_database_id"),
                client_database_id.to_string(),
            );
        }
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_querycreate(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        if self.store.query_accounts.contains_key(&login_name) {
            return QueryResponse::error(769, "query account already exists");
        }

        let password = request
            .named_args
            .get("client_login_password")
            .cloned()
            .unwrap_or_else(|| format!("generated-{}", login_name));
        let server_id = request
            .named_args
            .get("server_id")
            .and_then(|value| value.parse::<u32>().ok());
        let client_database_id = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| self.allocate_client_database_id());
        let creating_own_identity = client_database_id == actor.client_database_id;
        let required_permission_name = if creating_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_create", "b_client_query_create_own"],
                "b_client_query_create_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_create"],
                "b_client_query_create",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        self.store.query_accounts.insert(
            login_name.clone(),
            QueryAccount {
                login_name: login_name.clone(),
                password: password.clone(),
                server_id,
                client_database_id: Some(client_database_id),
                server_groups: self.default_server_groups_for_new_query_account(),
                permissions: BTreeMap::new(),
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), login_name);
        row.insert(String::from("client_login_password"), password);
        row.insert(String::from("cldbid"), client_database_id.to_string());
        if let Some(server_id) = server_id {
            row.insert(String::from("client_bounded_server"), server_id.to_string());
        }
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_queryrename(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };
        let Some(new_login_name) = request.named_args.get("client_new_login_name").cloned() else {
            return QueryResponse::error(512, "client_new_login_name is required");
        };

        if self.store.query_accounts.contains_key(&new_login_name) {
            return QueryResponse::error(769, "target query account already exists");
        }

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let renaming_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if renaming_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_rename", "b_client_query_rename_own"],
                "b_client_query_rename_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_rename"],
                "b_client_query_rename",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(mut account) = self.store.query_accounts.remove(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        account.login_name = new_login_name.clone();
        self.store
            .query_accounts
            .insert(new_login_name.clone(), account);
        if let Some(snapshot) = self.session_snapshots.remove(&login_name) {
            self.session_snapshots
                .insert(new_login_name.clone(), snapshot);
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), new_login_name.clone());
        if session.authenticated_login.as_deref() == Some(login_name.as_str()) {
            session.authenticated_login = Some(new_login_name.clone());
            row.insert(String::from("renamed_current_login"), String::from("1"));
        }
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_querychangepassword(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let changing_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if changing_own_identity {
            match check_required_permission(
                &actor_permissions,
                &[
                    "b_client_query_change_password",
                    "b_client_query_change_own_password",
                ],
                "b_client_query_change_own_password",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_change_password"],
                "b_client_query_change_password",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(account) = self.store.query_accounts.get_mut(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };

        let next_secret = request
            .named_args
            .get("client_login_password")
            .cloned()
            .unwrap_or_else(|| format!("generated-{}", login_name));
        account.password = next_secret.clone();

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), login_name);
        row.insert(String::from("client_login_password"), next_secret);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_querydelete(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let deleting_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if deleting_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_delete", "b_client_query_delete_own"],
                "b_client_query_delete_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_delete"],
                "b_client_query_delete",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        self.session_snapshots.remove(&login_name);
        match self.store.query_accounts.remove(&login_name) {
            Some(account) => {
                if let Some(client_database_id) = account.client_database_id {
                    self.store
                        .channel_group_assignments
                        .retain(|assignment| assignment.client_database_id != client_database_id);
                    self.store
                        .channel_client_permissions
                        .retain(|target| target.client_database_id != client_database_id);
                }
                QueryResponse::ok()
            }
            None => QueryResponse::error(768, "query account not found"),
        }
    }

}
