use super::*;
use crate::query::{CommandRequest, QueryResponse};

impl BaselineRuntime {
    pub fn enforce_web_antiflood(
        &mut self,
        command_name: &str,
        selected_server_id: Option<u32>,
        _current_channel_id: Option<u32>,
        _actor_client_database_id: Option<u64>,
        connection_ip: Option<&str>,
        anti_flood_state: &mut AntiFloodSessionState,
    ) -> Option<QueryResponse> {
        let server_id = selected_server_id?;
        let config = self.anti_flood_config_for_server(server_id)?;
        let now_millis = current_unix_timestamp_millis();
        let points_to_add = antiflood_command_cost(command_name);

        if antiflood_command_rejected(config, anti_flood_state, points_to_add, now_millis) {
            return Some(QueryResponse::error(
                ERROR_CLIENT_IS_FLOODING,
                "client is flooding",
            ));
        }

        if let Some(connection_ip) = connection_ip
            && self.shared_ip_antiflood_rejected(
                config,
                server_id,
                connection_ip,
                points_to_add,
                now_millis,
                true,
            )
        {
            return Some(QueryResponse::error(
                ERROR_CLIENT_IS_FLOODING,
                "client is flooding",
            ));
        }

        None
    }

    pub fn web_ban_reason_for_client(
        &mut self,
        server_id: u32,
        _client_database_id: u64,
        unique_identifier: &str,
        connection_ip: &str,
    ) -> Option<String> {
        self.prune_expired_active_bans();

        let matched_ban_id = self
            .store
            .active_bans
            .values()
            .find(|ban| {
                ban.server_id == server_id
                    && ((!ban.unique_identifier.is_empty()
                        && ban.unique_identifier == unique_identifier)
                        || (!ban.ip.is_empty() && ban.ip == connection_ip))
            })
            .map(|ban| ban.id)?;

        let reason = self
            .store
            .active_bans
            .get(&matched_ban_id)
            .map(|ban| ban.reason.clone())
            .unwrap_or_default();

        if let Some(ban) = self.store.active_bans.get_mut(&matched_ban_id) {
            ban.triggers.push(BanTrigger {
                client_unique_identifier: unique_identifier.to_string(),
                client_nickname: ban.name.clone(),
                client_hardware_identifier: ban.hardware_identifier.clone(),
                connection_client_ip: connection_ip.to_string(),
                timestamp: current_unix_timestamp(),
            });
        }

        Some(reason)
    }

    pub fn upsert_web_client(
        &mut self,
        client_id: u64,
        server_id: u32,
        channel_id: u32,
        nickname: String,
        unique_identifier: String,
        database_id: u64,
        version: String,
        platform: String,
        connection_ip: String,
    ) {
        let default_channel_id = self.default_channel_id_for_server(server_id).unwrap_or(channel_id);
        let previous = self.store.online_clients.get(&client_id).cloned();
        self.store.online_clients.insert(
            client_id,
            OnlineClient {
                id: client_id,
                database_id,
                unique_identifier,
                nickname,
                away: previous.as_ref().is_some_and(|client| client.away),
                away_message: previous
                    .as_ref()
                    .map(|client| client.away_message.clone())
                    .unwrap_or_default(),
                input_muted: previous.as_ref().is_some_and(|client| client.input_muted),
                output_muted: previous.as_ref().is_some_and(|client| client.output_muted),
                server_id,
                channel_id: if self.channel_exists(server_id, channel_id) {
                    channel_id
                } else {
                    default_channel_id
                },
                client_type: 0,
                version,
                platform,
                country: previous
                    .as_ref()
                    .map(|client| client.country.clone())
                    .unwrap_or_else(|| String::from("ZZ")),
                connection_ip,
                server_groups: previous
                    .as_ref()
                    .map(|client| client.server_groups.clone())
                    .unwrap_or_else(|| vec![8]),
                connected_at: previous
                    .as_ref()
                    .map(|client| client.connected_at)
                    .unwrap_or_else(current_unix_timestamp),
                last_seen_at: current_unix_timestamp(),
                extra_properties: previous
                    .as_ref()
                    .map(|client| client.extra_properties.clone())
                    .unwrap_or_default(),
            },
        );
    }

