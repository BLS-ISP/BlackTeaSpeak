use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_servercreate(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_create"],
            "b_virtualserver_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let port = if let Some(port_str) = request.named_args.get("virtualserver_port") {
            let Ok(parsed) = port_str.parse::<u16>() else {
                return QueryResponse::error(512, "virtualserver_port must be a valid port");
            };
            if parsed == 0 {
                return QueryResponse::error(512, "virtualserver_port must be greater than 0");
            }
            if self
                .store
                .virtual_servers
                .values()
                .any(|server| server.port == parsed)
            {
                return QueryResponse::error(1028, "unable to bind network port");
            }
            parsed
        } else {
            (9987_u16..=65534)
                .find(|candidate| {
                    !self
                        .store
                        .virtual_servers
                        .values()
                        .any(|server| server.port == *candidate)
                })
                .unwrap_or(9987)
        };

        let sid = self
            .store
            .virtual_servers
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);

        let name = request
            .named_args
            .get("virtualserver_name")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| String::from("BlackTeaSpeak Server"));
        let password = request
            .named_args
            .get("virtualserver_password")
            .cloned()
            .unwrap_or_default();
        let welcome_message = request
            .named_args
            .get("virtualserver_welcomemessage")
            .cloned()
            .unwrap_or_else(|| String::from("Welcome to BlackTeaSpeak!"));
        let host_message = request
            .named_args
            .get("virtualserver_hostmessage")
            .cloned()
            .unwrap_or_default();
        let host_message_mode = request
            .named_args
            .get("virtualserver_hostmessage_mode")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let max_clients = request
            .named_args
            .get("virtualserver_maxclients")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(32);
        let unique_identifier = request
            .named_args
            .get("virtualserver_unique_identifier")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("compat-server-uid-{sid}"));

        let new_server = VirtualServer {
            id: sid,
            port,
            name,
            status: String::from("online"),
            password,
            unique_identifier,
            welcome_message,
            host_message,
            host_message_mode,
            ask_for_privilegekey: 1,
            max_clients,
            antiflood_points_tick_reduce: 10,
            antiflood_points_needed_command_block: 150,
            antiflood_points_needed_ip_block: 250,
            antiflood_ban_time: 300,
        };

        let _ = self.db.save_virtual_server(&new_server);
        self.store.virtual_servers.insert(sid, new_server);

        let default_channel_id = self
            .store
            .channels
            .values()
            .flat_map(|chs| chs.iter().map(|c| c.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        let default_channel = Channel {
            id: default_channel_id,
            parent_id: 0,
            order: 0,
            kind: ChannelKind::Permanent,
            name: String::from("Default Channel"),
            topic: String::from("Default Channel has no topic"),
            description: String::new(),
            password: String::new(),
            codec: 4,
            codec_quality: 10,
            maxclients: -1,
            maxfamilyclients: -1,
            flag_default: true,
            flag_password: false,
            permissions: BTreeMap::new(),
        };
        let _ = self.db.save_channel(sid, &default_channel);
        self.store.channels.insert(sid, vec![default_channel]);

        let admin_sgid = self
            .store
            .server_groups
            .values()
            .find(|g| g.name == "Server Admin")
            .map(|g| g.id)
            .unwrap_or(9);
        let token_id = self.next_token_id();
        let action_id = self.next_token_action_id();
        let created_at = current_unix_timestamp();
        let token_value = format!("compat{:08x}{:016x}", token_id, created_at);
        let owner_login = session
            .authenticated_login
            .clone()
            .unwrap_or_else(|| format!("cldbid:{}", actor.client_database_id));
        let priv_token = PrivilegeToken {
            id: token_id,
            server_id: sid,
            token: token_value.clone(),
            description: String::from("Initial Server Admin privilege key"),
            max_uses: 1,
            uses: 0,
            created_at,
            owner_login,
            expired_at: None,
            actions: vec![TokenAction {
                id: action_id,
                action_type: 2,
                action_id1: admin_sgid,
                action_id2: 0,
                action_text: String::new(),
            }],
        };
        let _ = self.db.save_token(&priv_token);
        self.store.tokens.insert(token_id, priv_token);

        if let Some(tx) = &self.lifecycle_tx {
            let _ = tx.send(LifecycleAction::StartVirtualServer {
                server_id: sid,
                port,
            });
        }

        self.append_server_log(
            sid,
            4,
            "VirtualServerBase",
            format!("virtualserver {sid} created and started on port {port}"),
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("sid"), sid.to_string());
        row.insert(String::from("virtualserver_port"), port.to_string());
        row.insert(String::from("token"), token_value);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_serverstop(
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
                "b_virtualserver_stop",
                "b_virtualserver_stop_any",
                "b_virtualserver_create",
            ],
            "b_virtualserver_stop",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(sid) = request
            .named_args
            .get("sid")
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| request.positional_args.first().and_then(|v| v.parse::<u32>().ok()))
        else {
            return QueryResponse::error(512, "sid is required");
        };

        let Some(server) = self.store.virtual_servers.get_mut(&sid) else {
            return QueryResponse::error(768, "virtual server not found");
        };
        if server.status == "offline" {
            return QueryResponse::error(1026, "server is not running");
        }
        server.status = String::from("offline");

        let reasonmsg = request
            .named_args
            .get("reasonmsg")
            .cloned()
            .unwrap_or_else(|| String::from("Server Shutdown"));
        let client_ids = self
            .store
            .online_clients
            .values()
            .filter(|c| c.server_id == sid)
            .map(|c| c.id)
            .collect::<Vec<_>>();
        for clid in client_ids {
            self.remove_session_client(clid, 11, reasonmsg.clone());
        }

        if let Some(tx) = &self.lifecycle_tx {
            let _ = tx.send(LifecycleAction::StopVirtualServer { server_id: sid });
        }

        self.append_server_log(
            sid,
            4,
            "VirtualServerBase",
            format!("virtualserver {sid} stopped ({reasonmsg})"),
        );

        QueryResponse::ok()
    }

    pub(crate) fn handle_serverstart(
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
                "b_virtualserver_start",
                "b_virtualserver_start_any",
                "b_virtualserver_create",
            ],
            "b_virtualserver_start",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(sid) = request
            .named_args
            .get("sid")
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| request.positional_args.first().and_then(|v| v.parse::<u32>().ok()))
        else {
            return QueryResponse::error(512, "sid is required");
        };

        let Some(server) = self.store.virtual_servers.get_mut(&sid) else {
            return QueryResponse::error(768, "virtual server not found");
        };
        if server.status != "offline" {
            return QueryResponse::error(1025, "server is running");
        }
        server.status = String::from("online");
        let port = server.port;

        if let Some(tx) = &self.lifecycle_tx {
            let _ = tx.send(LifecycleAction::StartVirtualServer {
                server_id: sid,
                port,
            });
        }

        self.append_server_log(
            sid,
            4,
            "VirtualServerBase",
            format!("virtualserver {sid} started on port {port}"),
        );

        QueryResponse::ok()
    }

    pub(crate) fn handle_serverdelete(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
            &["b_virtualserver_delete", "b_virtualserver_create"],
            "b_virtualserver_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(sid) = request
            .named_args
            .get("sid")
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| request.positional_args.first().and_then(|v| v.parse::<u32>().ok()))
        else {
            return QueryResponse::error(512, "sid is required");
        };

        let Some(server) = self.store.virtual_servers.get(&sid) else {
            return QueryResponse::error(768, "virtual server not found");
        };
        if server.status != "offline" && !request.flags.contains("force") {
            return QueryResponse::error(1025, "server is running");
        }

        if let Some(tx) = &self.lifecycle_tx {
            let _ = tx.send(LifecycleAction::StopVirtualServer { server_id: sid });
        }

        let channel_ids = self
            .store
            .channels
            .get(&sid)
            .map(|chs| chs.iter().map(|c| c.id).collect::<BTreeSet<_>>())
            .unwrap_or_default();

        self.store.virtual_servers.remove(&sid);
        self.store.channels.remove(&sid);
        self.store.conversation_messages.remove(&sid);
        self.store.private_messages.remove(&sid);
        self.store
            .channel_group_assignments
            .retain(|a| !channel_ids.contains(&a.channel_id));
        self.store
            .channel_client_permissions
            .retain(|t| !channel_ids.contains(&t.channel_id));
        self.store
            .offline_messages
            .retain(|_, m| m.server_id != sid);
        self.store.complaints.retain(|c| c.server_id != sid);
        self.store
            .custom_client_properties
            .retain(|(s_id, _), _| *s_id != sid);
        self.store.tokens.retain(|_, t| t.server_id != sid);
        self.store
            .server_temp_passwords
            .retain(|p| p.server_id != sid);
        self.store.active_bans.retain(|_, b| b.server_id != sid);
        self.store.music_bots.retain(|_, b| b.server_id != sid);
        self.store.online_clients.retain(|_, c| c.server_id != sid);
        let _ = self.db.delete_virtual_server(sid);

        if session.selected_virtual_server_id == Some(sid) {
            session.selected_virtual_server_id = None;
            session.current_channel_id = None;
            session.virtual_mode = false;
        }

        self.append_server_log(
            0,
            4,
            "ServerLibPriv",
            format!("virtualserver {sid} deleted"),
        );

        QueryResponse::ok()
    }

    pub(crate) fn handle_serverprocessstop(
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
            &["b_serverinstance_stop"],
            "b_serverinstance_stop",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let reasonmsg = request
            .named_args
            .get("reasonmsg")
            .cloned()
            .unwrap_or_else(|| String::from("Server Process Shutdown"));
        self.append_server_log(
            0,
            4,
            "ServerLibPriv",
            format!("server process stop requested ({reasonmsg})"),
        );

        QueryResponse::ok()
    }

    pub(crate) fn handle_serversnapshotcreate(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server) = self.selected_server(session).cloned() else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[
                "b_virtualserver_snapshot_create",
                "b_virtualserver_modify_name",
            ],
            "b_virtualserver_snapshot_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let server_id = server.id;
        let channels = self
            .store
            .channels
            .get(&server_id)
            .cloned()
            .unwrap_or_default();
        let channel_ids = channels.iter().map(|c| c.id).collect::<BTreeSet<_>>();

        let payload = VirtualServerSnapshotPayload {
            server: PersistedVirtualServer {
                id: server.id,
                port: server.port,
                name: server.name.clone(),
                unique_identifier: server.unique_identifier.clone(),
                status: server.status.clone(),
                password: server.password.clone(),
                welcome_message: server.welcome_message.clone(),
                host_message: server.host_message.clone(),
                host_message_mode: server.host_message_mode,
                ask_for_privilegekey: server.ask_for_privilegekey,
                max_clients: server.max_clients,
                antiflood_points_tick_reduce: server.antiflood_points_tick_reduce,
                antiflood_points_needed_command_block: server.antiflood_points_needed_command_block,
                antiflood_points_needed_ip_block: server.antiflood_points_needed_ip_block,
                antiflood_ban_time: server.antiflood_ban_time,
            },
            channels: channels
                .iter()
                .map(|channel| PersistedChannel {
                    id: channel.id,
                    parent_id: channel.parent_id,
                    order: channel.order,
                    kind: channel.kind.into(),
                    name: channel.name.clone(),
                    topic: channel.topic.clone(),
                    description: channel.description.clone(),
                    password: channel.password.clone(),
                    codec: channel.codec,
                    codec_quality: channel.codec_quality,
                    maxclients: channel.maxclients,
                    maxfamilyclients: channel.maxfamilyclients,
                    flag_default: channel.flag_default,
                    flag_password: channel.flag_password || !channel.password.is_empty(),
                    permissions: channel
                        .permissions
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                PersistedPermissionAssignment {
                                    value: v.value,
                                    negated: v.negated,
                                    skipped: v.skipped,
                                },
                            )
                        })
                        .collect(),
                })
                .collect(),
            server_groups: self
                .store
                .server_groups
                .iter()
                .map(|(id, g)| {
                    (
                        *id,
                        PersistedServerGroup {
                            id: g.id,
                            name: g.name.clone(),
                            group_type: g.group_type,
                            icon_id: g.icon_id,
                            save_db: g.save_db,
                            permissions: g
                                .permissions
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        k.clone(),
                                        PersistedPermissionAssignment {
                                            value: v.value,
                                            negated: v.negated,
                                            skipped: v.skipped,
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
                .map(|(id, g)| {
                    (
                        *id,
                        PersistedChannelGroup {
                            id: g.id,
                            name: g.name.clone(),
                            group_type: g.group_type,
                            icon_id: g.icon_id,
                            save_db: g.save_db,
                            permissions: g
                                .permissions
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        k.clone(),
                                        PersistedPermissionAssignment {
                                            value: v.value,
                                            negated: v.negated,
                                            skipped: v.skipped,
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            channel_group_assignments: self
                .store
                .channel_group_assignments
                .iter()
                .filter(|a| channel_ids.contains(&a.channel_id))
                .map(|a| PersistedChannelGroupAssignment {
                    channel_id: a.channel_id,
                    client_database_id: a.client_database_id,
                    channel_group_id: a.channel_group_id,
                })
                .collect(),
            channel_client_permissions: self
                .store
                .channel_client_permissions
                .iter()
                .filter(|t| channel_ids.contains(&t.channel_id))
                .map(|t| PersistedChannelClientPermissionTarget {
                    channel_id: t.channel_id,
                    client_database_id: t.client_database_id,
                    permissions: t
                        .permissions
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                PersistedPermissionAssignment {
                                    value: v.value,
                                    negated: v.negated,
                                    skipped: v.skipped,
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
                .map(|t| PersistedClientPermissionTarget {
                    client_database_id: t.client_database_id,
                    client_unique_identifier: t.client_unique_identifier.clone(),
                    client_nickname: t.client_nickname.clone(),
                    permissions: t
                        .permissions
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                PersistedPermissionAssignment {
                                    value: v.value,
                                    negated: v.negated,
                                    skipped: v.skipped,
                                },
                            )
                        })
                        .collect(),
                })
                .collect(),
            custom_client_properties: self
                .store
                .custom_client_properties
                .iter()
                .filter(|((s_id, _), _)| *s_id == server_id)
                .map(|((s_id, cldbid), props)| PersistedCustomClientProperty {
                    server_id: *s_id,
                    client_database_id: *cldbid,
                    properties: props.clone(),
                })
                .collect(),
        };

        let json_bytes = serde_json::to_vec(&payload).unwrap_or_default();
        let data = BASE64_STANDARD.encode(&json_bytes);
        let hash = BASE64_STANDARD.encode(Sha1::digest(data.as_bytes()));
        let version = request
            .named_args
            .get("version")
            .cloned()
            .unwrap_or_else(|| String::from("2"));

        let mut row = BTreeMap::new();
        row.insert(String::from("version"), version);
        row.insert(String::from("hash"), hash);
        row.insert(String::from("data"), data);
        row.insert(
            String::from("virtualserver_unique_identifier"),
            server.unique_identifier,
        );
        row.insert(String::from("virtualserver_name"), server.name);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_serversnapshotdeploy(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
                "b_virtualserver_snapshot_deploy",
                "b_virtualserver_modify_name",
            ],
            "b_virtualserver_snapshot_deploy",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(data_b64) = request
            .named_args
            .get("data")
            .cloned()
            .or_else(|| {
                request
                    .positional_args
                    .iter()
                    .find(|arg| !arg.starts_with('-'))
                    .cloned()
            })
            .filter(|s| !s.trim().is_empty())
        else {
            return QueryResponse::error(512, "snapshot data is required");
        };

        let Ok(decoded_bytes) = BASE64_STANDARD.decode(data_b64.trim()) else {
            return QueryResponse::error(1538, "invalid snapshot base64 data");
        };
        let Ok(payload) = serde_json::from_slice::<VirtualServerSnapshotPayload>(&decoded_bytes)
        else {
            return QueryResponse::error(1538, "invalid snapshot payload");
        };

        if let Some(server) = self.store.virtual_servers.get_mut(&server_id) {
            server.name = payload.server.name;
            server.password = payload.server.password;
            server.welcome_message = payload.server.welcome_message;
            server.host_message = payload.server.host_message;
            server.host_message_mode = payload.server.host_message_mode;
            server.ask_for_privilegekey = payload.server.ask_for_privilegekey;
            server.max_clients = payload.server.max_clients;
            server.antiflood_points_tick_reduce = payload.server.antiflood_points_tick_reduce;
            server.antiflood_points_needed_command_block =
                payload.server.antiflood_points_needed_command_block;
            server.antiflood_points_needed_ip_block =
                payload.server.antiflood_points_needed_ip_block;
            server.antiflood_ban_time = payload.server.antiflood_ban_time;
        }
        if let Some(server) = self.store.virtual_servers.get(&server_id).cloned() {
            let _ = self.db.save_virtual_server(&server);
        }

        let old_channel_ids = self
            .store
            .channels
            .get(&server_id)
            .map(|chs| chs.iter().map(|c| c.id).collect::<BTreeSet<_>>())
            .unwrap_or_default();

        let restored_channels = payload
            .channels
            .into_iter()
            .map(|ch| Channel {
                id: ch.id,
                parent_id: ch.parent_id,
                order: ch.order,
                kind: ch.kind.into(),
                name: ch.name,
                topic: ch.topic,
                description: ch.description,
                password: ch.password,
                codec: ch.codec,
                codec_quality: ch.codec_quality,
                maxclients: ch.maxclients,
                maxfamilyclients: ch.maxfamilyclients,
                flag_default: ch.flag_default,
                flag_password: ch.flag_password,
                permissions: ch
                    .permissions
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            PermissionAssignment {
                                value: v.value,
                                negated: v.negated,
                                skipped: v.skipped,
                            },
                        )
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        let mapping_rows = restored_channels
            .iter()
            .map(|ch| {
                let mut row = BTreeMap::new();
                row.insert(String::from("ocid"), ch.id.to_string());
                row.insert(String::from("ncid"), ch.id.to_string());
                row
            })
            .collect::<Vec<_>>();

        for ch in &restored_channels {
            let _ = self.db.save_channel(server_id, ch);
        }
        if !restored_channels.is_empty() {
            self.store.channels.insert(server_id, restored_channels);
        }

        let new_channel_ids = self
            .store
            .channels
            .get(&server_id)
            .map(|chs| chs.iter().map(|c| c.id).collect::<BTreeSet<_>>())
            .unwrap_or_default();

        self.store.channel_group_assignments.retain(|a| {
            !old_channel_ids.contains(&a.channel_id) && !new_channel_ids.contains(&a.channel_id)
        });
        for a in payload.channel_group_assignments {
            self.store
                .channel_group_assignments
                .push(ChannelGroupAssignment {
                    channel_id: a.channel_id,
                    client_database_id: a.client_database_id,
                    channel_group_id: a.channel_group_id,
                });
        }

        self.store.channel_client_permissions.retain(|t| {
            !old_channel_ids.contains(&t.channel_id) && !new_channel_ids.contains(&t.channel_id)
        });
        for t in payload.channel_client_permissions {
            self.store
                .channel_client_permissions
                .push(ChannelClientPermissionTarget {
                    channel_id: t.channel_id,
                    client_database_id: t.client_database_id,
                    permissions: t
                        .permissions
                        .into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                PermissionAssignment {
                                    value: v.value,
                                    negated: v.negated,
                                    skipped: v.skipped,
                                },
                            )
                        })
                        .collect(),
                });
        }

        self.store
            .custom_client_properties
            .retain(|(s_id, _), _| *s_id != server_id);
        for prop in payload.custom_client_properties {
            self.store.custom_client_properties.insert(
                (server_id, prop.client_database_id),
                prop.properties,
            );
        }

        session.current_channel_id = self.default_channel_id_for_server(server_id);
        self.append_server_log(
            server_id,
            4,
            "VirtualServerBase",
            String::from("virtualserver snapshot deployed"),
        );

        if request.flags.contains("mapping") && !mapping_rows.is_empty() {
            QueryResponse::ok_rows(mapping_rows)
        } else {
            QueryResponse::ok()
        }
    }
}
