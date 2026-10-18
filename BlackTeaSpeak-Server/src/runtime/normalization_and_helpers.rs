use super::*;

impl BaselineRuntime {
    pub(crate) fn allocate_query_client_id(&mut self) -> u64 {
        while self
            .store
            .online_clients
            .contains_key(&self.store.next_query_client_id)
        {
            self.store.next_query_client_id = self.store.next_query_client_id.saturating_add(1);
        }
        let next_id = self.store.next_query_client_id;
        self.store.next_query_client_id = self.store.next_query_client_id.saturating_add(1);
        next_id
    }

    pub(crate) fn allocate_client_database_id(&mut self) -> u64 {
        let next_id = self.store.next_client_database_id;
        self.store.next_client_database_id = self.store.next_client_database_id.saturating_add(1);
        next_id
    }

    pub(crate) fn default_server_groups_for_new_query_account(&self) -> Vec<u32> {
        default_server_groups_for_login_with_availability(
            "query-account",
            self.store.server_groups.contains_key(&6),
            self.store.server_groups.contains_key(&7),
        )
    }

    pub(crate) fn normalize_query_account_groups(&mut self) {
        let has_admin_group = self.store.server_groups.contains_key(&6);
        let has_guest_group = self.store.server_groups.contains_key(&7);
        let valid_group_ids = self
            .store
            .server_groups
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();

        for account in self.store.query_accounts.values_mut() {
            account
                .server_groups
                .retain(|group_id| valid_group_ids.contains(group_id));
            if account.server_groups.is_empty() {
                account.server_groups = default_server_groups_for_login_with_availability(
                    &account.login_name,
                    has_admin_group,
                    has_guest_group,
                );
            }
            account.server_groups.sort_unstable();
            account.server_groups.dedup();
        }
    }

    pub(crate) fn normalize_online_client_groups(&mut self) {
        let valid_group_ids = self
            .store
            .server_groups
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();

        for client in self.store.online_clients.values_mut() {
            client
                .server_groups
                .retain(|group_id| valid_group_ids.contains(group_id));
            client.server_groups.sort_unstable();
            client.server_groups.dedup();
        }
    }

    pub(crate) fn normalize_channel_group_assignments(&mut self) {
        let valid_group_ids = self
            .store
            .channel_groups
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let valid_channel_ids = self
            .store
            .channels
            .values()
            .flat_map(|channels| channels.iter().map(|channel| channel.id))
            .collect::<BTreeSet<_>>();
        let valid_client_database_ids = self.known_client_database_ids();
        let mut normalized = BTreeMap::new();

        for assignment in std::mem::take(&mut self.store.channel_group_assignments) {
            if !valid_group_ids.contains(&assignment.channel_group_id)
                || !valid_channel_ids.contains(&assignment.channel_id)
                || !valid_client_database_ids.contains(&assignment.client_database_id)
            {
                continue;
            }

            normalized.insert(
                (assignment.channel_id, assignment.client_database_id),
                assignment.channel_group_id,
            );
        }
        self.store.channel_group_assignments = normalized
            .into_iter()
            .map(
                |((channel_id, client_database_id), channel_group_id)| ChannelGroupAssignment {
                    channel_id,
                    client_database_id,
                    channel_group_id,
                },
            )
            .collect();
    }

    pub(crate) fn normalize_client_permissions(&mut self) {
        let mut normalized = BTreeMap::new();

        for target in std::mem::take(&mut self.store.client_permissions) {
            if target.permissions.is_empty() {
                continue;
            }

            let entry = normalized
                .entry(target.client_database_id)
                .or_insert_with(|| ClientPermissionTarget {
                    server_id: 0,
                    client_database_id: target.client_database_id,
                    client_unique_identifier: String::new(),
                    client_nickname: String::new(),
                    permissions: BTreeMap::new(),
                });

            if entry.client_unique_identifier.is_empty()
                && !target.client_unique_identifier.is_empty()
            {
                entry.client_unique_identifier = target.client_unique_identifier;
            }
            if entry.client_nickname.is_empty() && !target.client_nickname.is_empty() {
                entry.client_nickname = target.client_nickname;
            }
            entry.permissions.extend(target.permissions);
        }

        self.store.client_permissions = normalized.into_values().collect();
    }