    pub fn web_ban_rows(&mut self, server_filter: Option<u32>) -> Vec<BTreeMap<String, String>> {
        self.prune_expired_active_bans();
        self.store
            .active_bans
            .values()
            .filter(|ban| server_filter.is_none_or(|server_id| ban.server_id == server_id))
            .map(|ban| {
                let mut row = BTreeMap::new();
                row.insert(String::from("sid"), ban.server_id.to_string());
                row.insert(String::from("banid"), ban.id.to_string());
                row.insert(String::from("ip"), ban.ip.clone());
                row.insert(String::from("name"), ban.name.clone());
                row.insert(String::from("uid"), ban.unique_identifier.clone());
                row.insert(String::from("hwid"), ban.hardware_identifier.clone());
                row.insert(String::from("created"), ban.created_at.to_string());
                row.insert(String::from("duration"), ban.duration_seconds.to_string());
                row.insert(String::from("invokername"), ban.invoker_name.clone());
                row.insert(
                    String::from("invokercldbid"),
                    ban.invoker_database_id.to_string(),
                );
                row.insert(
                    String::from("invokeruid"),
                    ban.invoker_unique_identifier.clone(),
                );
                row.insert(String::from("reason"), ban.reason.clone());
                row.insert(
                    String::from("enforcements"),
                    ban.triggers.len().to_string(),
                );
                row
            })
            .collect()
    }

    pub fn web_ban_trigger_rows(
        &mut self,
        ban_id: u32,
        server_filter: Option<u32>,
    ) -> Vec<BTreeMap<String, String>> {
        self.prune_expired_active_bans();
        let Some(ban) = self.store.active_bans.get(&ban_id) else {
            return Vec::new();
        };
        if server_filter.is_some_and(|server_id| ban.server_id != server_id) {
            return Vec::new();
        }

        ban.triggers
            .iter()
            .map(|trigger| {
                let mut row = BTreeMap::new();
                row.insert(
                    String::from("client_unique_identifier"),
                    trigger.client_unique_identifier.clone(),
                );
                row.insert(
                    String::from("client_nickname"),
                    trigger.client_nickname.clone(),
                );
                row.insert(
                    String::from("client_hardware_identifier"),
                    trigger.client_hardware_identifier.clone(),
                );
                row.insert(
                    String::from("connection_client_ip"),
                    trigger.connection_client_ip.clone(),
                );
                row.insert(String::from("timestamp"), trigger.timestamp.to_string());
                row
            })
            .collect()
    }

    pub fn web_server_init_info(&self) -> Option<WebServerInitInfo> {
        let server = self
            .store
            .virtual_servers
            .values()
            .min_by_key(|server| server.id)?;

        Some(WebServerInitInfo {
            server_id: server.id,
            server_name: server.name.clone(),
            server_unique_identifier: server.unique_identifier.clone(),
            server_port: server.port,
            welcome_message: server.welcome_message.clone(),
            host_message: server.host_message.clone(),
            host_message_mode: server.host_message_mode,
            ask_for_privilegekey: server.ask_for_privilegekey,
            antiflood_points_tick_reduce: server.antiflood_points_tick_reduce,
            antiflood_points_needed_command_block: server.antiflood_points_needed_command_block,
            antiflood_points_needed_ip_block: server.antiflood_points_needed_ip_block,
            antiflood_ban_time: server.antiflood_ban_time,
        })
    }

