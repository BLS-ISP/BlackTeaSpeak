use super::*;

impl BaselineRuntime {
    
    pub fn route_btea_media_to_desktop(&self, server_id: u32, sender_client_id: u64, packet_type: u8, payload: &[u8]) {
        if packet_type != 0x0B && !self.client_can_speak_in_channel(server_id, sender_client_id) {
            return;
        }
        if let Some(tx) = &self.desktop_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, packet_type, payload.to_vec()));
        }
    }

    pub fn route_btea_media_to_webtransport(&self, server_id: u32, sender_client_id: u64, packet_type: u8, payload: &[u8]) {
        if !self.client_can_speak_in_channel(server_id, sender_client_id) {
            return;
        }
        if let Some(tx) = &self.webtransport_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, packet_type, payload.to_vec()));
        }
    }


    pub fn set_file_transfer_registry(&mut self, registry: std::sync::Arc<crate::file_transfer::FileTransferRegistry>) {
        self.file_transfer_registry = Some(registry);
    }

    pub fn execute(&mut self, input: &str, session: &mut QuerySessionState) -> String {
        let response = match parse_request_line(input) {
            Ok(request) => self.execute_request(request, session),
            Err(error) => QueryResponse::error(1536, error.to_string()),
        };
        render_response(&response)
    }


    pub fn online_client_snapshot(&self, server_id: u32, client_id: u64) -> Option<OnlineClientSnapshot> {
        self.store.online_clients.get(&client_id)
            .filter(|client| client.server_id == server_id)
            .map(|client| OnlineClientSnapshot {
                id: client.id,
                database_id: client.database_id,
                unique_identifier: client.unique_identifier.clone(),
                nickname: client.nickname.clone(),
                away: client.away,
                away_message: client.away_message.clone(),
                input_muted: client.input_muted,
                output_muted: client.output_muted,
                server_id: client.server_id,
                channel_id: client.channel_id,
                client_type: client.client_type,
                client_type_exact: client
                    .extra_properties
                    .get("client_type_exact")
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(client.client_type),
                whisper_targets: client.whisper_targets.clone(),
                ignored_clients: Vec::new(),
                version: client.version.clone(),
                platform: client.platform.clone(),
                country: client.country.clone(),
                connection_ip: client.connection_ip.clone(),
                server_groups: client.server_groups.clone(),
                client_flag_avatar: String::new(),
                extra_properties: client.extra_properties.clone(),
            })
    }

    pub fn channel_exists_for_server(&self, server_id: u32, channel_id: u32) -> bool {
        self.channel_exists(server_id, channel_id)
    }

    pub fn cleanup_temporary_channels(
        &mut self,
        server_id: u32,
        channel_ids: &[u32],
    ) -> Vec<TemporaryChannelCleanup> {
        let mut seen = BTreeSet::new();
        let mut cleanups = Vec::new();

        for channel_id in channel_ids {
            if !seen.insert(*channel_id) {
                continue;
            }

            if let Some(cleanup) = self.cleanup_temporary_channel(server_id, *channel_id) {
                cleanups.push(cleanup);
            }
        }

        cleanups
    }

    pub fn update_online_client_properties(
        &mut self,
        server_id: u32,
        client_id: u64,
        updates: &BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, String>> {
        let client = self
            .store
            .online_clients
            .get_mut(&client_id)
            .filter(|client| client.server_id == server_id)?;
        let mut changed = BTreeMap::new();

        for (key, value) in updates {
            match key.as_str() {
                "clid" => {}
                "client_nickname" if !value.is_empty() && client.nickname != *value => {
                    client.nickname = value.clone();
                    changed.insert(key.clone(), value.clone());
                }
                "client_away" => {
                    let new_value = runtime_bool_flag(value);
                    if client.away != new_value {
                        client.away = new_value;
                        changed.insert(
                            key.clone(),
                            if new_value {
                                String::from("1")
                            } else {
                                String::from("0")
                            },
                        );
                    }
                    if !new_value
                        && !updates.contains_key("client_away_message")
                        && !client.away_message.is_empty()
                    {
                        client.away_message.clear();
                        changed.insert(String::from("client_away_message"), String::new());
                    }
                }
                "client_away_message" if client.away_message != *value => {
                    client.away_message = value.clone();
                    changed.insert(key.clone(), value.clone());
                }
                "client_input_muted" => {
                    let new_value = runtime_bool_flag(value);
                    if client.input_muted != new_value {
                        client.input_muted = new_value;
                        changed.insert(
                            key.clone(),
                            if new_value {
                                String::from("1")
                            } else {
                                String::from("0")
                            },
                        );
                    }
                }
                "client_output_muted" => {
                    let new_value = runtime_bool_flag(value);
                    if client.output_muted != new_value {
                        client.output_muted = new_value;
                        changed.insert(
                            key.clone(),
                            if new_value {
                                String::from("1")
                            } else {
                                String::from("0")
                            },
                        );
                    }
                }
                "client_country" if client.country != *value => {
                    client.country = value.clone();
                    changed.insert(key.clone(), value.clone());
                }
                "client_version" if client.version != *value => {
                    client.version = value.clone();
                    changed.insert(key.clone(), value.clone());
                }
                "client_platform" if client.platform != *value => {
                    client.platform = value.clone();
                    changed.insert(key.clone(), value.clone());
                }
                "player_volume" => {
                    let normalized = Self::format_music_bot_volume_factor(
                        Self::parse_music_bot_volume_factor(value),
                    );
                    if client.extra_properties.get(key) != Some(&normalized) {
                        client.extra_properties.insert(key.clone(), normalized.clone());
                        changed.insert(key.clone(), normalized);
                    }
                }
                _ => {
                    if client.extra_properties.get(key) != Some(value) {
                        client.extra_properties.insert(key.clone(), value.clone());
                        changed.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        if let Some(bot_id) = self.music_bot_id_by_client(server_id, client_id) {
            let mut sync_bot_client = false;
            let mut seek_target_secs = None;
            if let Some(player_volume) = changed.get("player_volume")
                && let Some(bot) = self.store.music_bots.get_mut(&bot_id)
                && bot.player_volume != *player_volume
            {
                bot.player_volume = player_volume.clone();
                if matches!(bot.state, MusicBotState::Playing) {
                    let replay_ms = Self::music_bot_current_song_replay_millis(bot);
                    bot.current_song_progress_millis = replay_ms;
                    bot.current_song_started_at_millis = Some(current_unix_timestamp_millis());
                    seek_target_secs = Some(replay_ms / 1000);
                }
                sync_bot_client = true;
            }
            if let Some(secs) = seek_target_secs {
                crate::runtime::musicbot::audio_engine::request_bot_seek(bot_id, secs);
            }

            if sync_bot_client {
                self.sync_music_bot_client_state(bot_id);
            }
        }

        Some(changed)
    }




    pub fn online_client_identity(
        &self,
        server_id: u32,
        client_id: u64,
    ) -> Option<(String, u64, String)> {
        self.store
            .online_clients
            .get(&client_id)
            .filter(|client| client.server_id == server_id)
            .map(|client| {
                (
                    client.unique_identifier.clone(),
                    client.database_id,
                    client.nickname.clone(),
                )
            })
    }

    pub fn online_client_id_by_database_id(&self, server_id: u32, client_database_id: u64) -> Option<u64> {
        self.store
            .online_clients
            .values()
            .find(|client| client.server_id == server_id && client.database_id == client_database_id)
            .map(|client| client.id)
    }


    pub fn annotate_active_ban(
        &mut self,
        ban_id: u32,
        last_nickname: Option<String>,
        connection_ip: Option<String>,
        invoker_name: String,
        invoker_database_id: u64,
        invoker_unique_identifier: String,
    ) {
        if let Some(ban) = self.store.active_bans.get_mut(&ban_id) {
            if let Some(last_nickname) = last_nickname.filter(|value| !value.is_empty()) {
                ban.name = last_nickname;
            }
            if let Some(connection_ip) = connection_ip.filter(|value| !value.is_empty()) {
                ban.ip = connection_ip;
            }
            ban.invoker_name = invoker_name;
            ban.invoker_database_id = invoker_database_id;
            ban.invoker_unique_identifier = invoker_unique_identifier;
        }
        self.persist_state_if_configured();
    }



    pub fn create_manual_active_ban(
        &mut self,
        server_id: u32,
        name: String,
        unique_identifier: String,
        hardware_identifier: String,
        ip: String,
        reason: String,
        duration_seconds: u32,
        invoker_name: String,
        invoker_database_id: u64,
        invoker_unique_identifier: String,
    ) -> u32 {
        self.prune_expired_active_bans();
        let ban_id = self.store.next_ban_id.max(1);
        self.store.next_ban_id = ban_id.saturating_add(1);
        self.store.active_bans.insert(
            ban_id,
            ActiveBan {
                id: ban_id,
                server_id,
                name,
                unique_identifier,
                hardware_identifier,
                ip,
                reason,
                created_at: current_unix_timestamp(),
                duration_seconds,
                invoker_name,
                invoker_database_id,
                invoker_unique_identifier,
                triggers: Vec::new(),
            },
        );
        self.persist_state_if_configured();
        ban_id
    }

    pub fn update_active_ban(
        &mut self,
        ban_id: u32,
        server_filter: Option<u32>,
        row: &BTreeMap<String, String>,
    ) -> bool {
        self.prune_expired_active_bans();
        let Some(ban) = self.store.active_bans.get_mut(&ban_id) else {
            return false;
        };
        if server_filter.is_some_and(|server_id| ban.server_id != server_id) {
            return false;
        }

        if let Some(value) = row.get("name") {
            ban.name = value.clone();
        }
        if let Some(value) = row.get("uid") {
            ban.unique_identifier = value.clone();
        }
        if let Some(value) = row.get("hwid") {
            ban.hardware_identifier = value.clone();
        }
        if let Some(value) = row.get("ip") {
            ban.ip = value.clone();
        }
        if let Some(value) = row.get("banreason").or_else(|| row.get("reason")) {
            ban.reason = value.clone();
        }
        if let Some(value) = row.get("time").and_then(|value| value.parse::<u32>().ok()) {
            ban.duration_seconds = value;
        }

        self.persist_state_if_configured();
        true
    }

    pub fn remove_active_ban(&mut self, ban_id: u32, server_filter: Option<u32>) -> bool {
        self.prune_expired_active_bans();
        let Some(ban) = self.store.active_bans.get(&ban_id) else {
            return false;
        };
        if server_filter.is_some_and(|server_id| ban.server_id != server_id) {
            return false;
        }
        self.store.active_bans.remove(&ban_id);
        self.persist_state_if_configured();
        true
    }

    pub fn remove_session_client_without_left_event(&mut self, client_id: u64) {
        let removed_bot_ids: Vec<u32> = self
            .store
            .music_bots
            .iter()
            .filter_map(|(bot_id, bot)| (bot.linked_client_id == Some(client_id)).then_some(*bot_id))
            .collect();
        for bot_id in &removed_bot_ids {
            if let Some(mut removed_bot) = self.store.music_bots.remove(bot_id) {
                Self::stop_music_bot_playback(&mut removed_bot);
            }
        }
        let Some(client) = self.store.online_clients.remove(&client_id) else {
            if !removed_bot_ids.is_empty() {
                self.persist_state_if_configured();
            }
            return;
        };
        self.clear_client_media_broadcasts(client.server_id, client.id);
        if let Some(cleanup) = self.cleanup_temporary_channel(client.server_id, client.channel_id) {
            if let Some(removed_bot_client) = &cleanup.removed_client {
                let left_notif = crate::transport::TransportNotification::ClientLeftView {
                    presence: crate::transport::SessionPresence {
                        client_id: removed_bot_client.id,
                        login_name: removed_bot_client.nickname.clone(),
                        unique_identifier: removed_bot_client.unique_identifier.clone(),
                        client_type: removed_bot_client.client_type_exact,
                        server_id: removed_bot_client.server_id,
                        channel_id: removed_bot_client.channel_id,
                    },
                    to_channel_id: None,
                    reason_id: 5,
                    reason_message: String::from("temporary channel cleanup"),
                    invoker_id: 0,
                    invoker_name: String::from("Server"),
                    invoker_uid: String::new(),
                    ban_time: None,
                };
                self.broadcast_event(client.server_id, &left_notif);
            }
            if let Some(removed_channel) = &cleanup.removed_channel {
                let del_notif = crate::transport::TransportNotification::ChannelDeleted {
                    server_id: client.server_id,
                    channel: removed_channel.clone(),
                    invoker_id: 0,
                    invoker_name: String::from("Server"),
                };
                self.broadcast_event(client.server_id, &del_notif);
            }
            self.pending_temp_cleanups.push(cleanup);
        }
        if !removed_bot_ids.is_empty() {
            self.persist_state_if_configured();
        }
    }

    pub fn remove_session_client(&mut self, client_id: u64, reason_id: u32, reason_message: String) {
        let removed_bot_ids: Vec<u32> = self
            .store
            .music_bots
            .iter()
            .filter_map(|(bot_id, bot)| (bot.linked_client_id == Some(client_id)).then_some(*bot_id))
            .collect();
        for bot_id in &removed_bot_ids {
            if let Some(mut removed_bot) = self.store.music_bots.remove(bot_id) {
                Self::stop_music_bot_playback(&mut removed_bot);
            }
        }
        if let Some(client) = self.store.online_clients.remove(&client_id) {
            self.clear_client_media_broadcasts(client.server_id, client.id);
            let client_type = client.extra_properties.get("client_type_exact")
                .and_then(|s| s.parse().ok())
                .unwrap_or(client.client_type);
            let presence = crate::transport::SessionPresence {
                client_id: client.id,
                login_name: client.nickname.clone(),
                unique_identifier: client.unique_identifier.clone(),
                client_type,
                server_id: client.server_id,
                channel_id: client.channel_id,
            };
            let notif = crate::transport::TransportNotification::ClientLeftView {
                presence,
                to_channel_id: None,
                reason_id,
                reason_message,
                invoker_id: 0,
                invoker_name: String::new(),
                invoker_uid: String::new(),
                ban_time: None,
            };
            self.broadcast_event(client.server_id, &notif);
            if let Some(cleanup) = self.cleanup_temporary_channel(client.server_id, client.channel_id) {
                if let Some(removed_bot_client) = &cleanup.removed_client {
                    let left_notif = crate::transport::TransportNotification::ClientLeftView {
                        presence: crate::transport::SessionPresence {
                            client_id: removed_bot_client.id,
                            login_name: removed_bot_client.nickname.clone(),
                            unique_identifier: removed_bot_client.unique_identifier.clone(),
                            client_type: removed_bot_client.client_type_exact,
                            server_id: removed_bot_client.server_id,
                            channel_id: removed_bot_client.channel_id,
                        },
                        to_channel_id: None,
                        reason_id: 5,
                        reason_message: String::from("temporary channel cleanup"),
                        invoker_id: 0,
                        invoker_name: String::from("Server"),
                        invoker_uid: String::new(),
                        ban_time: None,
                    };
                    self.broadcast_event(client.server_id, &left_notif);
                }
                if let Some(removed_channel) = &cleanup.removed_channel {
                    let del_notif = crate::transport::TransportNotification::ChannelDeleted {
                        server_id: client.server_id,
                        channel: removed_channel.clone(),
                        invoker_id: 0,
                        invoker_name: String::from("Server"),
                    };
                    self.broadcast_event(client.server_id, &del_notif);
                }
                self.pending_temp_cleanups.push(cleanup);
            }
        }
        if !removed_bot_ids.is_empty() {
            self.persist_state_if_configured();
        }
    }

    pub fn query_session_unique_identifier(&self, session: &QuerySessionState) -> String {
        if let Some(login_name) = session.authenticated_login.as_ref() {
            return stable_query_client_unique_identifier(login_name);
        }

        if let (Some(server_id), Some(client_database_id)) = (
            session.selected_virtual_server_id,
            session.actor_client_database_id_override,
        )
            && let Some((unique_identifier, _, _)) =
                self.lookup_client_identity_by_dbid(server_id, client_database_id)
        {
            return unique_identifier;
        }

        format!("compat-query-{}", session.client_id)
    }

    pub(crate) fn anti_flood_config_for_server(&self, server_id: u32) -> Option<AntiFloodConfig> {
        self.store.virtual_servers.get(&server_id).map(|server| AntiFloodConfig {
            points_tick_reduce: server.antiflood_points_tick_reduce,
            points_needed_command_block: server.antiflood_points_needed_command_block,
            points_needed_ip_block: server.antiflood_points_needed_ip_block,
            ban_time: server.antiflood_ban_time,
        })
    }

    pub(crate) fn shared_ip_antiflood_rejected(
        &mut self,
        config: AntiFloodConfig,
        server_id: u32,
        connection_ip: &str,
        points_to_add: u32,
        now_millis: u64,
        skip_loopback: bool,
    ) -> bool {
        if connection_ip.trim().is_empty() {
            return false;
        }
        if skip_loopback
            && connection_ip
                .parse::<IpAddr>()
                .ok()
                .is_some_and(|ip| ip.is_loopback())
        {
            return false;
        }

        let anti_flood_state = self
            .anti_flood_ip_states
            .entry((server_id, connection_ip.to_string()))
            .or_default();
        antiflood_command_rejected(config, anti_flood_state, points_to_add, now_millis)
    }

    pub(crate) fn prune_expired_active_bans(&mut self) {
        let now = current_unix_timestamp();
        self.store.active_bans.retain(|_, ban| {
            ban.duration_seconds == 0
                || ban
                    .created_at
                    .saturating_add(u64::from(ban.duration_seconds))
                    > now
        });
    }

    pub(crate) fn prune_expired_temp_passwords(&mut self) {
        let now = current_unix_timestamp();
        self.store
            .server_temp_passwords
            .retain(|entry| entry.end_timestamp == 0 || entry.end_timestamp > now);
    }

    pub fn run_housekeeping(&mut self) {
        self.prune_expired_active_bans();
        self.prune_expired_temp_passwords();
        
        let now = current_unix_timestamp();
        let zombie_timeout_seconds = 60;
        
        let linked_music_bot_clients: std::collections::BTreeSet<u64> = self
            .store
            .music_bots
            .values()
            .filter_map(|bot| bot.linked_client_id)
            .collect();

        // Find zombie clients (never expire server-hosted MusicBots)
        let zombie_clients: Vec<(u64, u32, u32)> = self.store.online_clients
            .iter()
            .filter(|(id, client)| {
                let is_music_bot = client.client_type == 2
                    || client
                        .extra_properties
                        .get("client_type_exact")
                        .is_some_and(|v| v == "4")
                    || client.unique_identifier.starts_with("compat-musicbot-")
                    || linked_music_bot_clients.contains(id);
                if is_music_bot {
                    return false;
                }
                now.saturating_sub(client.last_seen_at) > zombie_timeout_seconds
            })
            .map(|(id, client)| (*id, client.server_id, client.channel_id))
            .collect();
            
        // Remove them
        for (client_id, _server_id, _channel_id) in zombie_clients {
            self.remove_session_client(client_id, 3, "timeout".to_string());
        }
        
        // Clean up temporary channels that might be empty or only contain a music bot
        let mut cleanup_candidates = Vec::new();
        for (server_id, channels) in &self.store.channels {
            for channel in channels {
                if channel.kind == ChannelKind::Temporary {
                    cleanup_candidates.push((*server_id, channel.id));
                }
            }
        }
        
        for (server_id, channel_id) in cleanup_candidates {
            if let Some(cleanup) = self.cleanup_temporary_channel(server_id, channel_id) {
                if let Some(removed_bot_client) = &cleanup.removed_client {
                    let left_notif = crate::transport::TransportNotification::ClientLeftView {
                        presence: crate::transport::SessionPresence {
                            client_id: removed_bot_client.id,
                            login_name: removed_bot_client.nickname.clone(),
                            unique_identifier: removed_bot_client.unique_identifier.clone(),
                            client_type: removed_bot_client.client_type_exact,
                            server_id: removed_bot_client.server_id,
                            channel_id: removed_bot_client.channel_id,
                        },
                        to_channel_id: None,
                        reason_id: 5,
                        reason_message: String::from("temporary channel cleanup"),
                        invoker_id: 0,
                        invoker_name: String::from("Server"),
                        invoker_uid: String::new(),
                        ban_time: None,
                    };
                    self.broadcast_event(server_id, &left_notif);
                }
                if let Some(removed_channel) = &cleanup.removed_channel {
                    let del_notif = crate::transport::TransportNotification::ChannelDeleted {
                        server_id,
                        channel: removed_channel.clone(),
                        invoker_id: 0,
                        invoker_name: String::from("Server"),
                    };
                    self.broadcast_event(server_id, &del_notif);
                }
            }
        }
    }

    pub fn mark_client_seen(&mut self, client_id: u64) {
        if let Some(client) = self.store.online_clients.get_mut(&client_id) {
            client.last_seen_at = current_unix_timestamp();
        }
    }

    pub fn track_bandwidth(&mut self, client_id: u64, uploaded_bytes: u64, downloaded_bytes: u64) {
        let database_id = self.store.online_clients.get(&client_id).map(|c| c.database_id);
        if let Some(db_id) = database_id {
            if let Some(client) = self.store.clients.get_mut(&db_id) {
                client.total_bytes_uploaded = client.total_bytes_uploaded.saturating_add(uploaded_bytes);
                client.total_bytes_downloaded = client.total_bytes_downloaded.saturating_add(downloaded_bytes);
                client.month_bytes_uploaded = client.month_bytes_uploaded.saturating_add(uploaded_bytes);
                client.month_bytes_downloaded = client.month_bytes_downloaded.saturating_add(downloaded_bytes);
            }
        }
    }

    pub(crate) fn register_active_ban(
        &mut self,
        target: &OnlineClientSnapshot,
        duration_seconds: u32,
        reason: String,
    ) -> u32 {
        self.prune_expired_active_bans();
        let ban_id = self.store.next_ban_id.max(1);
        self.store.next_ban_id = ban_id.saturating_add(1);
        self.store.active_bans.insert(
            ban_id,
            ActiveBan {
                id: ban_id,
                server_id: target.server_id,
                name: target.nickname.clone(),
                unique_identifier: target.unique_identifier.clone(),
                hardware_identifier: String::new(),
                ip: target.connection_ip.clone(),
                reason,
                created_at: current_unix_timestamp(),
                duration_seconds,
                invoker_name: String::new(),
                invoker_database_id: 0,
                invoker_unique_identifier: String::new(),
                triggers: vec![BanTrigger {
                    client_unique_identifier: target.unique_identifier.clone(),
                    client_nickname: target.nickname.clone(),
                    client_hardware_identifier: String::new(),
                    connection_client_ip: target.connection_ip.clone(),
                    timestamp: current_unix_timestamp(),
                }],
            },
        );
        ban_id
    }
    pub(crate) fn target_client_snapshot_and_permissions(
        &self,
        server_id: u32,
        target_client_id: u64,
    ) -> std::result::Result<
        (
            OnlineClientSnapshot,
            BTreeMap<String, PermissionAssignment>,
        ),
        QueryResponse,
    > {
        let Some(snapshot) = self.online_client_snapshot(server_id, target_client_id) else {
            return Err(QueryResponse::error(768, "target client not found"));
        };
        let Some(target_permissions) = self.effective_permissions_for_client(
            server_id,
            snapshot.channel_id,
            snapshot.database_id,
        ) else {
            return Err(QueryResponse::error(768, "target client not found"));
        };

        Ok((snapshot, target_permissions))
    }

    pub(crate) fn check_target_client_power(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
        actor_power_names: &[&str],
        needed_power_names: &[&str],
        failed_permission_name: &str,
    ) -> Option<QueryResponse> {
        if permission_value_or_default(actor_permissions, actor_power_names)
            < permission_value_or_default(target_permissions, needed_power_names)
        {
            return Some(self.insufficient_permission_response(failed_permission_name));
        }

        None
    }

    pub fn banner_lines(&self) -> Vec<String> {
        vec![
            String::from("BlackTeaSpeak Compat ServerQuery"),
            format!(
                "version={} build={} platform=compat-rust",
                self.specs.build_version.build_version, self.specs.build_version.build_index
            ),
            format!(
                "baseline_commands={} binary_sha256={}",
                self.specs.baseline_profile.essential_commands.len(),
                self.specs.binary_manifest.binary.sha256
            ),
        ]
    }










    pub fn delete_conversation_messages(
        &mut self,
        server_id: u32,
        conversation_id: u32,
        timestamp_begin: Option<u64>,
        timestamp_end: Option<u64>,
        limit: Option<usize>,
        sender_database_id: Option<u64>,
    ) -> usize {
        let Some(messages) = self.store.conversation_messages.get_mut(&server_id) else {
            return 0;
        };

        let normalized_begin = timestamp_begin.filter(|timestamp| *timestamp > 0);
        let normalized_end = timestamp_end.filter(|timestamp| *timestamp > 0);
        let mut deleted = 0_usize;
        let max_delete = limit.unwrap_or(usize::MAX);

        messages.retain(|message| {
            let matches = message.conversation_id == conversation_id
                && normalized_begin.is_none_or(|timestamp| message.timestamp >= timestamp)
                && normalized_end.is_none_or(|timestamp| message.timestamp <= timestamp)
                && sender_database_id
                    .is_none_or(|database_id| message.sender_database_id == database_id)
                && deleted < max_delete;
            if matches {
                deleted = deleted.saturating_add(1);
                false
            } else {
                true
            }
        });

        deleted
    }





    pub(crate) fn build_web_channel_row(
        &self,
        server_id: u32,
        channel: &Channel,
        channel_order: u32,
    ) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel.id.to_string());
        row.insert(String::from("cpid"), channel.parent_id.to_string());
        row.insert(String::from("pid"), channel.parent_id.to_string());
        row.insert(String::from("channel_order"), channel_order.to_string());
        row.insert(String::from("channel_name"), channel.name.clone());
        row.insert(String::from("channel_topic"), channel.topic.clone());
        row.insert(
            String::from("channel_description"),
            channel.description.clone(),
        );
        row.insert(String::from("channel_codec"), channel.codec.to_string());
        row.insert(
            String::from("channel_codec_quality"),
            channel.codec_quality.to_string(),
        );
        row.insert(
            String::from("channel_maxclients"),
            channel.maxclients.to_string(),
        );
        row.insert(
            String::from("channel_maxfamilyclients"),
            channel.maxfamilyclients.to_string(),
        );
        row.insert(
            String::from("channel_flag_maxclients_unlimited"),
            if channel.maxclients < 0 {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("channel_flag_maxfamilyclients_unlimited"),
            if channel.maxfamilyclients < 0 {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("channel_flag_maxfamilyclients_inherited"),
            String::from("0"),
        );
        row.insert(
            String::from("channel_needed_talk_power"),
            channel.needed_talk_power().to_string(),
        );
        row.insert(
            String::from("i_channel_needed_join_power"),
            channel
                .permission_power("i_channel_needed_join_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_subscribe_power"),
            channel
                .permission_power("i_channel_needed_subscribe_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_description_view_power"),
            channel
                .permission_power("i_channel_needed_description_view_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_modify_power"),
            channel
                .permission_power("i_channel_needed_modify_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_delete_power"),
            channel
                .permission_power("i_channel_needed_delete_power")
                .to_string(),
        );
        row.insert(
            String::from("channel_icon_id"),
            channel.permission_power("i_icon_id").to_string(),
        );
        apply_channel_kind_rows(&mut row, channel.kind);
        row.insert(
            String::from("total_clients"),
            self.client_count_in_channel(server_id, channel.id).to_string(),
        );
        row.insert(
            String::from("total_clients_family"),
            self.family_client_count_in_channel(server_id, channel.id)
                .to_string(),
        );
        row.insert(
            String::from("channel_flag_default"),
            if self.default_channel_id_for_server(server_id) == Some(channel.id) {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("channel_flag_password"),
            if channel.flag_password || !channel.password.is_empty() {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row
    }

    pub fn build_version(&self) -> &BuildVersion {
        &self.specs.build_version
    }

    pub fn snapshot_channel(&self, server_id: u32, channel_id: u32) -> Option<ChannelSnapshot> {
        let default_cid = self.default_channel_id_for_server(server_id);
        self.store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| ChannelSnapshot {
                id: channel.id,
                parent_id: channel.parent_id,
                order: channel.order,
                kind: channel.kind,
                name: channel.name.clone(),
                topic: channel.topic.clone(),
                description: channel.description.clone(),
                codec: channel.codec,
                codec_quality: channel.codec_quality,
                maxclients: channel.maxclients,
                maxfamilyclients: channel.maxfamilyclients,
                flag_default: default_cid == Some(channel.id) || channel.flag_default,
                flag_password: channel.flag_password || !channel.password.is_empty(),
                needed_talk_power: channel.needed_talk_power(),
                needed_join_power: channel.permission_power("i_channel_needed_join_power"),
                needed_subscribe_power: channel
                    .permission_power("i_channel_needed_subscribe_power"),
                needed_description_view_power: channel
                    .permission_power("i_channel_needed_description_view_power"),
                needed_modify_power: channel.permission_power("i_channel_needed_modify_power"),
                needed_delete_power: channel.permission_power("i_channel_needed_delete_power"),
                icon_id: channel.permission_power("i_icon_id"),
                total_clients: self.client_count_in_channel(server_id, channel.id),
            })
    }

    pub fn snapshot_server(&self, server_id: u32) -> Option<ServerSnapshot> {
        self.store
            .virtual_servers
            .get(&server_id)
            .map(|server| ServerSnapshot {
                id: server.id,
                port: server.port,
                name: server.name.clone(),
                unique_identifier: server.unique_identifier.clone(),
                welcome_message: server.welcome_message.clone(),
                host_message: server.host_message.clone(),
                host_message_mode: server.host_message_mode,
                ask_for_privilegekey: server.ask_for_privilegekey,
                max_clients: server.max_clients,
                antiflood_points_tick_reduce: server.antiflood_points_tick_reduce,
                antiflood_points_needed_command_block: server.antiflood_points_needed_command_block,
                antiflood_points_needed_ip_block: server.antiflood_points_needed_ip_block,
                antiflood_ban_time: server.antiflood_ban_time,
            })
    }































    pub(crate) fn build_permission_rows(&self) -> Vec<BTreeMap<String, String>> {
        let mut permissions = self.all_known_permission_names();
        permissions.sort_by_key(|permission_name| self.permission_id_for_name(permission_name));

        permissions
            .into_iter()
            .map(|permission_name| {
                let mut row = BTreeMap::new();
                row.insert(
                    String::from("permid"),
                    self.permission_id_for_name(&permission_name).to_string(),
                );
                row.insert(String::from("permname"), permission_name.clone());
                row.insert(
                    String::from("permdesc"),
                    self.permission_description_for_name(&permission_name),
                );
                row
            })
            .collect()
    }

    pub(crate) fn build_feature_rows(&self) -> Vec<BTreeMap<String, String>> {
        [
            ("error-bulks", "1", "1"),
            ("advanced-channel-chat", "1", "1"),
            ("whisper-echo", "1", "1"),
            ("video", "1", "1"),
            ("screen-sharing", "1", "1"),
            ("music-bots", "1", "1"),
            ("playlists", "1", "1"),
            ("token-actions", "1", "1"),
            ("query-management", "1", "1"),
            ("web-rtc", "1", "1"),
            ("query-notifications", "1", "1"),
            ("channel-tree-updates", "1", "1"),
            ("permission-catalog", "1", "1"),
        ]
        .into_iter()
        .map(|(name, support, version)| {
            let mut row = BTreeMap::new();
            row.insert(String::from("name"), String::from(name));
            row.insert(String::from("support"), String::from(support));
            row.insert(String::from("version"), String::from(version));
            row
        })
        .collect()
    }











    pub fn resolve_text_message_target(
        &self,
        server_id: u32,
        current_channel_id: Option<u32>,
        target_mode: u32,
        requested_channel_id: Option<u32>,
        target_client_id: Option<u64>,
        message: String,
    ) -> std::result::Result<TextMessageTarget, (u32, &'static str)> {
        match target_mode {
            1 => {
                let Some(target_client_id) = target_client_id else {
                    return Err((512, "target is required for private text messages"));
                };

                Ok(TextMessageTarget {
                    target_mode,
                    server_id,
                    channel_id: None,
                    target_client_id: Some(target_client_id),
                    message,
                })
            }
            2 => {
                let channel_id = requested_channel_id
                    .or(current_channel_id)
                    .ok_or((768, "channel target not available"))?;

                if self.snapshot_channel(server_id, channel_id).is_none() {
                    return Err((768, "target channel not found"));
                }

                Ok(TextMessageTarget {
                    target_mode,
                    server_id,
                    channel_id: Some(channel_id),
                    target_client_id: None,
                    message,
                })
            }
            3 => Ok(TextMessageTarget {
                target_mode,
                server_id,
                channel_id: None,
                target_client_id: None,
                message,
            }),
            _ => Err((512, "unsupported targetmode")),
        }
    }

    pub fn text_message_target(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> std::result::Result<TextMessageTarget, (u32, &'static str)> {
        if !session_has_permission_actor(session) {
            return Err((521, "login required"));
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return Err((522, "virtual server selection required"));
        };
        let Some(target_mode) = request
            .named_args
            .get("targetmode")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return Err((512, "targetmode is required"));
        };
        let Some(message) = request.named_args.get("msg").cloned() else {
            return Err((512, "msg is required"));
        };

        self.resolve_text_message_target(
            server_id,
            session.current_channel_id,
            target_mode,
            request
                .named_args
                .get("cid")
                .and_then(|value| value.parse::<u32>().ok()),
            request
                .named_args
                .get("target")
                .and_then(|value| value.parse::<u64>().ok()),
            message,
        )
    }


    pub fn record_private_text_message(
        &mut self,
        server_id: u32,
        sender_database_id: u64,
        sender_unique_id: String,
        sender_name: String,
        target_database_id: u64,
        target_unique_id: String,
        target_name: String,
        message: String,
    ) -> u64 {
        self.record_private_message(
            server_id,
            ConversationParticipant {
                database_id: sender_database_id,
                unique_identifier: sender_unique_id,
                nickname: sender_name,
            },
            ConversationParticipant {
                database_id: target_database_id,
                unique_identifier: target_unique_id,
                nickname: target_name,
            },
            message,
        )
    }


























}
