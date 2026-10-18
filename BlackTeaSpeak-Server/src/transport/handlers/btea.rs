use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use std::collections::BTreeMap;

pub struct BteaSessionHandler {
    pub login_phase: u8, // 0 = AwaitClientInit, 1 = Connected
    pub unique_identifier: String,
    pub session: crate::runtime::QuerySessionState,
}

impl BteaSessionHandler {
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
                subscribed_channels: std::collections::BTreeSet::new(),
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

        let mut request = request;
        let return_code = request.named_args.remove("return_code").or_else(|| {
            request
                .positional_args
                .iter()
                .find(|arg| arg.starts_with("rt-") && arg.len() > 3)
                .cloned()
        });
        request.positional_args.retain(|arg| !arg.starts_with("rt-"));
        for group in &mut request.option_groups {
            group.remove("return_code");
        }
        request.option_groups.retain(|g| !g.is_empty());

        // If connected, handle common client commands or forward to runtime (ServerQuery)
        match request.command.as_str() {
            "whoami" => {
                let mut row = BTreeMap::new();
                row.insert("client_id".to_string(), self.session.client_id.to_string());
                if let Some(cid) = self.session.current_channel_id {
                    row.insert("client_channel_id".to_string(), cid.to_string());
                }
                if let Some(dbid) = self.session.actor_client_database_id_override {
                    row.insert("client_database_id".to_string(), dbid.to_string());
                }
                let mut resp = QueryResponse::ok_row(row);
                if let Some(rc) = return_code {
                    resp.extra_fields.insert("return_code".to_string(), rc);
                }
                (vec![crate::query::render_response(&resp)], vec![])
            }
            "channellist" | "clientlist" => {
                if let Some(_server_id) = self.session.selected_virtual_server_id {
                    let session_clone = self.session.clone();
                    let (mut resp, notifs) = crate::transport::execute_request_with_notifications(runtime, &request, &session_clone, &mut self.session);
                    if let Some(rc) = return_code {
                        resp.extra_fields.insert("return_code".to_string(), rc);
                    }
                    
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
                if let Some(online) = runtime.store.online_clients.get_mut(&self.session.client_id) {
                    online.whisper_targets = Some(targets.clone());
                }
                self.session.whisper_targets = Some(targets);
                (vec![String::from("error id=0 msg=ok")], vec![])
            }
            "desktopgetmyperms" => {
                let mut resp = runtime.handle_desktopgetmyperms(&self.session);
                if let Some(rc) = return_code {
                    resp.extra_fields.insert("return_code".to_string(), rc);
                }
                let raw_resp = crate::query::render_response(&resp);
                if !resp.rows.is_empty() {
                    (vec![format!("desktopgetmyperms {}", raw_resp)], vec![])
                } else {
                    (vec![raw_resp], vec![])
                }
            }
            _ => {
                if let Some(server_id) = self.session.selected_virtual_server_id {
                    let session_clone = self.session.clone();
                    let (mut resp, notifs) = crate::transport::execute_request_with_notifications(runtime, &request, &session_clone, &mut self.session);
                    if let Some(rc) = return_code {
                        resp.extra_fields.insert("return_code".to_string(), rc);
                    }
                    let raw_resp = crate::query::render_response(&resp);
                    let mut out = Vec::new();
                    if !resp.rows.is_empty() {
                        out.push(format!("{} {}", request.command, raw_resp));
                    } else {
                        out.push(raw_resp);
                    }
                    let refresh_needed = resp.error_id == 0
                        && matches!(
                            request.command.as_str(),
                            "tokenuse"
                                | "privilegekeyuse"
                                | "servergroupaddclient"
                                | "servergroupdelclient"
                                | "setclientchannelgroup"
                                | "servergroupaddperm"
                                | "servergroupdelperm"
                                | "channelgroupaddperm"
                                | "channelgroupdelperm"
                                | "clientaddperm"
                                | "clientdelperm"
                                | "channeladdperm"
                                | "channeldelperm"
                                | "channelclientaddperm"
                                | "channelclientdelperm"
                                | "permreset"
                        );
                    if refresh_needed {
                        let perms_resp = runtime.handle_desktopgetmyperms(&self.session);
                        if !perms_resp.rows.is_empty() {
                            out.push(format!(
                                "desktopgetmyperms {}",
                                crate::query::render_response(&perms_resp)
                            ));
                        }
                        let current_cid = self.session.current_channel_id.unwrap_or(1);
                        let dbid = self
                            .session
                            .actor_client_database_id_override
                            .unwrap_or(self.session.client_id + 1000);
                        if let Some(needed_rows) =
                            runtime.web_client_needed_permission_rows(server_id, current_cid, dbid)
                        {
                            let rendered = needed_rows
                                .into_iter()
                                .map(|row| {
                                    row.into_iter()
                                        .map(|(k, v)| {
                                            format!("{}={}", k, crate::query::encode_query_value(&v))
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .collect::<Vec<_>>()
                                .join("|");
                            out.push(format!("notifyclientneededpermissions {}", rendered));
                        }
                    }
                    (out, notifs)
                } else {
                    (vec![String::from("error id=256 msg=command\\snot\\sfound")], vec![])
                }
            }
        }
    }

    fn handle_client_init(&mut self, request: &CommandRequest, runtime: &mut BaselineRuntime) -> (Vec<String>, Vec<crate::transport::TransportNotification>) {
        let bootstrap = match crate::transport::handlers::perform_client_bootstrap(
            runtime, &mut self.session, request, &self.unique_identifier, false
        ) {
            Ok(data) => data,
            Err(err) if err == "invalid server password" => {
                return (vec![String::from("error id=781 msg=invalid\\sserver\\spassword")], vec![]);
            }
            Err(_) => return (vec![String::from("error id=1024 msg=invalid\\sserverID")], vec![]),
        };

        self.login_phase = 1;

        let server_name_escaped = crate::query::encode_query_value(&bootstrap.server_name);
        let welcome_msg_escaped = crate::query::encode_query_value(&bootstrap.welcome_message);
        let server_uid_escaped = crate::query::encode_query_value(&self.unique_identifier);
        let client_nickname_escaped = crate::query::encode_query_value(&bootstrap.client_nickname);
        
        let initserver_str = format!("initserver virtualserver_name={} virtualserver_welcomemessage={} virtualserver_maxclients=32 virtualserver_password= virtualserver_clientsonline=1 virtualserver_channelsonline=1 virtualserver_created=1494921612 virtualserver_uptime=33245 virtualserver_hostmessage= virtualserver_hostmessage_mode=0 virtualserver_filebase=files\\/virtualserver_1 virtualserver_default_server_group=8 virtualserver_default_channel_group=8 virtualserver_flag_password=0 virtualserver_default_channel_admin_group=5 virtualserver_max_download_total_bandwidth=-1 virtualserver_max_upload_total_bandwidth=-1 virtualserver_hostbanner_url= virtualserver_hostbanner_gfx_url= virtualserver_hostbanner_gfx_interval=0 virtualserver_complain_autoban_count=5 virtualserver_complain_autoban_time=1200 virtualserver_complain_remove_time=3600 virtualserver_min_clients_in_channel_before_forced_silence=100 virtualserver_priority_speaker_dimm_modificator=-18.0000 virtualserver_id=1 virtualserver_antiflood_points_tick_reduce=5 virtualserver_antiflood_points_needed_command_block=150 virtualserver_antiflood_points_needed_ip_block=250 virtualserver_client_connections=1 virtualserver_query_client_connections=0 virtualserver_hostbutton_tooltip= virtualserver_hostbutton_url= virtualserver_hostbutton_gfx_url= virtualserver_queryclientsonline=0 virtualserver_download_quota=-1 virtualserver_upload_quota=-1 virtualserver_month_bytes_downloaded=0 virtualserver_month_bytes_uploaded=0 virtualserver_total_bytes_downloaded=0 virtualserver_total_bytes_uploaded=0 virtualserver_port=9987 virtualserver_autostart=1 virtualserver_machine_id= virtualserver_needed_identity_security_level=8 virtualserver_log_client=0 virtualserver_log_query=0 virtualserver_log_channel=0 virtualserver_log_permissions=1 virtualserver_log_server=0 virtualserver_log_filetransfer=0 virtualserver_min_client_version=1481105459 virtualserver_name_phonetic= virtualserver_icon_id=0 virtualserver_reserved_slots=0 virtualserver_total_packetloss_speech=0.0000 virtualserver_total_packetloss_keepalive=0.0000 virtualserver_total_packetloss_control=0.0000 virtualserver_total_packetloss_total=0.0000 virtualserver_total_ping=0.0000 virtualserver_ip=0.0000 virtualserver_weblist_identifier= virtualserver_ask_for_privilegekey=0 virtualserver_hostbanner_mode=0 virtualserver_channel_temp_delete_delay_default=0 virtualserver_min_android_version=1429007622 virtualserver_min_ios_version=1429007622 virtualserver_nickname= virtualserver_unique_identifier={} virtualserver_platform=Windows virtualserver_version=3.5.6 virtualserver_status=online virtualserver_codec_encryption_mode=0 client_talk_power=0 client_needed_serverquery_view_power=0 client_myteamspeak_id= client_integrations= lt=0 pv=6 acn={} aclid={} client_id={}", server_name_escaped, welcome_msg_escaped, server_uid_escaped, client_nickname_escaped, self.session.client_id, self.session.client_id);
        
        tracing::info!("TS3 Desktop Client initserver payload: {}", initserver_str);
        
        let mut out = vec![];
        out.push(initserver_str);
        
        let default_cid = bootstrap.default_cid;
        
        // --- Add missing initialization commands ---
        let mut channel_parts = Vec::new();
        for channel in bootstrap.channels {
            let name_esc = crate::query::encode_query_value(&channel.name);
            let topic_esc = crate::query::encode_query_value(&channel.topic);
            let desc_esc = crate::query::encode_query_value(&channel.description);
            let is_perm = if channel.kind.is_permanent() { 1 } else { 0 };
            let is_semi = if channel.kind.is_semi_permanent() { 1 } else { 0 };
            let is_default = if default_cid == (channel.id as u64) { 1 } else { 0 };
            channel_parts.push(format!(
                "cid={} cpid={} channel_name={} channel_topic={} channel_description={} channel_password={} channel_codec={} channel_codec_quality={} channel_maxclients={} channel_maxfamilyclients={} channel_order={} channel_flag_permanent={} channel_flag_semi_permanent={} channel_flag_default={} channel_flag_password={} channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power={} channel_forced_silence=0 channel_name_phonetic= channel_icon_id=0 channel_flag_private=0",
                channel.id, channel.parent_id, name_esc, topic_esc, desc_esc, crate::query::encode_query_value(&channel.password), channel.codec, channel.codec_quality, channel.maxclients, channel.maxfamilyclients, channel.order, is_perm, is_semi, is_default, if channel.flag_password || !channel.password.is_empty() { 1 } else { 0 }, channel.needed_talk_power()
            ));
        }
        let channellist_str = if channel_parts.is_empty() {
            format!("channellist cid=1 cpid=0 channel_name=Default\\sChannel channel_topic= channel_description= channel_password= channel_codec=4 channel_codec_quality=6 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=1 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic= channel_icon_id=0 channel_flag_private=0")
        } else {
            format!("channellist {}", channel_parts.join("|"))
        };
        
        let notifychannelsubscribed = format!("notifychannelsubscribed cid={} es=0", default_cid);
        let channellistfinished = "channellistfinished";
        let render_rows = |rows: Vec<BTreeMap<String, String>>| -> String {
            rows.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|(k, v)| format!("{}={}", k, crate::query::encode_query_value(&v)))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("|")
        };
        let notifyservergrouplist = format!(
            "notifyservergrouplist {}",
            render_rows(runtime.web_server_group_rows())
        );
        let notifychannelgrouplist = format!(
            "notifychannelgrouplist {}",
            render_rows(runtime.web_channel_group_rows())
        );
        let needed_rows = runtime
            .web_client_needed_permission_rows(
                bootstrap.server_id as u32,
                default_cid as u32,
                bootstrap.dbid,
            )
            .unwrap_or_default();
        let notifyclientneededpermissions = format!(
            "notifyclientneededpermissions {}",
            render_rows(needed_rows)
        );

        out.push(channellist_str);
        out.push(notifychannelsubscribed);

        for online in runtime.store.online_clients.values() {
            if online.server_id == bootstrap.server_id as u32 {
                let enter_notif = crate::transport::TransportNotification::ClientEnterView {
                    presence: crate::transport::SessionPresence {
                        client_id: online.id,
                        login_name: online.nickname.clone(),
                        unique_identifier: online.unique_identifier.clone(),
                        client_type: online.client_type,
                        server_id: online.server_id as u32,
                        channel_id: online.channel_id as u32,
                    },
                    from_channel_id: None,
                    reason_id: 0,
                };
                out.push(crate::transport::render_notification(&enter_notif));
            }
        }
        
        out.push(channellistfinished.to_string());
        out.push(notifyservergrouplist);
        out.push(notifychannelgrouplist);
        out.push(notifyclientneededpermissions);
        // ---------------------------------------------
        
        out.push(String::from("error id=0 msg=ok"));
        
        let enter_self_notif = crate::transport::TransportNotification::ClientEnterView {
            presence: crate::transport::SessionPresence {
                client_id: self.session.client_id,
                login_name: self.session.client_nickname.clone(),
                unique_identifier: self.unique_identifier.clone(),
                client_type: 0,
                server_id: bootstrap.server_id as u32,
                channel_id: self.session.current_channel_id.unwrap(),
            },
            from_channel_id: None,
            reason_id: 0,
        };
        runtime.broadcast_event(bootstrap.server_id as u32, &enter_self_notif);
        let notifs = vec![enter_self_notif];
        
        (out, notifs)
    }
}
