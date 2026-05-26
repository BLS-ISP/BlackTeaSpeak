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

        let rows = self
            .store
            .virtual_servers
            .values()
            .map(|server| {
                let mut row = BTreeMap::new();
                row.insert(String::from("virtualserver_id"), server.id.to_string());
                row.insert(String::from("virtualserver_port"), server.port.to_string());
                row.insert(String::from("virtualserver_status"), String::from("online"));
                row.insert(
                    String::from("virtualserver_clientsonline"),
                    self.client_count_in_server(server.id).to_string(),
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

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_serverinfo(&self, session: &QuerySessionState) -> QueryResponse {
        let server = match self.selected_server(session) {
            Some(server) => server,
            None => return QueryResponse::error(522, "virtual server selection required"),
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("virtualserver_id"), server.id.to_string());
        row.insert(String::from("virtualserver_port"), server.port.to_string());
        row.insert(String::from("virtualserver_name"), server.name.clone());
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

        if !applied {
            return QueryResponse::error(512, "no supported server properties provided");
        }

        QueryResponse::ok()
    }

}
