use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use std::collections::BTreeMap;

pub struct DesktopSessionHandler {
    pub login_phase: u8, // 0 = AwaitClientInit, 1 = Connected
    pub unique_identifier: String,
    pub session: crate::runtime::QuerySessionState,
}

impl DesktopSessionHandler {
    pub fn new(client_id: u64, connection_ip: String) -> Self {
        Self {
            login_phase: 0,
            unique_identifier: format!("desktop-{}", client_id),
            session: crate::runtime::QuerySessionState {
                client_id,
                connection_ip,
                authenticated_login: None,
                selected_virtual_server_id: None,
                current_channel_id: None,
                virtual_mode: false,
                notification_subscriptions: vec![],
                actor_client_database_id_override: Some(client_id + 1000),
                client_nickname: String::from("DesktopUser"),
                client_away: false,
                client_away_message: String::new(),
                client_input_muted: false,
                client_output_muted: false,
                is_desktop_client: true,
                whisper_targets: None,
                ignored_clients: Vec::new(),
                points: 0,
                blocked_until_millis: 0,
                last_points_decay_millis: 0,
            },
        }
    }

    pub fn handle_command(
        &mut self,
        command_str: &str,
        runtime: &mut BaselineRuntime,
    ) -> (Vec<String>, Vec<crate::transport::TransportNotification>) {
        let request = match crate::query::parse_request_line(command_str) {
            Ok(req) => req,
            Err(_) => return (vec![String::from("error id=1536 msg=invalid\\scommand")], vec![]),
        };

        // Anti-Flood implementation
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        
        // Decay points: 5 points every 1 second
        if self.session.last_points_decay_millis == 0 {
            self.session.last_points_decay_millis = now;
        } else {
            let diff = now.saturating_sub(self.session.last_points_decay_millis);
            let decay = (diff / 1000) * 5;
            if decay > 0 {
                self.session.points = self.session.points.saturating_sub(decay as u32);
                self.session.last_points_decay_millis = now;
            }
        }

        if self.session.blocked_until_millis > now {
            return (vec![String::from("error id=3329 msg=connection\\sfailed,\\syou\\sare\\sbanned (flood)")], vec![]);
        }

        let cmd_points = 10; // Simple penalty per command
        self.session.points = self.session.points.saturating_add(cmd_points);
        
        let flood_limit = 150; 
        if self.session.points > flood_limit {
            self.session.blocked_until_millis = now + 5000; // 5 second block
            return (vec![String::from("error id=3329 msg=connection\\sfailed,\\syou\\sare\\sbanned (flood)")], vec![]);
        }

        if self.login_phase == 0 {
            if request.command != "clientinit" {
                return (vec![String::from("error id=3329 msg=connection\\sfailed,\\syou\\sare\\snot\\sconnected")], vec![]);
            }
            return self.handle_client_init(&request, runtime);
        }

        // If connected, handle common client commands or forward to runtime (ServerQuery)
        match request.command.as_str() {
            "whoami" => {
                let mut row = BTreeMap::new();
                row.insert("client_id".to_string(), self.session.client_id.to_string());
                if let Some(cid) = self.session.current_channel_id {
                    row.insert("client_channel_id".to_string(), cid.to_string());
                }
                let resp = QueryResponse::ok_row(row);
                (vec![crate::query::render_response(&resp)], vec![])
            }
            "channellist" | "clientlist" => {
                if let Some(_server_id) = self.session.selected_virtual_server_id {
                    let session_clone = self.session.clone();
                    let (resp, notifs) = crate::transport::execute_request_with_notifications(runtime, &request, &session_clone, &mut self.session);
                    
                    let mut out = Vec::new();
                    let raw_resp = crate::query::render_response(&resp);
                    if !resp.rows.is_empty() {
                        out.push(format!("{} {}", request.command, raw_resp));
                    } else {
                        out.push(raw_resp);
                    }
                    (out, notifs)
                } else {
                    (vec![String::from("error id=1024 msg=invalid\\sserverID")], vec![])
                }
            }
            "desktopwhisperset" => {
                let mut targets = crate::models::WhisperTargetSelection::default();
                if let Some(target) = request.named_args.get("target") {
                    match target.as_str() {
                        "channel" => {
                            if let Some(tid) = request.named_args.get("target_id") {
                                if let Ok(cid) = tid.parse::<u32>() {
                                    targets.channel_ids.insert(cid);
                                }
                            }
                        }
                        "client" => {
                            if let Some(tid) = request.named_args.get("target_id") {
                                for client_id_str in tid.split(',') {
                                    if let Ok(cid) = client_id_str.parse::<u64>() {
                                        if cid > 0 {
                                            targets.client_ids.insert(cid);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                self.session.whisper_targets = Some(targets);
                (vec![String::from("error id=0 msg=ok")], vec![])
            }
            _ => {
                if let Some(_server_id) = self.session.selected_virtual_server_id {
                    let session_clone = self.session.clone();
                    let (resp, notifs) = crate::transport::execute_request_with_notifications(runtime, &request, &session_clone, &mut self.session);
                    (vec![crate::query::render_response(&resp)], notifs)
                } else {
                    (vec![String::from("error id=256 msg=command\\snot\\sfound")], vec![])
                }
            }
        }
    }

    fn handle_client_init(&mut self, request: &CommandRequest, runtime: &mut BaselineRuntime) -> (Vec<String>, Vec<crate::transport::TransportNotification>) {
        let row = if !request.named_args.is_empty() {
            &request.named_args
        } else if let Some(first_group) = request.option_groups.first() {
            first_group
        } else {
            return (vec![String::from("error id=1538 msg=invalid\\sparameter")], vec![]);
        };

        let server_info = match runtime.web_server_init_info() {
            Some(info) => info,
            None => return (vec![String::from("error id=1024 msg=invalid\\sserverID")], vec![]),
        };

        if let Some(nick) = row.get("client_nickname") {
            self.session.client_nickname = nick.clone();
        }
        
        // Setup client in runtime
        let database_id = self.session.client_id + 1000;
        self.session.selected_virtual_server_id = Some(server_info.server_id);
        self.session.current_channel_id = runtime.web_default_channel_id(server_info.server_id).or(Some(1));

        runtime.upsert_web_client(
            self.session.client_id,
            server_info.server_id,
            self.session.current_channel_id.unwrap(),
            self.session.client_nickname.clone(),
            self.unique_identifier.clone(),
            database_id,
            "BlackTeaSpeak Desktop".to_string(),
            "desktop".to_string(),
            self.session.connection_ip.clone(),
        );

        self.login_phase = 1;

        // Build the TS3-style initserver response string
        // We do this manually or via BTreeMap
        let mut initserver_row = BTreeMap::new();
        initserver_row.insert("virtualserver_id".to_string(), server_info.server_id.to_string());
        initserver_row.insert("virtualserver_name".to_string(), server_info.server_name.clone());
        initserver_row.insert("virtualserver_welcomemessage".to_string(), server_info.welcome_message.clone());
        
        let mut out = vec![];
        
        let resp = QueryResponse::ok_row(initserver_row);
        let rendered_initserver = format!("initserver {}", crate::query::render_response(&resp)); 
        out.push(rendered_initserver); // Note: TS3 client expects `initserver virtualserver_id=...` wait `render_response` adds `error id=0 msg=ok`. 
        
        // Actually, normal TS3 command pushes (like initserver) are just plain formatted:
        out.push(format!(
            "initserver virtualserver_id={} virtualserver_name={} virtualserver_welcomemessage={} client_id={}",
            server_info.server_id,
            crate::query::encode_query_value(&server_info.server_name),
            crate::query::encode_query_value(&server_info.welcome_message),
            self.session.client_id
        ));

        out.push(String::from("error id=0 msg=ok"));
        
        let notifs = vec![crate::transport::TransportNotification::ClientEnterView {
            presence: crate::transport::SessionPresence {
                client_id: self.session.client_id,
                login_name: self.session.client_nickname.clone(),
                unique_identifier: self.unique_identifier.clone(),
                client_type: 0,
                server_id: server_info.server_id,
                channel_id: self.session.current_channel_id.unwrap(),
            },
            from_channel_id: None,
            reason_id: 0,
        }];
        
        (out, notifs)
    }
}
