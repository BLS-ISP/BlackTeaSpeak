use super::*;
use crate::query::{CommandRequest, QueryResponse};

impl BaselineRuntime {
    pub fn persist_state_if_configured(&self) {
        self.persist_state_best_effort();
    }

    pub(crate) fn load_persisted_state(&mut self) -> Result<()> {
        let Some(state_store) = &self.state_store else {
            return Ok(());
        };
        let Some(state) = state_store.load()? else {
            return Ok(());
        };
        let schema_version = state.schema_version;

        self.store.query_accounts = state
            .query_accounts
            .into_iter()
            .map(|(login_name, account)| {
                (
                    login_name,
                    QueryAccount {
                        login_name: account.login_name,
                        password: account.password,
                        server_id: account.server_id,
                        client_database_id: account.client_database_id,
                        server_groups: account.server_groups,
                        permissions: account
                            .permissions
                            .into_iter()
                            .map(|(permission_name, assignment)| {
                                (
                                    permission_name,
                                    PermissionAssignment {
                                        value: assignment.value,
                                        negated: assignment.negated,
                                        skipped: assignment.skipped,
                                    },
                                )
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        if !state.server_groups.is_empty() {
            self.store.server_groups = state
                .server_groups
                .into_iter()
                .map(|(group_id, group)| {
                    (
                        group_id,
                        ServerGroup {
                            id: group.id,
                            name: group.name,
                            group_type: group.group_type,
                            icon_id: group.icon_id,
                            save_db: group.save_db,
                            permissions: group
                                .permissions
                                .into_iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name,
                                        PermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect();
        }
        if schema_version >= 4 {
            self.store.channel_groups = state
                .channel_groups
                .into_iter()
                .map(|(group_id, group)| {
                    (
                        group_id,
                        ChannelGroup {
                            id: group.id,
                            name: group.name,
                            group_type: group.group_type,
                            icon_id: group.icon_id,
                            save_db: group.save_db,
                            permissions: group
                                .permissions
                                .into_iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name,
                                        PermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect();
            self.store.channel_group_assignments = state
                .channel_group_assignments
                .into_iter()
                .map(|assignment| ChannelGroupAssignment {
                    channel_id: assignment.channel_id,
                    client_database_id: assignment.client_database_id,
                    channel_group_id: assignment.channel_group_id,
                })
                .collect();
            self.store.channel_client_permissions = state
                .channel_client_permissions
                .into_iter()
                .map(|target| ChannelClientPermissionTarget {
                    channel_id: target.channel_id,
                    client_database_id: target.client_database_id,
                    permissions: target
                        .permissions
                        .into_iter()
                        .map(|(permission_name, assignment)| {
                            (
                                permission_name,
                                PermissionAssignment {
                                    value: assignment.value,
                                    negated: assignment.negated,
                                    skipped: assignment.skipped,
                                },
                            )
                        })
                        .collect(),
                })
                .collect();
        }
        self.store.client_permissions = state
            .client_permissions
            .into_iter()
            .map(|target| ClientPermissionTarget {
                server_id: 0,
                client_database_id: target.client_database_id,
                client_unique_identifier: target.client_unique_identifier,
                client_nickname: target.client_nickname,
                permissions: target
                    .permissions
                    .into_iter()
                    .map(|(permission_name, assignment)| {
                        (
                            permission_name,
                            PermissionAssignment {
                                value: assignment.value,
                                negated: assignment.negated,
                                skipped: assignment.skipped,
                            },
                        )
                    })
                    .collect(),
            })
            .collect();
        if !state.virtual_servers.is_empty() {
            self.store.virtual_servers = state
                .virtual_servers
                .into_iter()
                .map(|(server_id, server)| {
                    (
                        server_id,
                        VirtualServer {
                            id: server.id,
                            port: server.port,
                            name: server.name,
                            unique_identifier: server.unique_identifier,
                            welcome_message: server.welcome_message,
                            host_message: server.host_message,
                            host_message_mode: server.host_message_mode,
                            ask_for_privilegekey: server.ask_for_privilegekey,
                            max_clients: server.max_clients,
                            antiflood_points_tick_reduce: server.antiflood_points_tick_reduce,
                            antiflood_points_needed_command_block: server
                                .antiflood_points_needed_command_block,
                            antiflood_points_needed_ip_block: server
                                .antiflood_points_needed_ip_block,
                            antiflood_ban_time: server.antiflood_ban_time,
                        },
                    )
                })
                .collect();
        }
        self.store.channels = state
            .channels
            .into_iter()
            .map(|(server_id, channels)| {
                (
                    server_id,
                    channels
                        .into_iter()
                        .map(|channel| Channel {
                            id: channel.id,
                            parent_id: channel.parent_id,
                            order: channel.order,
                            kind: channel.kind.into(),
                            name: channel.name.clone(),
                            topic: channel.topic.clone(),
                            description: channel.description.clone(),
                            password: String::new(),
                            codec: 0,
                            codec_quality: 5,
                            maxclients: -1,
                            maxfamilyclients: -1,
                            flag_default: false,
                            flag_password: false,
                            permissions: channel
                                .permissions
                                .into_iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name,
                                        PermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        })
                        .collect(),
                )
            })
            .collect();
        self.store.conversation_messages = state
            .conversation_messages
            .into_iter()
            .map(|(server_id, messages)| {
                (
                    server_id,
                    messages
                        .into_iter()
                        .map(|message| ConversationMessage {
                            conversation_id: message.conversation_id,
                            timestamp: message.timestamp,
                            sender_database_id: message.sender_database_id,
                            sender_unique_id: message.sender_unique_id,
                            sender_name: message.sender_name,
                            message: message.message,
                        })
                        .collect(),
                )
            })
            .collect();
        self.store.private_messages = state
            .private_messages
            .into_iter()
            .map(|(server_id, messages)| {
                (
                    server_id,
                    messages
                        .into_iter()
                        .map(|message| PrivateConversationMessage {
                            timestamp: message.timestamp,
                            sender_database_id: message.sender_database_id,
                            sender_unique_id: message.sender_unique_id,
                            sender_name: message.sender_name,
                            target_database_id: message.target_database_id,
                            target_unique_id: message.target_unique_id,
                            target_name: message.target_name,
                            message: message.message,
                        })
                        .collect(),
                )
            })
            .collect();
        if !state.music_bots.is_empty() {
            self.store.music_bots = state
                .music_bots
                .into_iter()
                .map(|(bot_id, bot)| {
                    let mut bot = MusicBot {
                        id: bot.id,
                        server_id: bot.server_id,
                        client_database_id: bot.client_database_id,
                        linked_client_id: bot.linked_client_id,
                        playlist_id: if bot.playlist_id == 0 { bot.id } else { bot.playlist_id },
                        current_song_id: (bot.current_song_id != 0).then_some(bot.current_song_id),
                        next_song_id: bot.next_song_id,
                        state: bot.state.into(),
                        player_volume: bot.player_volume,
                        playlist_title: bot.playlist_title,
                        playlist_description: bot.playlist_description,
                        playlist_flag_delete_played: bot.playlist_flag_delete_played,
                        playlist_flag_finished: bot.playlist_flag_finished,
                        playlist_replay_mode: bot.playlist_replay_mode,
                        playlist_max_songs: bot.playlist_max_songs,
                        permissions: bot
                            .permissions
                            .into_iter()
                            .map(|(permission_name, assignment)| {
                                (
                                    permission_name,
                                    PermissionAssignment {
                                        value: assignment.value,
                                        negated: assignment.negated,
                                        skipped: assignment.skipped,
                                    },
                                )
                            })
                            .collect(),
                        client_permissions: bot
                            .client_permissions
                            .into_iter()
                            .map(|target| PlaylistClientPermissionTarget {
                                client_database_id: target.client_database_id,
                                permissions: target
                                    .permissions
                                    .into_iter()
                                    .map(|(permission_name, assignment)| {
                                        (
                                            permission_name,
                                            PermissionAssignment {
                                                value: assignment.value,
                                                negated: assignment.negated,
                                                skipped: assignment.skipped,
                                            },
                                        )
                                    })
                                    .collect(),
                            })
                            .collect(),
                        current_song_started_at_millis: None,
                        current_song_progress_millis: 0,
                        queue: bot
                            .queue
                            .into_iter()
                            .map(|entry| MusicQueueEntry {
                                id: entry.song_id,
                                previous_song_id: entry.song_previous_song_id,
                                url: entry.song_url,
                                url_loader: entry.song_url_loader,
                                invoker_database_id: entry.song_invoker,
                                loaded: entry.song_loaded,
                                metadata: entry.song_metadata,
                                title: entry.song_title,
                                description: entry.song_description,
                                thumbnail: entry.song_thumbnail,
                                length_seconds: entry.song_length,
                                seekable: entry.song_seekable,
                                live_stream: entry.song_is_live,
                            })
                            .collect(),
                    };
                    Self::normalize_music_bot_queue(&mut bot);
                    (bot_id, bot)
                })
                .collect();
        }
        self.store.tokens = state
            .tokens
            .into_iter()
            .map(|(token_id, token)| {
                (
                    token_id,
                    PrivilegeToken {
                        id: token.id,
                        server_id: token.server_id,
                        token: token.token,
                        description: token.description,
                        max_uses: token.max_uses,
                        uses: token.uses,
                        created_at: token.created_at,
                        owner_login: token.owner_login,
                        expired_at: token.expired_at,
                        actions: token
                            .actions
                            .into_iter()
                            .map(|action| TokenAction {
                                id: action.id,
                                action_type: action.action_type,
                                action_id1: action.action_id1,
                                action_id2: action.action_id2,
                                action_text: action.action_text,
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        self.session_snapshots = state.session_snapshots;
        self.normalize_query_account_groups();
        self.normalize_client_permissions();
        self.normalize_channel_group_assignments();
        self.normalize_channel_client_permissions();
        self.store.next_client_database_id = state
            .next_client_database_id
            .max(next_client_database_seed(&self.store.query_accounts));
        self.store.next_conversation_timestamp =
            state
                .next_conversation_timestamp
                .max(next_conversation_timestamp_seed(
                    &self.store.conversation_messages,
                    &self.store.private_messages,
                ));
        self.store.next_token_id = state
            .next_token_id
            .max(next_token_id_seed(&self.store.tokens));
        self.store.next_token_action_id = state
            .next_token_action_id
            .max(next_token_action_id_seed(&self.store.tokens));
        for bot in self.store.music_bots.values_mut() {
            Self::normalize_music_bot_queue(bot);
        }

        if !self.store.server_groups.values().any(|g| g.name == "Server Admin") {
            let next_id = self.next_server_group_id();
            let server_admin_permissions = crate::runtime::permissions::build_named_permission_map(&self.specs, "Server Admin", "SERVER");
            self.store.server_groups.insert(
                next_id,
                ServerGroup {
                    id: next_id,
                    name: String::from("Server Admin"),
                    group_type: 1,
                    icon_id: 300,
                    save_db: true,
                    permissions: server_admin_permissions,
                },
            );
        }

        Ok(())
    }

    pub fn persist_state_best_effort(&self) {
        if let Err(error) = self.persist_state() {
            eprintln!("query runtime persistence error: {error:#}");
        }
    }

    pub(crate) fn persist_state(&self) -> Result<()> {
        let Some(state_store) = &self.state_store else {
            return Ok(());
        };

        let persisted_state = PersistedRuntimeState {
            schema_version: RUNTIME_STATE_SCHEMA_VERSION,
            query_accounts: self
                .store
                .query_accounts
                .iter()
                .map(|(login_name, account)| {
                    (
                        login_name.clone(),
                        PersistedQueryAccount {
                            login_name: account.login_name.clone(),
                            password: account.password.clone(),
                            server_id: account.server_id,
                            client_database_id: account.client_database_id,
                            server_groups: account.server_groups.clone(),
                            permissions: account
                                .permissions
                                .iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name.clone(),
                                        PersistedPermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            server_groups: self
                .store
                .server_groups
                .iter()
                .map(|(group_id, group)| {
                    (
                        *group_id,
                        PersistedServerGroup {
                            id: group.id,
                            name: group.name.clone(),
                            group_type: group.group_type,
                            icon_id: group.icon_id,
                            save_db: group.save_db,
                            permissions: group
                                .permissions
                                .iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name.clone(),
                                        PersistedPermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            channel_groups: self
                .store
                .channel_groups
                .iter()
                .map(|(group_id, group)| {
                    (
                        *group_id,
                        PersistedChannelGroup {
                            id: group.id,
                            name: group.name.clone(),
                            group_type: group.group_type,
                            icon_id: group.icon_id,
                            save_db: group.save_db,
                            permissions: group
                                .permissions
                                .iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name.clone(),
                                        PersistedPermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            virtual_servers: self
                .store
                .virtual_servers
                .iter()
                .map(|(server_id, server)| {
                    (
                        *server_id,
                        PersistedVirtualServer {
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
                            antiflood_points_needed_command_block: server
                                .antiflood_points_needed_command_block,
                            antiflood_points_needed_ip_block: server
                                .antiflood_points_needed_ip_block,
                            antiflood_ban_time: server.antiflood_ban_time,
                        },
                    )
                })
                .collect(),
            channels: self
                .store
                .channels
                .iter()
                .map(|(server_id, channels)| {
                    (
                        *server_id,
                        channels
                            .iter()
                            .map(|channel| PersistedChannel {
                                id: channel.id,
                                parent_id: channel.parent_id,
                                order: channel.order,
                                kind: channel.kind.into(),
                                name: channel.name.clone(),
                                topic: channel.topic.clone(),
                                description: channel.description.clone(),
                                permissions: channel
                                    .permissions
                                    .iter()
                                    .map(|(permission_name, assignment)| {
                                        (
                                            permission_name.clone(),
                                            PersistedPermissionAssignment {
                                                value: assignment.value,
                                                negated: assignment.negated,
                                                skipped: assignment.skipped,
                                            },
                                        )
                                    })
                                    .collect(),
                            })
                            .collect(),
                    )
                })
                .collect(),
            channel_group_assignments: self
                .store
                .channel_group_assignments
                .iter()
                .map(|assignment| PersistedChannelGroupAssignment {
                    channel_id: assignment.channel_id,
                    client_database_id: assignment.client_database_id,
                    channel_group_id: assignment.channel_group_id,
                })
                .collect(),
            channel_client_permissions: self
                .store
                .channel_client_permissions
                .iter()
                .map(|target| PersistedChannelClientPermissionTarget {
                    channel_id: target.channel_id,
                    client_database_id: target.client_database_id,
                    permissions: target
                        .permissions
                        .iter()
                        .map(|(permission_name, assignment)| {
                            (
                                permission_name.clone(),
                                PersistedPermissionAssignment {
                                    value: assignment.value,
                                    negated: assignment.negated,
                                    skipped: assignment.skipped,
                                },
                            )
                        })
                        .collect(),
                })
                .collect(),
            client_permissions: self
                .store
                .client_permissions
                .iter()
                .map(|target| PersistedClientPermissionTarget {
                    client_database_id: target.client_database_id,
                    client_unique_identifier: target.client_unique_identifier.clone(),
                    client_nickname: target.client_nickname.clone(),
                    permissions: target
                        .permissions
                        .iter()
                        .map(|(permission_name, assignment)| {
                            (
                                permission_name.clone(),
                                PersistedPermissionAssignment {
                                    value: assignment.value,
                                    negated: assignment.negated,
                                    skipped: assignment.skipped,
                                },
                            )
                        })
                        .collect(),
                })
                .collect(),
            conversation_messages: self
                .store
                .conversation_messages
                .iter()
                .map(|(server_id, messages)| {
                    (
                        *server_id,
                        messages
                            .iter()
                            .map(|message| PersistedConversationMessage {
                                conversation_id: message.conversation_id,
                                timestamp: message.timestamp,
                                sender_database_id: message.sender_database_id,
                                sender_unique_id: message.sender_unique_id.clone(),
                                sender_name: message.sender_name.clone(),
                                message: message.message.clone(),
                            })
                            .collect(),
                    )
                })
                .collect(),
            private_messages: self
                .store
                .private_messages
                .iter()
                .map(|(server_id, messages)| {
                    (
                        *server_id,
                        messages
                            .iter()
                            .map(|message| PersistedPrivateConversationMessage {
                                timestamp: message.timestamp,
                                sender_database_id: message.sender_database_id,
                                sender_unique_id: message.sender_unique_id.clone(),
                                sender_name: message.sender_name.clone(),
                                target_database_id: message.target_database_id,
                                target_unique_id: message.target_unique_id.clone(),
                                target_name: message.target_name.clone(),
                                message: message.message.clone(),
                            })
                            .collect(),
                    )
                })
                .collect(),
            music_bots: self
                .store
                .music_bots
                .iter()
                .map(|(bot_id, bot)| {
                    (
                        *bot_id,
                        PersistedMusicBot {
                            id: bot.id,
                            server_id: bot.server_id,
                            client_database_id: bot.client_database_id,
                            linked_client_id: bot.linked_client_id,
                            playlist_id: bot.playlist_id,
                            current_song_id: bot.current_song_id.unwrap_or(0),
                            next_song_id: bot.next_song_id,
                            state: bot.state.clone().into(),
                            player_volume: bot.player_volume.clone(),
                            playlist_title: bot.playlist_title.clone(),
                            playlist_description: bot.playlist_description.clone(),
                            playlist_flag_delete_played: bot.playlist_flag_delete_played,
                            playlist_flag_finished: bot.playlist_flag_finished,
                            playlist_replay_mode: bot.playlist_replay_mode,
                            playlist_max_songs: bot.playlist_max_songs,
                            permissions: bot
                                .permissions
                                .iter()
                                .map(|(permission_name, assignment)| {
                                    (
                                        permission_name.clone(),
                                        PersistedPermissionAssignment {
                                            value: assignment.value,
                                            negated: assignment.negated,
                                            skipped: assignment.skipped,
                                        },
                                    )
                                })
                                .collect(),
                            client_permissions: bot
                                .client_permissions
                                .iter()
                                .map(|target| PersistedPlaylistClientPermissionTarget {
                                    client_database_id: target.client_database_id,
                                    permissions: target
                                        .permissions
                                        .iter()
                                        .map(|(permission_name, assignment)| {
                                            (
                                                permission_name.clone(),
                                                PersistedPermissionAssignment {
                                                    value: assignment.value,
                                                    negated: assignment.negated,
                                                    skipped: assignment.skipped,
                                                },
                                            )
                                        })
                                        .collect(),
                                })
                                .collect(),
                            queue: bot
                                .queue
                                .iter()
                                .map(|entry| PersistedMusicQueueEntry {
                                    song_id: entry.id,
                                    song_previous_song_id: entry.previous_song_id,
                                    song_url: entry.url.clone(),
                                    song_url_loader: entry.url_loader.clone(),
                                    song_invoker: entry.invoker_database_id,
                                    song_loaded: entry.loaded,
                                    song_metadata: entry.metadata.clone(),
                                    song_title: entry.title.clone(),
                                    song_description: entry.description.clone(),
                                    song_thumbnail: entry.thumbnail.clone(),
                                    song_length: entry.length_seconds,
                                    song_seekable: entry.seekable,
                                    song_is_live: entry.live_stream,
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            tokens: self
                .store
                .tokens
                .iter()
                .map(|(token_id, token)| {
                    (
                        *token_id,
                        PersistedToken {
                            id: token.id,
                            server_id: token.server_id,
                            token: token.token.clone(),
                            description: token.description.clone(),
                            max_uses: token.max_uses,
                            uses: token.uses,
                            created_at: token.created_at,
                            owner_login: token.owner_login.clone(),
                            expired_at: token.expired_at,
                            actions: token
                                .actions
                                .iter()
                                .map(|action| PersistedTokenAction {
                                    id: action.id,
                                    action_type: action.action_type,
                                    action_id1: action.action_id1,
                                    action_id2: action.action_id2,
                                    action_text: action.action_text.clone(),
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            session_snapshots: self.session_snapshots.clone(),
            next_client_database_id: self.store.next_client_database_id,
            next_conversation_timestamp: self.store.next_conversation_timestamp,
            next_token_id: self.store.next_token_id,
            next_token_action_id: self.store.next_token_action_id,
        };

        state_store.save(&persisted_state)
    }

    pub(crate) fn sync_session_snapshot(
        &mut self,
        _before_session: &QuerySessionState,
        session: &QuerySessionState,
    ) {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return;
        };

        self.session_snapshots.insert(
            login_name.clone(),
            PersistedSessionSnapshot {
                selected_virtual_server_id: session.selected_virtual_server_id,
                current_channel_id: session.current_channel_id,
                virtual_mode: session.virtual_mode,
                notification_subscriptions: session
                    .notification_subscriptions
                    .iter()
                    .map(|subscription| PersistedNotificationSubscription {
                        event: subscription.event.as_str().to_string(),
                        channel_id: subscription.channel_id,
                    })
                    .collect(),
            },
        );
    }

    pub(crate) fn restore_session_from_snapshot(
        &self,
        login_name: &str,
        fallback_server_id: Option<u32>,
        session: &mut QuerySessionState,
    ) {
        let selected_virtual_server_id = self
            .session_snapshots
            .get(login_name)
            .and_then(|snapshot| snapshot.selected_virtual_server_id)
            .filter(|server_id| self.store.virtual_servers.contains_key(server_id))
            .or(fallback_server_id
                .filter(|server_id| self.store.virtual_servers.contains_key(server_id)));

        session.selected_virtual_server_id = selected_virtual_server_id;
        session.current_channel_id = selected_virtual_server_id.and_then(|server_id| {
            self.session_snapshots
                .get(login_name)
                .and_then(|snapshot| snapshot.current_channel_id)
                .filter(|channel_id| self.channel_exists(server_id, *channel_id))
                .or_else(|| self.default_channel_id_for_server(server_id))
        });
        session.virtual_mode = self
            .session_snapshots
            .get(login_name)
            .map(|snapshot| snapshot.virtual_mode)
            .unwrap_or(false);
        session.notification_subscriptions = selected_virtual_server_id
            .map(|server_id| self.restore_notification_subscriptions(login_name, server_id))
            .unwrap_or_default();
    }

}
