use super::*;

impl BaselineRuntime {
    pub(crate) fn selected_server(&self, session: &QuerySessionState) -> Option<&VirtualServer> {
        session
            .selected_virtual_server_id
            .and_then(|server_id| self.store.virtual_servers.get(&server_id))
    }

    pub(crate) fn selected_server_mut(&mut self, session: &QuerySessionState) -> Option<&mut VirtualServer> {
        session
            .selected_virtual_server_id
            .and_then(|server_id| self.store.virtual_servers.get_mut(&server_id))
    }


    pub(crate) fn sync_session_client(&mut self, session: &mut QuerySessionState, command_name: &str) {
        if command_name == "quit"
            || (session.authenticated_login.is_none()
                && !session.is_desktop_client
                && session.actor_client_database_id_override.is_none())
        {
            if session.client_id != 0 {
                self.store.online_clients.remove(&session.client_id);
            }
            return;
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            self.store.online_clients.remove(&session.client_id);
            return;
        };

        if session.client_id == 0 {
            session.client_id = self.allocate_query_client_id();
        }

        let channel_id = session
            .current_channel_id
            .or_else(|| self.default_channel_id_for_server(server_id))
            .unwrap_or(1);

        if session.authenticated_login.is_none()
            && (session.is_desktop_client || session.actor_client_database_id_override.is_some())
        {
            if let Some(existing) = self.store.online_clients.get_mut(&session.client_id) {
                existing.nickname = session.effective_nickname();
                existing.last_seen_at = current_unix_timestamp();
                existing.away = session.client_away;
                existing.away_message = session.client_away_message.clone();
                existing.input_muted = session.client_input_muted;
                existing.output_muted = session.client_output_muted;
                existing.server_id = server_id;
                existing.channel_id = channel_id;
                if session.whisper_targets.is_some() {
                    existing.whisper_targets = session.whisper_targets.clone();
                }
                return;
            }
        }

        let (database_id, unique_identifier, server_groups, client_type, platform, version) = if session.is_desktop_client {
            (
                session.actor_client_database_id_override.unwrap_or(session.client_id + 1000),
                format!("desktop-{}", session.client_id),
                vec![8],
                0, // client_type = 0 (Voice/Desktop)
                String::from("desktop"),
                String::from("BlackTeaSpeak Desktop"),
            )
        } else {
            let Some(login_name) = session.authenticated_login.as_ref() else {
                return;
            };
            let Some(account) = self.store.query_accounts.get(login_name) else {
                return;
            };
            (
                account.client_database_id.unwrap_or_else(|| session.client_id + 1000),
                self.query_account_unique_identifier(account),
                account.server_groups.clone(),
                1, // client_type = 1 (Query)
                String::from("compat-rust"),
                String::from("BlackTeaSpeak Compat Query"),
            )
        };

        let connection_ip = if session.connection_ip.is_empty() {
            String::from("127.0.0.1")
        } else {
            session.connection_ip.clone()
        };

        let existing_extra_properties = self
            .store
            .online_clients
            .get(&session.client_id)
            .map(|client| client.extra_properties.clone())
            .unwrap_or_default();

        self.store.online_clients.insert(
            session.client_id,
            OnlineClient {
                id: session.client_id,
                database_id,
                unique_identifier,
                nickname: session.effective_nickname(),
                last_seen_at: current_unix_timestamp(),
                away: session.client_away,
                away_message: session.client_away_message.clone(),
                input_muted: session.client_input_muted,
                output_muted: session.client_output_muted,
                server_id,
                channel_id,
                client_type,
                version,
                platform,
                country: String::from("ZZ"),
                connection_ip,
                server_groups,
                connected_at: self
                    .store
                    .online_clients
                    .get(&session.client_id)
                    .map(|client| client.connected_at)
                    .unwrap_or_else(current_unix_timestamp),
                whisper_targets: session.whisper_targets.clone(),
                extra_properties: existing_extra_properties,
            },
        );
    }



