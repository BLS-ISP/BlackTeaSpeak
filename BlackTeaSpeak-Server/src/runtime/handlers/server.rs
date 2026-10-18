use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_servernotifyregister(
        &self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        }

        let Some(event_name) = request.named_args.get("event") else {
            return QueryResponse::error(512, "event is required");
        };
        let Some(event) = NotificationEventKind::parse(event_name) else {
            return QueryResponse::error(512, "unsupported notify event");
        };

        let channel_id = request
            .named_args
            .get("id")
            .and_then(|value| value.parse::<u32>().ok())
            .or_else(|| {
                if matches!(
                    event,
                    NotificationEventKind::Channel | NotificationEventKind::TextChannel
                ) {
                    session.current_channel_id
                } else {
                    None
                }
            });
        let subscription = NotificationSubscription {
            event,
            channel_id: if matches!(
                event,
                NotificationEventKind::Channel | NotificationEventKind::TextChannel
            ) {
                channel_id
            } else {
                None
            },
        };

        if !session.notification_subscriptions.contains(&subscription) {
            session.notification_subscriptions.push(subscription);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_servernotifyunregister(&self, session: &mut QuerySessionState) -> QueryResponse {
        session.notification_subscriptions.clear();
        QueryResponse::ok()
    }

    pub(crate) fn handle_serverrequestconnectioninfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server) = self.selected_server(session) else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("virtualserver_id"), server.id.to_string());
        row.insert(String::from("virtualserver_port"), server.port.to_string());
        row.insert(
            String::from("virtualserver_clientsonline"),
            self.client_count_in_server(server.id).to_string(),
        );
        row.insert(
            String::from("virtualserver_queryclientsonline"),
            self.store
                .online_clients
                .values()
                .filter(|client| client.server_id == server.id && client.client_type == 1)
                .count()
                .to_string(),
        );
        row.insert(String::from("connection_connected_time"), String::from("0"));
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

    pub(crate) fn handle_serveridgetbyport(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(port) = request
            .named_args
            .get("virtualserver_port")
            .and_then(|value| value.parse::<u16>().ok())
        else {
            return QueryResponse::error(512, "virtualserver_port is required");
        };
        let Some(server) = self
            .store
            .virtual_servers
            .values()
            .find(|server| server.port == port)
        else {
            return QueryResponse::error(768, "virtual server not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("server_id"), server.id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_serverlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let only_offline = request.flags.contains("onlyoffline");
        let rows = self
            .store
            .virtual_servers
            .values()
            .filter(|server| !only_offline || server.status == "offline")
            .map(|server| {
                let effective_status = if server.status == "offline"
                    && session.virtual_mode
                    && session.selected_virtual_server_id == Some(server.id)
                {
                    String::from("virtual online")
                } else if server.status.is_empty() {
                    String::from("online")
                } else {
                    server.status.clone()
                };
                let mut row = BTreeMap::new();
                row.insert(String::from("virtualserver_id"), server.id.to_string());
                row.insert(String::from("virtualserver_port"), server.port.to_string());
                row.insert(String::from("virtualserver_status"), effective_status);
                row.insert(
                    String::from("virtualserver_clientsonline"),
                    self.client_count_in_server(server.id).to_string(),
                );
                row.insert(
                    String::from("virtualserver_maxclients"),
                    server.max_clients.to_string(),
                );
                row.insert(String::from("virtualserver_name"), server.name.clone());
                if request.flags.contains("uid") || !request.flags.contains("short") {
                    row.insert(
                        String::from("virtualserver_unique_identifier"),
                        server.unique_identifier.clone(),
                    );
                }
                row
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_serverinfo(&self, session: &QuerySessionState) -> QueryResponse {
        let server = match self.selected_server(session) {
            Some(server) => server,
            None => return QueryResponse::error(522, "virtual server selection required"),
        };

        let effective_status = if server.status == "offline" && session.virtual_mode {
            String::from("virtual online")
        } else if server.status.is_empty() {
            String::from("online")
        } else {
            server.status.clone()
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("virtualserver_id"), server.id.to_string());
        row.insert(String::from("virtualserver_port"), server.port.to_string());
        row.insert(String::from("virtualserver_name"), server.name.clone());
        row.insert(String::from("virtualserver_status"), effective_status);
        row.insert(
            String::from("virtualserver_unique_identifier"),
            server.unique_identifier.clone(),
        );
        row.insert(
            String::from("virtualserver_welcomemessage"),
            server.welcome_message.clone(),
        );
        row.insert(
            String::from("virtualserver_hostmessage"),
            server.host_message.clone(),
        );
        row.insert(
            String::from("virtualserver_hostmessage_mode"),
            server.host_message_mode.to_string(),
        );
        row.insert(
            String::from("virtualserver_ask_for_privilegekey"),
            server.ask_for_privilegekey.to_string(),
        );
        row.insert(
            String::from("virtualserver_maxclients"),
            server.max_clients.to_string(),
        );
        row.insert(
            String::from("virtualserver_clientsonline"),
            self.client_count_in_server(server.id).to_string(),
        );
        row.insert(
            String::from("virtualserver_channelsonline"),
            self.store
                .channels
                .get(&server.id)
                .map(|c| c.len())
                .unwrap_or(0)
                .to_string(),
        );
        row.insert(
            String::from("virtualserver_antiflood_points_tick_reduce"),
            server.antiflood_points_tick_reduce.to_string(),
        );
        row.insert(
            String::from("virtualserver_antiflood_points_needed_command_block"),
            server.antiflood_points_needed_command_block.to_string(),
        );
        row.insert(
            String::from("virtualserver_antiflood_points_needed_ip_block"),
            server.antiflood_points_needed_ip_block.to_string(),
        );
        row.insert(
            String::from("virtualserver_antiflood_ban_time"),
            server.antiflood_ban_time.to_string(),
        );
        row.insert(
            String::from("virtualserver_flag_password"),
            if server.password.is_empty() { "0" } else { "1" }.to_string(),
        );
        row.insert(
            String::from("virtualserver_password"),
            server.password.clone(),
        );
        let autoban_count = self
            .store
            .instance_properties
            .get(&format!(
                "vs:{}:virtualserver_complain_autoban_count",
                server.id
            ))
            .cloned()
            .unwrap_or_else(|| String::from("5"));
        let autoban_time = self
            .store
            .instance_properties
            .get(&format!(
                "vs:{}:virtualserver_complain_autoban_time",
                server.id
            ))
            .cloned()
            .unwrap_or_else(|| String::from("1200"));
        let complain_remove_time = self
            .store
            .instance_properties
            .get(&format!(
                "vs:{}:virtualserver_complain_remove_time",
                server.id
            ))
            .cloned()
            .unwrap_or_else(|| String::from("3600"));
        row.insert(
            String::from("virtualserver_complain_autoban_count"),
            autoban_count,
        );
        row.insert(
            String::from("virtualserver_complain_autoban_time"),
            autoban_time,
        );
        row.insert(
            String::from("virtualserver_complain_remove_time"),
            complain_remove_time,
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_serveredit(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if request.named_args.is_empty() {
            return QueryResponse::error(512, "at least one server property is required");
        }

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if request.named_args.contains_key("virtualserver_name")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_name"],
                "b_virtualserver_modify_name",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("virtualserver_password")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &[
                    "b_virtualserver_modify_password",
                    "b_virtualserver_modify_name",
                ],
                "b_virtualserver_modify_password",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request
            .named_args
            .contains_key("virtualserver_welcomemessage")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_welcomemessage"],
                "b_virtualserver_modify_welcomemessage",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if (request.named_args.contains_key("virtualserver_hostmessage")
            || request
                .named_args
                .contains_key("virtualserver_hostmessage_mode"))
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_hostmessage"],
                "b_virtualserver_modify_hostmessage",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("virtualserver_maxclients")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_maxclients"],
                "b_virtualserver_modify_maxclients",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request
            .named_args
            .keys()
            .any(|key| key.starts_with("virtualserver_antiflood_"))
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_antiflood"],
                "b_virtualserver_modify_antiflood",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request
            .named_args
            .contains_key("virtualserver_ask_for_privilegekey")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_name"],
                "b_virtualserver_modify_name",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(server) = self.selected_server_mut(session) else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let mut applied = false;

        if let Some(server_name) = request.named_args.get("virtualserver_name") {
            server.name = server_name.clone();
            applied = true;
        }
        if let Some(password) = request.named_args.get("virtualserver_password") {
            server.password = password.clone();
            applied = true;
        }
        if let Some(welcome_message) = request.named_args.get("virtualserver_welcomemessage") {
            server.welcome_message = welcome_message.clone();
            applied = true;
        }
        if let Some(host_message) = request.named_args.get("virtualserver_hostmessage") {
            server.host_message = host_message.clone();
            applied = true;
        }
        if let Some(host_message_mode) = request.named_args.get("virtualserver_hostmessage_mode") {
            let Ok(host_message_mode) = host_message_mode.parse::<u32>() else {
                return QueryResponse::error(512, "virtualserver_hostmessage_mode must be numeric");
            };
            server.host_message_mode = host_message_mode;
            applied = true;
        }
        if let Some(ask_for_privilegekey) =
            request.named_args.get("virtualserver_ask_for_privilegekey")
        {
            let Some(ask_for_privilegekey) = parse_query_bool(ask_for_privilegekey) else {
                return QueryResponse::error(
                    512,
                    "virtualserver_ask_for_privilegekey must be 0 or 1",
                );
            };
            server.ask_for_privilegekey = if ask_for_privilegekey { 1 } else { 0 };
            applied = true;
        }
        if let Some(max_clients) = request.named_args.get("virtualserver_maxclients") {
            let Ok(max_clients) = max_clients.parse::<u32>() else {
                return QueryResponse::error(512, "virtualserver_maxclients must be numeric");
            };
            server.max_clients = max_clients;
            applied = true;
        }
        if let Some(value) = request
            .named_args
            .get("virtualserver_antiflood_points_tick_reduce")
        {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_points_tick_reduce must be numeric",
                );
            };
            server.antiflood_points_tick_reduce = value;
            applied = true;
        }
        if let Some(value) = request
            .named_args
            .get("virtualserver_antiflood_points_needed_command_block")
        {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_points_needed_command_block must be numeric",
                );
            };
            server.antiflood_points_needed_command_block = value;
            applied = true;
        }
        if let Some(value) = request
            .named_args
            .get("virtualserver_antiflood_points_needed_ip_block")
        {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_points_needed_ip_block must be numeric",
                );
            };
            server.antiflood_points_needed_ip_block = value;
            applied = true;
        }
        if let Some(value) = request.named_args.get("virtualserver_antiflood_ban_time") {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_ban_time must be numeric",
                );
            };
            server.antiflood_ban_time = value;
            applied = true;
        }
        let server_id = server.id;

        for key in [
            "virtualserver_complain_autoban_count",
            "virtualserver_complain_autoban_time",
            "virtualserver_complain_remove_time",
        ] {
            if let Some(value) = request.named_args.get(key) {
                let Ok(parsed) = value.parse::<u32>() else {
                    return QueryResponse::error(512, format!("{key} must be numeric"));
                };
                self.store
                    .instance_properties
                    .insert(format!("vs:{server_id}:{key}"), parsed.to_string());
                applied = true;
            }
        }

        if !applied {
            return QueryResponse::error(512, "no supported server properties provided");
        }

        if let Some(server) = self.selected_server(session).cloned() {
            let _ = self.db.save_virtual_server(&server);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_servertemppasswordadd(
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
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[
                "b_virtualserver_modify_temporary_passwords",
                "b_virtualserver_modify_password",
                "b_virtualserver_modify_name",
            ],
            "b_virtualserver_modify_temporary_passwords",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(pw) = request
            .named_args
            .get("pw")
            .map(String::as_str)
            .filter(|pw| !pw.is_empty())
        else {
            return QueryResponse::error(512, "pw is required");
        };
        let Some(duration_str) = request.named_args.get("duration") else {
            return QueryResponse::error(512, "duration is required");
        };
        let Ok(duration) = duration_str.parse::<u64>() else {
            return QueryResponse::error(512, "duration must be a positive integer");
        };
        if duration == 0 {
            return QueryResponse::error(512, "duration must be greater than 0");
        }

        let desc = request
            .named_args
            .get("desc")
            .cloned()
            .unwrap_or_default();
        let tcid = match request.named_args.get("tcid") {
            Some(tcid_str) => {
                let Ok(tcid) = tcid_str.parse::<u32>() else {
                    return QueryResponse::error(512, "tcid must be numeric");
                };
                if tcid != 0 && !self.channel_exists(server_id, tcid) {
                    return QueryResponse::error(768, "invalid channelID");
                }
                tcid
            }
            None => 0,
        };
        let tcpw = request
            .named_args
            .get("tcpw")
            .cloned()
            .unwrap_or_default();

        let (invoker_uid, _, invoker_name) = self
            .lookup_client_identity_by_dbid(server_id, actor.client_database_id)
            .unwrap_or_else(|| {
                let fallback_name = session.effective_nickname();
                (String::from("serveradmin"), actor.client_database_id, fallback_name)
            });

        let now = current_unix_timestamp();
        self.prune_expired_temp_passwords();
        self.store
            .server_temp_passwords
            .retain(|entry| !(entry.server_id == server_id && entry.password == pw));
        self.store.server_temp_passwords.push(ServerTempPassword {
            server_id,
            password: pw.to_string(),
            password_clear: pw.to_string(),
            description: desc,
            start_timestamp: now,
            end_timestamp: now.saturating_add(duration),
            target_channel_id: tcid,
            target_channel_password: tcpw,
            invoker_name,
            invoker_unique_id: invoker_uid,
        });

        QueryResponse::ok()
    }

    pub(crate) fn handle_servertemppassworddel(
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
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[
                "b_virtualserver_modify_temporary_passwords",
                "b_virtualserver_modify_password",
                "b_virtualserver_modify_name",
            ],
            "b_virtualserver_modify_temporary_passwords",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(pw) = request
            .named_args
            .get("pw")
            .map(String::as_str)
            .filter(|pw| !pw.is_empty())
        else {
            return QueryResponse::error(512, "pw is required");
        };

        self.prune_expired_temp_passwords();
        let before = self.store.server_temp_passwords.len();
        self.store
            .server_temp_passwords
            .retain(|entry| !(entry.server_id == server_id && entry.password == pw));
        if self.store.server_temp_passwords.len() == before {
            return QueryResponse::error(1281, "database empty result set");
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_servertemppasswordlist(
        &mut self,
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
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[
                "b_virtualserver_modify_temporary_passwords",
                "b_virtualserver_modify_password",
                "b_virtualserver_modify_name",
            ],
            "b_virtualserver_modify_temporary_passwords",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        self.prune_expired_temp_passwords();
        let rows = self
            .store
            .server_temp_passwords
            .iter()
            .filter(|entry| entry.server_id == server_id)
            .map(|entry| {
                let mut row = BTreeMap::new();
                row.insert(String::from("nickname"), entry.invoker_name.clone());
                row.insert(String::from("uid"), entry.invoker_unique_id.clone());
                row.insert(String::from("desc"), entry.description.clone());
                row.insert(String::from("pw_clear"), entry.password_clear.clone());
                row.insert(String::from("start"), entry.start_timestamp.to_string());
                row.insert(String::from("end"), entry.end_timestamp.to_string());
                row.insert(String::from("tcid"), entry.target_channel_id.to_string());
                row.insert(String::from("pw"), entry.password.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub fn verify_server_password(
        &mut self,
        server_id: u32,
        provided_password: &str,
        client_database_id: Option<u64>,
    ) -> Result<Option<(u32, String)>, ()> {
        self.prune_expired_temp_passwords();
        if let Some(temp) = self
            .store
            .server_temp_passwords
            .iter()
            .find(|entry| {
                entry.server_id == server_id
                    && !provided_password.is_empty()
                    && (entry.password == provided_password
                        || entry.password_clear == provided_password)
            })
            .cloned()
        {
            return Ok((temp.target_channel_id != 0)
                .then_some((temp.target_channel_id, temp.target_channel_password)));
        }

        let Some(server) = self.store.virtual_servers.get(&server_id) else {
            return Ok(None);
        };
        if server.password.is_empty() || server.password == provided_password {
            return Ok(None);
        }

        if let Some(cldbid) = client_database_id {
            let default_cid = self.default_channel_id_for_server(server_id).unwrap_or(1);
            if let Some(perms) = self.effective_permissions_for_client(server_id, default_cid, cldbid)
                && check_required_permission(
                    &perms,
                    &["b_virtualserver_join_ignore_password"],
                    "b_virtualserver_join_ignore_password",
                )
                .is_ok()
            {
                return Ok(None);
            }
        }

        Err(())
    }

    pub(crate) fn handle_servergetvariables(
        &self,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        match self.web_server_variables_row(server_id) {
            Some(row) => QueryResponse::ok_row(row),
            None => QueryResponse::error(1024, "invalid serverID"),
        }
    }

}