    pub fn web_client_name_row_by_dbid(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<BTreeMap<String, String>> {
        let (client_uid, resolved_database_id, client_name) =
            self.lookup_client_identity_by_dbid(server_id, client_database_id)?;

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), resolved_database_id.to_string());
        row.insert(String::from("cluid"), client_uid);
        row.insert(String::from("clname"), client_name);
        Some(row)
    }

    pub fn web_client_name_row_by_uid(
        &self,
        server_id: u32,
        client_uid: &str,
    ) -> Option<BTreeMap<String, String>> {
        let (resolved_uid, client_database_id, client_name) =
            self.lookup_client_identity_by_uid(server_id, client_uid)?;

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        row.insert(String::from("clname"), client_name);
        Some(row)
    }

    pub fn web_client_database_id_row_by_uid(
        &self,
        server_id: u32,
        client_uid: &str,
    ) -> Option<BTreeMap<String, String>> {
        let (resolved_uid, client_database_id, _) =
            self.lookup_client_identity_by_uid(server_id, client_uid)?;

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        Some(row)
    }

    pub fn web_feature_rows(&self) -> Vec<BTreeMap<String, String>> {
        self.build_feature_rows()
    }

    pub fn web_channel_description_row(
        &self,
        server_id: u32,
        channel_id: u32,
    ) -> Option<BTreeMap<String, String>> {
        let channel = self.channel_by_id(server_id, channel_id)?;

        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel_id.to_string());
        row.insert(
            String::from("channel_description"),
            channel.description.clone(),
        );
        Some(row)
    }

    pub fn web_conversation_index_rows(
        &self,
        server_id: u32,
        conversation_ids: &[u32],
    ) -> Vec<BTreeMap<String, String>> {
        conversation_ids
            .iter()
            .filter(|conversation_id| self.conversation_id_exists(server_id, **conversation_id))
            .map(|conversation_id| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cid"), conversation_id.to_string());
                row.insert(
                    String::from("timestamp"),
                    self.latest_conversation_timestamp(server_id, *conversation_id)
                        .to_string(),
                );
                row
            })
            .collect()
    }

    pub fn web_conversation_history_rows(
        &self,
        server_id: u32,
        conversation_id: u32,
        timestamp_begin: Option<u64>,
        timestamp_end: Option<u64>,
        message_count: Option<usize>,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        if !self.conversation_id_exists(server_id, conversation_id) {
            return None;
        }

        let normalized_begin = timestamp_begin.filter(|timestamp| *timestamp > 0);
        let normalized_end = timestamp_end.filter(|timestamp| *timestamp > 1);
        let mut messages = self
            .conversation_messages(server_id, conversation_id)
            .into_iter()
            .filter(|message| {
                normalized_begin.is_none_or(|timestamp| message.timestamp >= timestamp)
                    && normalized_end.is_none_or(|timestamp| message.timestamp <= timestamp)
            })
            .collect::<Vec<_>>();
        messages.sort_by_key(|message| message.timestamp);

        if let Some(limit) = message_count {
            if messages.len() > limit {
                messages = messages.split_off(messages.len() - limit);
            }
        }

        Some(
            messages
                .into_iter()
                .map(|message| {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("cid"), conversation_id.to_string());
                    row.insert(String::from("timestamp"), message.timestamp.to_string());
                    row.insert(
                        String::from("sender_database_id"),
                        message.sender_database_id.to_string(),
                    );
                    row.insert(
                        String::from("sender_unique_id"),
                        message.sender_unique_id.clone(),
                    );
                    row.insert(String::from("sender_name"), message.sender_name.clone());
                    row.insert(String::from("msg"), message.message.clone());
                    row
                })
                .collect(),
        )
    }

    pub fn web_private_conversation_history_rows(
        &self,
        server_id: u32,
        requester_database_id: u64,
        partner_unique_id: Option<&str>,
        partner_database_id: Option<u64>,
        timestamp_begin: Option<u64>,
        timestamp_end: Option<u64>,
        message_count: Option<usize>,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let partner =
            if let Some(partner_unique_id) = partner_unique_id.filter(|value| !value.is_empty()) {
                self.lookup_client_identity_by_uid(server_id, partner_unique_id)
                    .map(|(_, database_id, nickname)| ConversationParticipant {
                        database_id,
                        unique_identifier: partner_unique_id.to_string(),
                        nickname,
                    })
                    .or_else(|| {
                        looks_like_blackteaspeak_unique_id(partner_unique_id).then(|| {
                            ConversationParticipant {
                                database_id: stable_web_client_database_id(partner_unique_id),
                                unique_identifier: partner_unique_id.to_string(),
                                nickname: partner_unique_id.to_string(),
                            }
                        })
                    })
            } else if let Some(partner_database_id) = partner_database_id {
                let (partner_unique_id, _, partner_name) = self
                    .lookup_client_identity_by_dbid(server_id, partner_database_id)
                    .unwrap_or_else(|| (String::new(), partner_database_id, String::new()));
                Some(ConversationParticipant {
                    database_id: partner_database_id,
                    unique_identifier: partner_unique_id,
                    nickname: partner_name,
                })
            } else {
                None
            }?;

        let normalized_begin = timestamp_begin.filter(|timestamp| *timestamp > 0);
        let normalized_end = timestamp_end.filter(|timestamp| *timestamp > 1);
        let mut messages = self
            .private_conversation_messages(server_id, requester_database_id, partner.database_id)
            .into_iter()
            .filter(|message| {
                normalized_begin.is_none_or(|timestamp| message.timestamp >= timestamp)
                    && normalized_end.is_none_or(|timestamp| message.timestamp <= timestamp)
            })
            .collect::<Vec<_>>();
        messages.sort_by_key(|message| message.timestamp);

        if let Some(limit) = message_count {
            if messages.len() > limit {
                messages = messages.split_off(messages.len() - limit);
            }
        }

        Some(
            messages
                .into_iter()
                .map(|message| {
                    let mut row = BTreeMap::new();
                    row.insert(
                        String::from("cluid"),
                        if message.sender_database_id == requester_database_id {
                            message.target_unique_id.clone()
                        } else {
                            message.sender_unique_id.clone()
                        },
                    );
                    row.insert(String::from("cldbid"), partner.database_id.to_string());
                    row.insert(String::from("timestamp"), message.timestamp.to_string());
                    row.insert(
                        String::from("sender_database_id"),
                        message.sender_database_id.to_string(),
                    );
                    row.insert(
                        String::from("sender_unique_id"),
                        message.sender_unique_id.clone(),
                    );
                    row.insert(String::from("sender_name"), message.sender_name.clone());
                    row.insert(String::from("msg"), message.message.clone());
                    row
                })
                .collect(),
        )
    }

    pub fn web_default_channel_id(&self, server_id: u32) -> Option<u32> {
        self.default_channel_id_for_server(server_id)
    }

    pub fn web_server_variables_row(&self, server_id: u32) -> Option<BTreeMap<String, String>> {
        let server = self.store.virtual_servers.get(&server_id)?;
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
        row.insert(String::from("virtualserver_icon_id"), String::from("0"));
        row.insert(
            String::from("virtualserver_hostbanner_mode"),
            String::from("0"),
        );
        row.insert(String::from("virtualserver_hostbanner_url"), String::new());
        row.insert(
            String::from("virtualserver_hostbanner_gfx_url"),
            String::new(),
        );
        row.insert(
            String::from("virtualserver_hostbanner_gfx_interval"),
            String::from("0"),
        );
        Some(row)
    }

    pub fn web_client_variables_row(
        &self,
        server_id: u32,
        client_id: u64,
    ) -> Option<BTreeMap<String, String>> {
        let client = self.online_client_by_id_in_server(server_id, client_id)?;
        let client_type_exact = if client.client_type == 0 && client.platform == "web" {
            3
        } else {
            client.client_type
        };
        let default_hardware = if client.client_type == 1 { "0" } else { "1" };
        let channel_group = self.effective_channel_group(client.channel_id, client.database_id);
        let channel_group_id = channel_group.map(|group| group.id).unwrap_or(0);
        let connected_at = client.connected_at.to_string();

        let mut row = BTreeMap::new();
        row.insert(String::from("clid"), client.id.to_string());
        row.insert(String::from("cid"), client.channel_id.to_string());
        row.insert(
            String::from("client_database_id"),
            client.database_id.to_string(),
        );
        row.insert(String::from("client_nickname"), client.nickname.clone());
        row.insert(
            String::from("client_unique_identifier"),
            client.unique_identifier.clone(),
        );
        row.insert(String::from("client_type"), client.client_type.to_string());
        row.insert(
            String::from("client_type_exact"),
            client_type_exact.to_string(),
        );
        row.insert(String::from("client_description"), String::new());
        row.insert(
            String::from("client_servergroups"),
            client
                .server_groups
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        row.insert(
            String::from("client_channel_group_id"),
            channel_group_id.to_string(),
        );
        row.insert(
            String::from("client_channel_group_inherited_channel_id"),
            client.channel_id.to_string(),
        );
        row.insert(String::from("client_lastconnected"), connected_at.clone());
        row.insert(String::from("client_created"), connected_at);
        row.insert(String::from("client_totalconnections"), String::from("1"));
        row.insert(String::from("client_flag_avatar"), String::new());
        row.insert(String::from("client_icon_id"), String::from("0"));
        row.insert(
            String::from("client_away"),
            if client.away {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("client_away_message"),
            client.away_message.clone(),
        );
        row.insert(String::from("client_country"), client.country.clone());
        row.insert(
            String::from("client_input_hardware"),
            String::from(default_hardware),
        );
        row.insert(
            String::from("client_output_hardware"),
            String::from(default_hardware),
        );
        row.insert(
            String::from("client_input_muted"),
            if client.input_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("client_output_muted"),
            if client.output_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("client_is_channel_commander"),
            String::from("0"),
        );
        row.insert(String::from("client_talk_power"), String::from("0"));
        row.insert(String::from("client_talk_request"), String::from("0"));
        row.insert(String::from("client_talk_request_msg"), String::new());
        row.insert(String::from("client_is_talker"), String::from("0"));
        row.insert(
            String::from("client_is_priority_speaker"),
            String::from("0"),
        );
        row.insert(String::from("client_version"), client.version.clone());
        row.insert(String::from("client_platform"), client.platform.clone());
        row.insert(
            String::from("connection_client_ip"),
            client.connection_ip.clone(),
        );
        row.extend(client.extra_properties.clone());
        Some(row)
    }

    pub fn web_client_connection_info_row(
        &self,
        server_id: u32,
        client_id: u64,
    ) -> Option<BTreeMap<String, String>> {
        let client = self.online_client_by_id_in_server(server_id, client_id)?;
        let mut row = BTreeMap::new();
        row.insert(String::from("clid"), client.id.to_string());
        row.insert(String::from("connection_ping"), String::from("1"));
        row.insert(String::from("connection_ping_deviation"), String::from("0"));
        row.insert(
            String::from("connection_connected_time"),
            current_unix_timestamp()
                .saturating_sub(client.connected_at)
                .to_string(),
        );
        row.insert(String::from("connection_idle_time"), String::from("0"));
        row.insert(String::from("connection_client_ip"), String::new());
        row.insert(String::from("connection_client_port"), String::from("-1"));

        for key in [
            "connection_bandwidth_received_last_minute_control",
            "connection_bandwidth_received_last_minute_keepalive",
            "connection_bandwidth_received_last_minute_speech",
            "connection_bandwidth_received_last_second_control",
            "connection_bandwidth_received_last_second_keepalive",
            "connection_bandwidth_received_last_second_speech",
            "connection_bandwidth_sent_last_minute_control",
            "connection_bandwidth_sent_last_minute_keepalive",
            "connection_bandwidth_sent_last_minute_speech",
            "connection_bandwidth_sent_last_second_control",
            "connection_bandwidth_sent_last_second_keepalive",
            "connection_bandwidth_sent_last_second_speech",
            "connection_bytes_received_control",
            "connection_bytes_received_keepalive",
            "connection_bytes_received_speech",
            "connection_bytes_sent_control",
            "connection_bytes_sent_keepalive",
            "connection_bytes_sent_speech",
            "connection_packets_received_control",
            "connection_packets_received_keepalive",
            "connection_packets_received_speech",
            "connection_packets_sent_control",
            "connection_packets_sent_keepalive",
            "connection_packets_sent_speech",
            "connection_server2client_packetloss_control",
            "connection_server2client_packetloss_keepalive",
            "connection_server2client_packetloss_speech",
            "connection_server2client_packetloss_total",
            "connection_client2server_packetloss_control",
            "connection_client2server_packetloss_keepalive",
            "connection_client2server_packetloss_speech",
            "connection_client2server_packetloss_total",
            "connection_filetransfer_bandwidth_sent",
            "connection_filetransfer_bandwidth_received",
        ] {
            row.insert(String::from(key), String::from("-1"));
        }

        Some(row)
    }

    pub fn web_visible_channel_ids_for_client(
        &self,
        server_id: u32,
        client_database_id: u64,
        forced_visible_channel_id: Option<u32>,
    ) -> BTreeSet<u32> {
        let Some(channels) = self.store.channels.get(&server_id) else {
            return BTreeSet::new();
        };

        let mut visible_channel_ids = BTreeSet::new();
        collect_visible_channel_ids_for_client(
            self,
            channels,
            server_id,
            0,
            client_database_id,
            &mut visible_channel_ids,
        );

        if let Some(forced_channel_id) = forced_visible_channel_id.filter(|channel_id| {
            channels.iter().any(|channel| channel.id == *channel_id)
        }) {
            let mut current_channel_id = Some(forced_channel_id);
            while let Some(channel_id) = current_channel_id.filter(|channel_id| *channel_id != 0) {
                visible_channel_ids.insert(channel_id);
                current_channel_id = channels
                    .iter()
                    .find(|channel| channel.id == channel_id)
                    .map(|channel| channel.parent_id);
            }
        }

        visible_channel_ids
    }

    pub fn web_channel_rows_for_visibility(
        &self,
        server_id: u32,
        visible_channel_ids: &BTreeSet<u32>,
    ) -> Vec<BTreeMap<String, String>> {
        ordered_visible_channel_ids(
            self.store
                .channels
                .get(&server_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            0,
            visible_channel_ids,
        )
        .into_iter()
        .filter_map(|channel_id| {
            self.web_channel_row_for_visibility(server_id, channel_id, visible_channel_ids)
        })
        .collect()
    }

    pub fn web_channel_row_for_visibility(
        &self,
        server_id: u32,
        channel_id: u32,
        visible_channel_ids: &BTreeSet<u32>,
    ) -> Option<BTreeMap<String, String>> {
        if !visible_channel_ids.contains(&channel_id) {
            return None;
        }

        let channels = self.store.channels.get(&server_id)?;
        let channel = channels.iter().find(|channel| channel.id == channel_id)?;
        let sibling_ids = ordered_sibling_ids(channels, channel.parent_id, None);
        let mut previous_visible_id = 0;

        for sibling_id in sibling_ids {
            if !visible_channel_ids.contains(&sibling_id) {
                continue;
            }

            if sibling_id == channel_id {
                return Some(self.build_web_channel_row(server_id, channel, previous_visible_id));
            }

            previous_visible_id = sibling_id;
        }

        None
    }

    pub fn web_channel_rows(&self, server_id: u32) -> Vec<BTreeMap<String, String>> {
        let visible_channel_ids = self
            .store
            .channels
            .get(&server_id)
            .map(|channels| channels.iter().map(|channel| channel.id).collect())
            .unwrap_or_default();
        self.web_channel_rows_for_visibility(server_id, &visible_channel_ids)
    }

    pub fn web_connection_info_row(&self, server_id: u32) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(
            String::from("connection_filetransfer_bandwidth_sent"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_received"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bytes_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bytes_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bytes_sent_month"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bytes_received_month"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_packets_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_packets_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bandwidth_sent_last_second_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bandwidth_sent_last_minute_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bandwidth_received_last_second_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bandwidth_received_last_minute_total"),
            String::from("0"),
        );
        row.insert(String::from("connection_connected_time"), String::from("0"));
        row.insert(
            String::from("connection_packetloss_total"),
            String::from("0"),
        );
        row.insert(String::from("connection_ping"), String::from("0"));
        row.insert(
            String::from("virtualserver_clientsonline"),
            self.client_count_in_server(server_id).to_string(),
        );
        row
    }

    pub fn web_visible_client_rows(&self, server_id: u32) -> Vec<BTreeMap<String, String>> {
        self.web_visible_client_rows_excluding(server_id, None)
    }

    pub fn web_visible_client_rows_excluding(
        &self,
        server_id: u32,
        exclude_client_id: Option<u64>,
    ) -> Vec<BTreeMap<String, String>> {
        let visible_channel_ids = self
            .store
            .channels
            .get(&server_id)
            .map(|channels| channels.iter().map(|channel| channel.id).collect())
            .unwrap_or_default();
        self.web_visible_client_rows_excluding_in_channels(
            server_id,
            exclude_client_id,
            &visible_channel_ids,
        )
    }

    pub fn web_visible_client_rows_excluding_in_channels(
        &self,
        server_id: u32,
        exclude_client_id: Option<u64>,
        visible_channel_ids: &BTreeSet<u32>,
    ) -> Vec<BTreeMap<String, String>> {
        let mut clients = self
            .store
            .online_clients
            .values()
            .filter(|client| {
                client.server_id == server_id
                    && Some(client.id) != exclude_client_id
                    && visible_channel_ids.contains(&client.channel_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        clients.sort_by(|left, right| {
            left.channel_id
                .cmp(&right.channel_id)
                .then_with(|| left.client_type.cmp(&right.client_type))
                .then_with(|| left.id.cmp(&right.id))
        });

        clients
            .into_iter()
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("cfid"), String::from("0"));
                row.insert(String::from("ctid"), client.channel_id.to_string());
                row.insert(String::from("reasonid"), String::from("2"));
                row.insert(String::from("client_nickname"), client.nickname);
                row.insert(
                    String::from("client_unique_identifier"),
                    client.unique_identifier,
                );
                row.insert(String::from("client_type"), client.client_type.to_string());
                row.insert(
                    String::from("client_type_exact"),
                    client.client_type.to_string(),
                );
                row.insert(
                    String::from("client_database_id"),
                    client.database_id.to_string(),
                );
                row.insert(
                    String::from("client_servergroups"),
                    client
                        .server_groups
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
                row.insert(String::from("client_version"), client.version);
                row.insert(String::from("client_platform"), client.platform);
                row.insert(String::from("client_country"), client.country);
                row.insert(String::from("connection_client_ip"), client.connection_ip);
                row.extend(client.extra_properties);
                row
            })
                .collect()
    }

}