    pub(crate) fn restore_notification_subscriptions(
        &self,
        login_name: &str,
        server_id: u32,
    ) -> Vec<NotificationSubscription> {
        self.session_snapshots
            .get(login_name)
            .map(|snapshot| {
                snapshot
                    .notification_subscriptions
                    .iter()
                    .filter_map(|subscription| {
                        let event = NotificationEventKind::parse(&subscription.event)?;
                        let channel_id = if matches!(
                            event,
                            NotificationEventKind::Channel | NotificationEventKind::TextChannel
                        ) {
                            subscription
                                .channel_id
                                .filter(|channel_id| self.channel_exists(server_id, *channel_id))
                        } else {
                            None
                        };
                        Some(NotificationSubscription { event, channel_id })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    pub fn default_channel_id_for_server(&self, server_id: u32) -> Option<u32> {
        self.store.channels.get(&server_id).and_then(|channels| {
            channels
                .iter()
                .find(|channel| channel.flag_default)
                .or_else(|| {
                    channels
                        .iter()
                        .find(|channel| channel.parent_id == 0 && channel.order == 0)
                })
                .or_else(|| channels.iter().min_by_key(|channel| channel.id))
                .map(|channel| channel.id)
        })
    }

    pub(crate) fn channel_exists(&self, server_id: u32, channel_id: u32) -> bool {
        self.store
            .channels
            .get(&server_id)
            .is_some_and(|channels| channels.iter().any(|channel| channel.id == channel_id))
    }

    pub(crate) fn channel_by_id(&self, server_id: u32, channel_id: u32) -> Option<&Channel> {
        self.store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
    }

    pub(crate) fn conversation_id_exists(&self, server_id: u32, conversation_id: u32) -> bool {
        self.store.virtual_servers.contains_key(&server_id)
            && (conversation_id == 0 || self.channel_by_id(server_id, conversation_id).is_some())
    }

    pub(crate) fn latest_conversation_timestamp(&self, server_id: u32, conversation_id: u32) -> u64 {
        self.conversation_messages(server_id, conversation_id)
            .into_iter()
            .map(|message| message.timestamp)
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn conversation_messages(
        &self,
        server_id: u32,
        conversation_id: u32,
    ) -> Vec<&ConversationMessage> {
        self.store
            .conversation_messages
            .get(&server_id)
            .into_iter()
            .flat_map(|messages| messages.iter())
            .filter(|message| message.conversation_id == conversation_id)
            .collect()
    }

    pub(crate) fn private_conversation_messages(
        &self,
        server_id: u32,
        left_database_id: u64,
        right_database_id: u64,
    ) -> Vec<&PrivateConversationMessage> {
        self.store
            .private_messages
            .get(&server_id)
            .into_iter()
            .flat_map(|messages| messages.iter())
            .filter(|message| {
                (message.sender_database_id == left_database_id
                    && message.target_database_id == right_database_id)
                    || (message.sender_database_id == right_database_id
                        && message.target_database_id == left_database_id)
            })
            .collect()
    }

    pub(crate) fn render_client_row(
        &self,
        client: &OnlineClient,
        request: &CommandRequest,
        detailed: bool,
    ) -> BTreeMap<String, String> {
        let include_all = detailed || request.flags.contains("info");
        let mut row = BTreeMap::new();
        row.insert(String::from("clid"), client.id.to_string());
        row.insert(String::from("cid"), client.channel_id.to_string());
        row.insert(
            String::from("client_database_id"),
            client.database_id.to_string(),
        );
        row.insert(String::from("client_nickname"), client.nickname.clone());
        row.insert(String::from("client_type"), client.client_type.to_string());

        if detailed || request.flags.contains("uid") {
            row.insert(
                String::from("client_unique_identifier"),
                client.unique_identifier.clone(),
            );
        }
        if detailed || request.flags.contains("away") {
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
        }
        if detailed || request.flags.contains("voice") {
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
            row.insert(String::from("client_flag_talking"), String::from("0"));
            let talk_power = client
                .extra_properties
                .get("client_talk_power")
                .cloned()
                .unwrap_or_else(|| {
                    self.client_talk_power(client.server_id, client.channel_id, client.database_id)
                        .to_string()
                });
            row.insert(String::from("client_talk_power"), talk_power);
            row.insert(
                String::from("client_is_talker"),
                client
                    .extra_properties
                    .get("client_is_talker")
                    .cloned()
                    .unwrap_or_else(|| String::from("0")),
            );
            row.insert(
                String::from("client_talk_request"),
                client
                    .extra_properties
                    .get("client_talk_request")
                    .cloned()
                    .unwrap_or_else(|| String::from("0")),
            );
            row.insert(
                String::from("client_talk_request_msg"),
                client
                    .extra_properties
                    .get("client_talk_request_msg")
                    .cloned()
                    .unwrap_or_default(),
            );
            row.insert(
                String::from("client_is_channel_commander"),
                client
                    .extra_properties
                    .get("client_is_channel_commander")
                    .cloned()
                    .unwrap_or_else(|| String::from("0")),
            );
        }
        if detailed || request.flags.contains("times") {
            row.insert(String::from("connection_connected_time"), String::from("0"));
            row.insert(String::from("client_idle_time"), String::from("0"));
        }
        if detailed || request.flags.contains("groups") {
            row.insert(
                String::from("client_servergroups"),
                client
                    .server_groups
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if include_all {
            row.insert(String::from("client_version"), client.version.clone());
            row.insert(String::from("client_platform"), client.platform.clone());
            let description = client
                .extra_properties
                .get("client_description")
                .cloned()
                .or_else(|| {
                    self.store
                        .clients
                        .get(&client.database_id)
                        .map(|c| c.description.clone())
                })
                .unwrap_or_default();
            row.insert(String::from("client_description"), description);
        }
        if detailed || request.flags.contains("country") {
            row.insert(String::from("client_country"), client.country.clone());
        }
        if detailed || request.flags.contains("ip") {
            row.insert(
                String::from("connection_client_ip"),
                client.connection_ip.clone(),
            );
        }
        if detailed || request.flags.contains("badges") {
            row.insert(String::from("client_badges"), String::from("compat"));
        }
        
        let avatar = client
            .extra_properties
            .get("client_flag_avatar")
            .cloned()
            .or_else(|| {
                self.store
                    .clients
                    .get(&client.database_id)
                    .map(|db_client| db_client.client_flag_avatar.clone())
            })
            .unwrap_or_default();
        row.insert(String::from("client_flag_avatar"), avatar);

        row
    }

    pub(crate) fn query_account_unique_identifier(&self, account: &QueryAccount) -> String {
        stable_query_client_unique_identifier(&account.login_name)
    }

    pub(crate) fn query_account_matches_server(&self, account: &QueryAccount, server_id: u32) -> bool {
        match account.server_id {
            Some(account_server_id) => account_server_id == server_id,
            None => true,
        }
    }

    pub(crate) fn lookup_client_identity_by_uid(
        &self,
        server_id: u32,
        client_uid: &str,
    ) -> Option<(String, u64, String)> {
        let normalized_uid = client_uid.strip_prefix("query:").unwrap_or(client_uid);
        self.store
            .online_clients
            .values()
            .find(|client| {
                client.server_id == server_id
                    && (client.unique_identifier == client_uid
                        || client.unique_identifier == normalized_uid)
            })
            .map(|client| {
                (
                    client.unique_identifier.clone(),
                    client.database_id,
                    client.nickname.clone(),
                )
            })
            .or_else(|| {
                self.store
                    .query_accounts
                    .values()
                    .find(|account| {
                        self.query_account_matches_server(account, server_id)
                            && (self.query_account_unique_identifier(account) == client_uid
                                || self.query_account_unique_identifier(account) == normalized_uid)
                    })
                    .and_then(|account| {
                        account.client_database_id.map(|client_database_id| {
                            (
                                self.query_account_unique_identifier(account),
                                client_database_id,
                                account.login_name.clone(),
                            )
                        })
                    })
            })
            .or_else(|| {
                self.store
                    .clients
                    .values()
                    .find(|client| {
                        client.unique_identifier == client_uid
                            || client.unique_identifier == normalized_uid
                    })
                    .map(|client| {
                        (
                            client.unique_identifier.clone(),
                            client.database_id,
                            client.nickname.clone(),
                        )
                    })
            })
            .or_else(|| {
                self.store
                    .client_permissions
                    .iter()
                    .find(|target| target.client_unique_identifier == client_uid)
                    .and_then(|target| {
                        (!target.client_unique_identifier.is_empty()).then(|| {
                            (
                                target.client_unique_identifier.clone(),
                                target.client_database_id,
                                if target.client_nickname.is_empty() {
                                    target.client_unique_identifier.clone()
                                } else {
                                    target.client_nickname.clone()
                                },
                            )
                        })
                    })
            })
            .or_else(|| {
                looks_like_blackteaspeak_unique_id(client_uid).then(|| {
                    (
                        client_uid.to_string(),
                        stable_web_client_database_id(client_uid),
                        client_uid.to_string(),
                    )
                })
            })
    }

    pub(crate) fn lookup_client_identity_by_dbid(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<(String, u64, String)> {
        self.store
            .query_accounts
            .values()
            .find(|account| {
                self.query_account_matches_server(account, server_id)
                    && account.client_database_id == Some(client_database_id)
            })
            .map(|account| {
                (
                    self.query_account_unique_identifier(account),
                    client_database_id,
                    account.login_name.clone(),
                )
            })
            .or_else(|| {
                self.store
                    .online_clients
                    .values()
                    .find(|client| {
                        client.server_id == server_id && client.database_id == client_database_id
                    })
                    .map(|client| {
                        (
                            client.unique_identifier.clone(),
                            client.database_id,
                            client.nickname.clone(),
                        )
                    })
            })
            .or_else(|| {
                self.store.clients.get(&client_database_id).map(|client| {
                    (
                        client.unique_identifier.clone(),
                        client.database_id,
                        client.nickname.clone(),
                    )
                })
            })
            .or_else(|| {
                self.store
                    .client_permissions
                    .iter()
                    .find(|target| target.client_database_id == client_database_id)
                    .and_then(|target| {
                        (!target.client_unique_identifier.is_empty()).then(|| {
                            (
                                target.client_unique_identifier.clone(),
                                target.client_database_id,
                                if target.client_nickname.is_empty() {
                                    target.client_unique_identifier.clone()
                                } else {
                                    target.client_nickname.clone()
                                },
                            )
                        })
                    })
            })
            .or_else(|| self.private_message_identity_by_dbid(server_id, client_database_id))
    }

    pub(crate) fn private_message_identity_by_dbid(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<(String, u64, String)> {
        self.store
            .private_messages
            .get(&server_id)
            .and_then(|messages| {
                messages.iter().rev().find_map(|message| {
                    if message.sender_database_id == client_database_id {
                        Some((
                            message.sender_unique_id.clone(),
                            client_database_id,
                            message.sender_name.clone(),
                        ))
                    } else if message.target_database_id == client_database_id {
                        Some((
                            message.target_unique_id.clone(),
                            client_database_id,
                            message.target_name.clone(),
                        ))
                    } else {
                        None
                    }
                })
            })
    }

    pub(crate) fn online_client_by_id_in_server(
        &self,
        server_id: u32,
        client_id: u64,
    ) -> Option<&OnlineClient> {
        self.store
            .online_clients
            .get(&client_id)
            .filter(|client| client.server_id == server_id)
    }

    pub(crate) fn client_count_in_channel(&self, server_id: u32, channel_id: u32) -> u32 {
        self.store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id && client.channel_id == channel_id)
            .count() as u32
    }

    pub(crate) fn family_client_count_in_channel(&self, server_id: u32, channel_id: u32) -> u32 {
        let Some(channels) = self.store.channels.get(&server_id) else {
            return 0;
        };
        self.store
            .online_clients
            .values()
            .filter(|client| {
                client.server_id == server_id
                    && (client.channel_id == channel_id
                        || channel_is_descendant(channels, client.channel_id, channel_id))
            })
            .count() as u32
    }

    pub fn client_talk_power(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> i64 {
        self.effective_permissions_for_client_in_channel_context(
            server_id,
            channel_id,
            client_database_id,
        )
        .map(|perms| permission_value_or_default(&perms, &["i_client_talk_power"]))
        .unwrap_or(0)
    }

    pub fn client_can_speak_in_channel(&self, server_id: u32, client_id: u64) -> bool {
        let Some(client) = self.online_client_by_id_in_server(server_id, client_id) else {
            return true;
        };
        if client
            .extra_properties
            .get("client_type_exact")
            .is_some_and(|v| v == "4")
            || client.unique_identifier.starts_with("compat-musicbot-")
        {
            return true;
        }
        if client
            .extra_properties
            .get("client_is_talker")
            .is_some_and(|v| runtime_bool_flag(v))
        {
            return true;
        }
        let needed_talk_power = self
            .channel_by_id(server_id, client.channel_id)
            .map(|c| c.needed_talk_power())
            .unwrap_or(0);
        if needed_talk_power <= 0 {
            return true;
        }
        let talk_power =
            self.client_talk_power(server_id, client.channel_id, client.database_id);
        talk_power >= needed_talk_power
    }

    pub(crate) fn client_count_in_server(&self, server_id: u32) -> u32 {
        self.store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .count() as u32
    }
}
