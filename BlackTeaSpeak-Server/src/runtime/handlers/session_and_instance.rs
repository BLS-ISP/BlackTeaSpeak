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

    pub(crate) fn handle_hostinfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let running_servers = self
            .store
            .virtual_servers
            .values()
            .filter(|server| server.status != "offline")
            .count();

        let mut row = BTreeMap::new();
        row.insert(String::from("instance_uptime"), String::from("0"));
        row.insert(
            String::from("host_timestamp_utc"),
            current_unix_timestamp().to_string(),
        );
        row.insert(
            String::from("virtualservers_running_total"),
            running_servers.to_string(),
        );
        row.insert(
            String::from("virtualservers_online_total"),
            running_servers.to_string(),
        );
        row.insert(
            String::from("virtualservers_total_maxclients"),
            self.store
                .virtual_servers
                .values()
                .filter(|s| s.status != "offline")
                .map(|s| s.max_clients)
                .sum::<u32>()
                .to_string(),
        );
        row.insert(
            String::from("virtualservers_total_clients_online"),
            self.store
                .online_clients
                .values()
                .filter(|c| c.client_type == 0)
                .count()
                .to_string(),
        );
        row.insert(
            String::from("virtualservers_total_channels_online"),
            self.store
                .channels
                .iter()
                .filter(|(sid, _)| {
                    self.store
                        .virtual_servers
                        .get(sid)
                        .is_some_and(|s| s.status != "offline")
                })
                .map(|(_, chs)| chs.len())
                .sum::<usize>()
                .to_string(),
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
            String::from("serverinstance_max_download_total_bandwidth"),
            String::from("18446744073709551615"),
        );
        row.insert(
            String::from("serverinstance_max_upload_total_bandwidth"),
            String::from("18446744073709551615"),
        );
        row.insert(
            String::from("serverinstance_serverquery_flood_commands"),
            String::from("10"),
        );
        row.insert(
            String::from("serverinstance_serverquery_flood_time"),
            String::from("3"),
        );
        row.insert(
            String::from("serverinstance_serverquery_ban_time"),
            String::from("600"),
        );
        row.insert(
            String::from("serverinstance_serverquery_max_connections_per_ip"),
            String::from("2"),
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
            self.store
                .server_groups
                .values()
                .find(|g| g.name == "Server Admin")
                .map(|g| g.id)
                .unwrap_or(0)
                .to_string(),
        );
        row.insert(
            String::from("serverinstance_default_virtualserver_port"),
            default_server_port.to_string(),
        );
        row.insert(
            String::from("serverinstance_query_port"),
            String::from("10101"),
        );
        row.insert(
            String::from("serverinstance_permissions_version"),
            String::from("19"),
        );
        row.insert(
            String::from("serverinstance_pending_connections_per_ip"),
            String::from("0"),
        );
        for (key, value) in &self.store.instance_properties {
            row.insert(key.clone(), value.clone());
        }
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
                .map(|server| (server_id, server.status.as_str()))
        }) {
            Some((_server_id, "offline")) if !request.flags.contains("virtual") => {
                QueryResponse::error(1033, "server is not running")
            }
            Some((server_id, _)) => {
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

    pub(crate) fn handle_instanceedit(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[
                "b_serverinstance_modify_settings",
                "b_serverinstance_modify_templates",
            ],
            "b_serverinstance_modify_settings",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let mut applied = false;
        for (key, value) in &request.named_args {
            if key.starts_with("serverinstance_") {
                self.store
                    .instance_properties
                    .insert(key.clone(), value.clone());
                applied = true;
            }
        }
        if !applied {
            return QueryResponse::error(512, "at least one serverinstance property is required");
        }

        self.append_server_log(0, 4, "ServerLibPriv", "instance settings updated");
        QueryResponse::ok()
    }

    pub(crate) fn handle_gm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_serverinstance_textmessage_send"],
            "b_serverinstance_textmessage_send",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(msg) = request
            .named_args
            .get("msg")
            .cloned()
            .filter(|s| !s.trim().is_empty())
        else {
            return QueryResponse::error(512, "msg is required");
        };

        let sender = self.query_session_participant(session);
        let server_ids = self
            .store
            .virtual_servers
            .values()
            .filter(|server| server.status != "offline")
            .map(|server| server.id)
            .collect::<Vec<_>>();
        for sid in server_ids {
            self.record_text_message(
                &TextMessageTarget {
                    target_mode: 3,
                    server_id: sid,
                    channel_id: None,
                    target_client_id: None,
                    message: msg.clone(),
                },
                sender.database_id,
                sender.unique_identifier.clone(),
                sender.nickname.clone(),
            );
        }

        self.append_server_log(0, 4, "Query", format!("global message sent: {msg}"));
        QueryResponse::ok()
    }

}