    pub(crate) fn normalize_channel_client_permissions(&mut self) {
        let valid_channel_ids = self
            .store
            .channels
            .values()
            .flat_map(|channels| channels.iter().map(|channel| channel.id))
            .collect::<BTreeSet<_>>();
        let valid_client_database_ids = self.known_client_database_ids();
        let mut normalized = BTreeMap::new();

        for target in std::mem::take(&mut self.store.channel_client_permissions) {
            if !valid_channel_ids.contains(&target.channel_id)
                || !valid_client_database_ids.contains(&target.client_database_id)
                || target.permissions.is_empty()
            {
                continue;
            }

            normalized
                .entry((target.channel_id, target.client_database_id))
                .or_insert_with(BTreeMap::new)
                .extend(target.permissions);
        }

        self.store.channel_client_permissions = normalized
            .into_iter()
            .map(
                |((channel_id, client_database_id), permissions)| ChannelClientPermissionTarget {
                    channel_id,
                    client_database_id,
                    permissions,
                },
            )
            .collect();
    }

    pub(crate) fn next_server_group_id(&self) -> u32 {
        self.store
            .server_groups
            .keys()
            .copied()
            .max()
            .map(|group_id| group_id.saturating_add(1))
            .unwrap_or(1)
    }

    pub(crate) fn next_channel_group_id(&self) -> u32 {
        self.store
            .channel_groups
            .keys()
            .copied()
            .max()
            .map(|group_id| group_id.saturating_add(1))
            .unwrap_or(1)
    }

    pub(crate) fn server_group_in_use(&self, group_id: u32) -> bool {
        self.store
            .query_accounts
            .values()
            .any(|account| account.server_groups.contains(&group_id))
            || self
                .store
                .online_clients
                .values()
                .any(|client| client.server_groups.contains(&group_id))
    }

    pub(crate) fn server_group_ids_by_auto_type(&self, auto_update_type: i64) -> Vec<u32> {
        self.store
            .server_groups
            .iter()
            .filter_map(|(group_id, group)| {
                group
                    .permissions
                    .get("i_group_auto_update_type")
                    .filter(|assignment| assignment.value == auto_update_type)
                    .map(|_| *group_id)
            })
            .collect()
    }

    pub(crate) fn channel_group_in_use(&self, group_id: u32) -> bool {
        self.store
            .channel_group_assignments
            .iter()
            .any(|assignment| assignment.channel_group_id == group_id)
    }

    pub(crate) fn known_client_database_ids(&self) -> BTreeSet<u64> {
        let mut client_database_ids = self
            .store
            .query_accounts
            .values()
            .filter_map(|account| account.client_database_id)
            .collect::<BTreeSet<_>>();
        client_database_ids.extend(self.store.clients.keys().copied());
        client_database_ids.extend(
            self.store
                .custom_client_properties
                .keys()
                .map(|(_, dbid)| *dbid),
        );
        client_database_ids.extend(
            self.store
                .online_clients
                .values()
                .map(|client| client.database_id),
        );
        client_database_ids.extend(
            self.store
                .client_permissions
                .iter()
                .map(|target| target.client_database_id),
        );
        client_database_ids
    }

    pub(crate) fn client_database_id_exists(&self, client_database_id: u64) -> bool {
        self.query_account_by_cldbid(client_database_id).is_some()
            || self.store.clients.contains_key(&client_database_id)
            || self
                .store
                .custom_client_properties
                .keys()
                .any(|(_, dbid)| *dbid == client_database_id)
            || self
                .store
                .client_permissions
                .iter()
                .any(|target| target.client_database_id == client_database_id)
            || self
                .store
                .online_clients
                .values()
                .any(|client| client.database_id == client_database_id)
    }

    pub(crate) fn online_client_by_cldbid(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<&OnlineClient> {
        self.store.online_clients.values().find(|client| {
            client.server_id == server_id && client.database_id == client_database_id
        })
    }

    pub(crate) fn online_client_by_cldbid_mut(
        &mut self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<&mut OnlineClient> {
        self.store.online_clients.values_mut().find(|client| {
            client.server_id == server_id && client.database_id == client_database_id
        })
    }

    pub(crate) fn query_account_by_cldbid(&self, client_database_id: u64) -> Option<&QueryAccount> {
        self.store
            .query_accounts
            .values()
            .find(|account| account.client_database_id == Some(client_database_id))
    }

    pub(crate) fn query_account_by_cldbid_mut(
        &mut self,
        client_database_id: u64,
    ) -> Option<&mut QueryAccount> {
        self.store
            .query_accounts
            .values_mut()
            .find(|account| account.client_database_id == Some(client_database_id))
    }

    pub(crate) fn next_token_id(&mut self) -> u32 {
        let token_id = self
            .store
            .next_token_id
            .max(next_token_id_seed(&self.store.tokens));
        self.store.next_token_id = token_id.saturating_add(1);
        token_id
    }

    pub(crate) fn next_token_action_id(&mut self) -> u32 {
        let action_id = self
            .store
            .next_token_action_id
            .max(next_token_action_id_seed(&self.store.tokens));
        self.store.next_token_action_id = action_id.saturating_add(1);
        action_id
    }

    pub(crate) fn next_conversation_timestamp(&mut self) -> u64 {
        let timestamp = current_unix_timestamp_millis().max(self.store.next_conversation_timestamp);
        self.store.next_conversation_timestamp = timestamp.saturating_add(1);
        timestamp
    }

    pub(crate) fn query_session_database_id(&self, session: &QuerySessionState) -> u64 {
        session
            .authenticated_login
            .as_ref()
            .and_then(|login| self.store.query_accounts.get(login))
            .and_then(|account| account.client_database_id)
            .unwrap_or(0)
    }

    pub(crate) fn query_session_participant(&self, session: &QuerySessionState) -> ConversationParticipant {
        ConversationParticipant {
            database_id: self.query_session_database_id(session),
            unique_identifier: self.query_session_unique_identifier(session),
            nickname: session
                .authenticated_login
                .clone()
                .unwrap_or_else(|| String::from("Query")),
        }
    }



    pub(crate) fn resolve_token_id(
        &self,
        request: &CommandRequest,
        server_id: u32,
    ) -> std::result::Result<u32, QueryResponse> {
        if let Some(token_id_value) = request.named_args.get("token_id") {
            let Some(token_id) = token_id_value.parse::<u32>().ok() else {
                return Err(QueryResponse::error(512, "token_id must be an integer"));
            };
            let Some(token) = self.store.tokens.get(&token_id) else {
                return Err(QueryResponse::error(768, "token not found"));
            };
            if token.server_id != server_id {
                return Err(QueryResponse::error(768, "token not found"));
            }
            if let Some(token_value) = request.named_args.get("token")
                && token.token != *token_value
            {
                return Err(QueryResponse::error(768, "token not found"));
            }
            return Ok(token_id);
        }

        let Some(token_value) = request.named_args.get("token") else {
            return Err(QueryResponse::error(512, "token_id or token is required"));
        };

        self.store
            .tokens
            .iter()
            .find_map(|(token_id, token)| {
                (token.server_id == server_id && token.token == *token_value).then_some(*token_id)
            })
            .ok_or_else(|| QueryResponse::error(768, "token not found"))
    }

    pub(crate) fn token_action_groups(&self, request: &CommandRequest) -> Vec<BTreeMap<String, String>> {
        request
            .option_groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .filter(|(key, _)| key.starts_with("action_"))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .filter(|group| !group.is_empty())
            .collect()
    }

    pub(crate) fn parse_token_action_mutations(
        &self,
        request: &CommandRequest,
    ) -> std::result::Result<Vec<ParsedTokenActionMutation>, QueryResponse> {
        let groups = self.token_action_groups(request);
        let mut mutations = Vec::new();

        if groups.is_empty() {
            let token_type_raw = request
                .named_args
                .get("tokentype")
                .or_else(|| request.named_args.get("token_type"));
            let token_id1_raw = request
                .named_args
                .get("tokenid1")
                .or_else(|| request.named_args.get("token_id1"));
            if token_type_raw.is_some() || token_id1_raw.is_some() {
                let token_type = match token_type_raw {
                    Some(v) => match v.parse::<u32>() {
                        Ok(parsed) => parsed,
                        Err(_) => {
                            return Err(QueryResponse::error(512, "tokentype must be an integer"))
                        }
                    },
                    None => 0,
                };
                let action_id1 = match token_id1_raw {
                    Some(v) => match v.parse::<u32>() {
                        Ok(parsed) => parsed,
                        Err(_) => {
                            return Err(QueryResponse::error(512, "tokenid1 must be an integer"))
                        }
                    },
                    None => return Err(QueryResponse::error(512, "tokenid1 is required")),
                };
                let action_id2 = match request
                    .named_args
                    .get("tokenid2")
                    .or_else(|| request.named_args.get("token_id2"))
                {
                    Some(v) => match v.parse::<u32>() {
                        Ok(parsed) => parsed,
                        Err(_) => {
                            return Err(QueryResponse::error(512, "tokenid2 must be an integer"))
                        }
                    },
                    None => 0,
                };
                let action_type = if token_type == 1 { 1 } else { 2 };
                mutations.push(ParsedTokenActionMutation::Add {
                    action_type,
                    action_id1,
                    action_id2,
                    action_text: String::new(),
                });
                return Ok(mutations);
            }
        }

        for group in groups {
            let action_id = if let Some(value) = group.get("action_id") {
                let Some(action_id) = value.parse::<u32>().ok() else {
                    return Err(QueryResponse::error(512, "action_id must be an integer"));
                };
                Some(action_id)
            } else {
                None
            };

            let has_action_body = group.contains_key("action_type")
                || group.contains_key("action_id1")
                || group.contains_key("action_id2")
                || group.contains_key("action_text");

            if !has_action_body {
                let Some(action_id) = action_id else {
                    return Err(QueryResponse::error(
                        512,
                        "action_type or action_id is required",
                    ));
                };
                mutations.push(ParsedTokenActionMutation::Remove { action_id });
                continue;
            }

            let Some(action_type) = group
                .get("action_type")
                .and_then(|value| value.parse::<u32>().ok())
            else {
                return Err(QueryResponse::error(512, "action_type must be an integer"));
            };

            let action_id1 = if let Some(value) = group.get("action_id1") {
                let Some(action_id1) = value.parse::<u32>().ok() else {
                    return Err(QueryResponse::error(512, "action_id1 must be an integer"));
                };
                action_id1
            } else {
                0
            };
            let action_id2 = if let Some(value) = group.get("action_id2") {
                let Some(action_id2) = value.parse::<u32>().ok() else {
                    return Err(QueryResponse::error(512, "action_id2 must be an integer"));
                };
                action_id2
            } else {
                0
            };
            let action_text = group.get("action_text").cloned().unwrap_or_default();

            if let Some(action_id) = action_id {
                mutations.push(ParsedTokenActionMutation::Update {
                    action_id,
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                });
            } else {
                mutations.push(ParsedTokenActionMutation::Add {
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                });
            }
        }

        Ok(mutations)
    }

    pub(crate) fn permission_option_groups(
        &self,
        request: &CommandRequest,
        global_keys: &[&str],
    ) -> Vec<BTreeMap<String, String>> {
        if request.option_groups.is_empty() {
            return vec![request.named_args.clone()];
        }

        request
            .option_groups
            .iter()
            .map(|group| {
                let mut merged = BTreeMap::new();
                for global_key in global_keys {
                    if let Some(value) = request.named_args.get(*global_key) {
                        merged.insert((*global_key).to_string(), value.clone());
                    }
                }
                merged.extend(group.clone());
                merged
            })
            .collect()
    }

    pub(crate) fn permoverview_requested_permissions(
        &self,
        request: &CommandRequest,
    ) -> std::result::Result<Option<BTreeSet<String>>, QueryResponse> {
        let groups = self.permission_option_groups(request, &["cid", "cldbid"]);
        let mut permission_names = BTreeSet::new();
        let mut has_filter = false;

        for group in groups {
            if let Some(permission_name) = group.get("permsid") {
                permission_names.insert(permission_name.clone());
                has_filter = true;
                continue;
            }

            if let Some(permission_id_value) = group.get("permid") {
                let Some(permission_id) = permission_id_value.parse::<u32>().ok() else {
                    return Err(QueryResponse::error(512, "permid must be an integer"));
                };
                if permission_id == 0 {
                    return Ok(None);
                }
                let Some(permission_name) = self.permission_name_for_id(permission_id) else {
                    return Err(QueryResponse::error(
                        768,
                        format!("permission {} not found", permission_id),
                    ));
                };
                permission_names.insert(permission_name);
                has_filter = true;
            }
        }

        if has_filter {
            Ok(Some(permission_names))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn permission_filter_matches(
        &self,
        permission_filter: &Option<BTreeSet<String>>,
        permission_name: &str,
    ) -> bool {
        permission_filter
            .as_ref()
            .is_none_or(|names| names.contains(permission_name))
    }

    pub(crate) fn cleanup_temporary_channel(
        &mut self,
        server_id: u32,
        channel_id: u32,
    ) -> Option<TemporaryChannelCleanup> {
        let channel_snapshot = self.snapshot_channel(server_id, channel_id)?;
        if channel_snapshot.kind != ChannelKind::Temporary {
            return None;
        }

        let mut removed_client = None;
        let clients_in_channel = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id && client.channel_id == channel_id)
            .map(|client| client.id)
            .collect::<Vec<_>>();

        let all_are_music_bots = !clients_in_channel.is_empty()
            && clients_in_channel.iter().all(|&cid| {
                self.online_client_snapshot(server_id, cid)
                    .is_some_and(|snapshot| snapshot.client_type_exact == 4)
            });

        if all_are_music_bots {
            for bot_client_id in clients_in_channel {
                let bot_snapshot = self.online_client_snapshot(server_id, bot_client_id);
                if let Some(bot_id) = self.music_bot_id_by_client(server_id, bot_client_id) {
                    if let Some(mut removed_bot) = self.store.music_bots.remove(&bot_id) {
                        Self::stop_music_bot_playback(&mut removed_bot);
                    }
                }
                self.store.online_clients.remove(&bot_client_id);
                if removed_client.is_none() {
                    removed_client = bot_snapshot;
                }
            }
            self.persist_state_if_configured();
        }

        let removed_channel = if self.client_count_in_channel(server_id, channel_id) == 0
            && self.store.channels.get(&server_id).is_some_and(|channels| {
                !channels.iter().any(|channel| channel.parent_id == channel_id)
            })
        {
            self.remove_channel_without_session_reassign(server_id, channel_id)
                .map(|_| channel_snapshot)
        } else {
            None
        };

        if removed_client.is_none() && removed_channel.is_none() {
            return None;
        }

        Some(TemporaryChannelCleanup {
            removed_client,
            removed_channel,
        })
    }

    pub(crate) fn remove_channel_without_session_reassign(
        &mut self,
        server_id: u32,
        channel_id: u32,
    ) -> Option<()> {
        let channels = self.store.channels.get_mut(&server_id)?;
        let channel_index = channels.iter().position(|channel| channel.id == channel_id)?;
        let parent_id = channels[channel_index].parent_id;
        channels.remove(channel_index);
        let _ = self.db.delete_channel(server_id, channel_id);
        let sibling_ids = ordered_sibling_ids(channels, parent_id, None);
        relink_sibling_orders(channels, parent_id, &sibling_ids);
        self.store
            .channel_group_assignments
            .retain(|assignment| assignment.channel_id != channel_id);
        self.store
            .channel_client_permissions
            .retain(|target| target.channel_id != channel_id);
        if let Some(messages) = self.store.conversation_messages.get_mut(&server_id) {
            messages.retain(|message| message.conversation_id != channel_id);
        }
        Some(())
    }
    pub(crate) fn append_server_log(
        &mut self,
        server_id: u32,
        level: u32,
        channel: &str,
        message: impl Into<String>,
    ) {
        let id = self.store.next_log_id.max(1);
        self.store.next_log_id = id.saturating_add(1);
        self.store.server_logs.push(ServerLogEntry {
            id,
            server_id,
            timestamp: current_unix_timestamp(),
            level: level.clamp(1, 4),
            channel: channel.to_string(),
            message: message.into(),
        });
    }
}
