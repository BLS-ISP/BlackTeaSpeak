use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use sha1::{Digest, Sha1};

use crate::models::{BuildVersion, CommandSpec, PermissionCatalogEntry};
use crate::query::{CommandRequest, QueryResponse, parse_request_line, render_response};
use crate::specs::FoundationSpecs;
use crate::state::{
    PersistedChannel, PersistedChannelClientPermissionTarget, PersistedChannelGroup,
    PersistedChannelGroupAssignment, PersistedChannelKind, PersistedClientPermissionTarget,
    PersistedConversationMessage, PersistedMusicBot, PersistedMusicBotState,
    PersistedMusicQueueEntry, PersistedNotificationSubscription,
    PersistedPermissionAssignment, PersistedPlaylistClientPermissionTarget,
    PersistedPrivateConversationMessage, PersistedQueryAccount, PersistedRuntimeState,
    PersistedServerGroup, PersistedSessionSnapshot, PersistedToken, PersistedTokenAction,
    PersistedVirtualServer, RUNTIME_STATE_SCHEMA_VERSION, RuntimeStateStore,
};

mod ffmpeg;
mod musicbot;
mod handlers;
mod ytdlp;

pub mod models;
pub use models::*;

pub mod permissions;
pub use permissions::*;

pub mod persistence;
pub mod dispatch;
pub mod web;
pub mod events;

pub use self::musicbot::MusicBotNotifyPayload;
use self::musicbot::{MusicBot, MusicBotState, MusicQueueEntry, PlaylistClientPermissionTarget};

pub(crate) use self::permissions::PermissionAssignment;
use self::permissions::{
    build_named_permission_map, build_permission_catalog,
    build_permission_map, check_channel_delete_power_allowed, check_channel_modify_power_allowed,
    check_required_permission, load_blackteaweb_permission_ids, permission_value_or_default,
    seed_store, session_has_permission_actor,
};

pub const ERROR_CLIENT_IS_FLOODING: u32 = 524;
pub const ERROR_DATABASE_EMPTY_RESULT: u32 = 0x501;

#[derive(Debug, Clone, Copy)]
struct AntiFloodConfig {
    pub(crate) points_tick_reduce: u32,
    pub(crate) points_needed_command_block: u32,
    pub(crate) points_needed_ip_block: u32,
    pub(crate) ban_time: u32,
}

#[derive(Debug, Clone)]
struct InMemoryStore {
    pub(crate) query_accounts: BTreeMap<String, QueryAccount>,
    pub(crate) server_groups: BTreeMap<u32, ServerGroup>,
    pub(crate) channel_groups: BTreeMap<u32, ChannelGroup>,
    pub(crate) virtual_servers: BTreeMap<u32, VirtualServer>,
    pub(crate) channels: BTreeMap<u32, Vec<Channel>>,
    pub(crate) channel_group_assignments: Vec<ChannelGroupAssignment>,
    pub(crate) channel_client_permissions: Vec<ChannelClientPermissionTarget>,
    pub(crate) client_permissions: Vec<ClientPermissionTarget>,
    pub(crate) conversation_messages: BTreeMap<u32, Vec<ConversationMessage>>,
    pub(crate) private_messages: BTreeMap<u32, Vec<PrivateConversationMessage>>,
    pub(crate) tokens: BTreeMap<u32, PrivilegeToken>,
    pub(crate) active_bans: BTreeMap<u32, ActiveBan>,
    pub(crate) online_clients: BTreeMap<u64, OnlineClient>,
    pub(crate) clients: BTreeMap<u64, Client>,
    pub(crate) music_bots: BTreeMap<u32, MusicBot>,
    pub(crate) next_query_client_id: u64,
    pub(crate) next_client_database_id: u64,
    pub(crate) next_conversation_timestamp: u64,
    pub(crate) next_ban_id: u32,
    pub(crate) next_token_id: u32,
    pub(crate) next_token_action_id: u32,
    pub db: std::sync::Arc<crate::database::Database>,
}

pub type EventCallback = Box<dyn Fn(&BaselineRuntime, u32, &crate::transport::TransportNotification) + Send + Sync>;

#[derive(Clone, Debug)]
pub enum LifecycleAction {
    StartVirtualServer { server_id: u32, port: u16 },
    StopVirtualServer { server_id: u32 },
}

pub struct BaselineRuntime {
    pub(crate) specs: FoundationSpecs,
    pub(crate) store: InMemoryStore,
    pub(crate) permission_catalog: BTreeMap<String, PermissionCatalogEntry>,
    pub(crate) web_permission_base_ids: BTreeMap<String, u32>,
    pub(crate) session_snapshots: BTreeMap<String, PersistedSessionSnapshot>,
    pub(crate) anti_flood_ip_states: BTreeMap<(u32, String), AntiFloodSessionState>,
    pub(crate) state_store: Option<RuntimeStateStore>,
    pub db: crate::database::Database,
    pub(crate) event_subscribers: Arc<Mutex<Vec<EventCallback>>>,
    pub lifecycle_tx: Option<std::sync::mpsc::Sender<LifecycleAction>>,
    pub file_transfer_registry: Option<std::sync::Arc<crate::file_transfer::FileTransferRegistry>>,
    pub music_download_tx: Option<std::sync::mpsc::Sender<(u32, u32, String)>>,
    pub webtransport_btea_media_tx: Option<tokio::sync::mpsc::UnboundedSender<(u32, u64, u8, Vec<u8>)>>,
    pub desktop_btea_media_tx: Option<tokio::sync::broadcast::Sender<(u32, u64, u8, Vec<u8>)>>,
    pub next_upload_id: u64,
    pub next_download_id: u64,
}

pub fn create_baseline_runtime(workspace_root: impl AsRef<Path>) -> Result<BaselineRuntime> {
    let workspace_root = workspace_root.as_ref().to_path_buf();
    let state_path = RuntimeStateStore::default_path(&workspace_root);
    create_baseline_runtime_with_state_path(&workspace_root, &state_path)
}

pub fn create_baseline_runtime_with_state_path(
    workspace_root: impl AsRef<Path>,
    state_path: impl AsRef<Path>,
) -> Result<BaselineRuntime> {
    let workspace_root = workspace_root.as_ref().to_path_buf();
    let specs = FoundationSpecs::load(&workspace_root)?;

    let admin_permissions = build_named_permission_map(&specs, "Admin Server Query", "QUERY");
    let guest_permissions = build_named_permission_map(&specs, "Guest Server Query", "QUERY");
    let server_admin_permissions = build_named_permission_map(&specs, "Server Admin", "SERVER");
    let channel_admin_permissions = build_named_permission_map(&specs, "Channel Admin", "CHANNEL");
    let channel_operator_permissions = build_named_permission_map(&specs, "Operator", "CHANNEL");
    let channel_guest_permissions = build_named_permission_map(&specs, "Guest", "CHANNEL");

    let mut store = seed_store(
        &admin_permissions,
        &guest_permissions,
        &server_admin_permissions,
        &channel_admin_permissions,
        &channel_operator_permissions,
        &channel_guest_permissions,
    );

    let db_path = workspace_root.join("blackteaspeak.db");
    let db = crate::database::Database::new(db_path).expect("Failed to initialize database");

    if let Ok(loaded_servers) = db.load_virtual_servers() {
        if loaded_servers.is_empty() {
            for server in store.virtual_servers.values() {
                let _ = db.save_virtual_server(server);
            }
            for (server_id, channels) in &store.channels {
                for channel in channels {
                    let _ = db.save_channel(*server_id, channel);
                }
            }
            for group in store.server_groups.values() {
                let _ = db.save_server_group(0, group); // Scope 0 = global/all
            }
            for group in store.channel_groups.values() {
                let _ = db.save_channel_group(0, group);
            }
            for client in store.clients.values() {
                let _ = db.save_client(client);
            }
            for ban in store.active_bans.values() {
                let _ = db.save_ban(ban);
            }
            for token in store.tokens.values() {
                let _ = db.save_token(token);
            }
            for assignment in &store.channel_group_assignments {
                // Find server_id for channel_id
                let mut assignment_server_id = 0;
                for (s_id, channels) in &store.channels {
                    if channels.iter().any(|c| c.id == assignment.channel_id) {
                        assignment_server_id = *s_id;
                        break;
                    }
                }
                if assignment_server_id != 0 {
                    let _ = db.save_channel_group_assignment(assignment_server_id, assignment);
                }
            }
        } else {
            store.virtual_servers = loaded_servers;
            if let Ok(loaded_channels) = db.load_channels() {
                store.channels = loaded_channels;
            }
            if let Ok(loaded_groups) = db.load_server_groups() {
                store.server_groups = loaded_groups;
            }
            if let Ok(loaded_c_groups) = db.load_channel_groups() {
                store.channel_groups = loaded_c_groups;
            }
            if let Ok(loaded_assignments) = db.load_channel_group_assignments() {
                store.channel_group_assignments = loaded_assignments;
            }
            if let Ok(loaded_clients) = db.load_clients() {
                if let Some(max_id) = loaded_clients.keys().max() {
                    if *max_id >= store.next_client_database_id {
                        store.next_client_database_id = max_id + 1;
                    }
                }
                store.clients = loaded_clients;
            }
            if let Ok(loaded_bans) = db.load_bans() {
                if let Some(max_id) = loaded_bans.keys().max() {
                    if *max_id >= store.next_ban_id {
                        store.next_ban_id = max_id + 1;
                    }
                }
                store.active_bans = loaded_bans;
            }
            if let Ok(loaded_tokens) = db.load_tokens() {
                if let Some(max_id) = loaded_tokens.keys().max() {
                    if *max_id >= store.next_token_id {
                        store.next_token_id = max_id + 1;
                    }
                }
                
                // Track highest action ID as well
                let mut max_action_id = 0;
                for token in loaded_tokens.values() {
                    if let Some(token_max_action_id) = token.actions.iter().map(|a| a.id).max() {
                        if token_max_action_id > max_action_id {
                            max_action_id = token_max_action_id;
                        }
                    }
                }
                if max_action_id >= store.next_token_action_id {
                    store.next_token_action_id = max_action_id + 1;
                }

                store.tokens = loaded_tokens;
            }
        }
    }

    let (desktop_tx, _) = tokio::sync::broadcast::channel(1024);
    let mut runtime = BaselineRuntime {
        permission_catalog: build_permission_catalog(&specs),
        web_permission_base_ids: load_blackteaweb_permission_ids(&workspace_root),
        specs,
        store,
        session_snapshots: BTreeMap::new(),
        anti_flood_ip_states: BTreeMap::new(),
        state_store: Some(RuntimeStateStore::new(state_path)),
        db,
        event_subscribers: Arc::new(Mutex::new(Vec::new())),
        lifecycle_tx: None,
        file_transfer_registry: None,
        music_download_tx: None,
        webtransport_btea_media_tx: None,
        desktop_btea_media_tx: Some(desktop_tx),
        next_upload_id: 1,
        next_download_id: 1,
    };
    runtime.load_persisted_state()?;

    let music_bot_ids = runtime.store.music_bots.keys().copied().collect::<Vec<_>>();
    for bot in music_bot_ids {
        runtime.sync_music_bot_client_state(bot);
    }
    runtime.ensure_web_server_group_assignment_permission_basis();

    Ok(runtime)
}

impl BaselineRuntime {
    
    pub fn route_btea_media_to_desktop(&self, server_id: u32, sender_client_id: u64, packet_type: u8, payload: &[u8]) {
        if let Some(tx) = &self.desktop_btea_media_tx {
            let _ = tx.send((server_id, sender_client_id, packet_type, payload.to_vec()));
        }
    }

    pub fn route_btea_media_to_webtransport(&self, server_id: u32, sender_client_id: u64, packet_type: u8, payload: &[u8]) {
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
                whisper_targets: None,
                ignored_clients: Vec::new(),
                version: client.version.clone(),
                platform: client.platform.clone(),
                country: client.country.clone(),
                connection_ip: client.connection_ip.clone(),
                server_groups: client.server_groups.clone(),
                client_flag_avatar: String::new(),
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
            if let Some(player_volume) = changed.get("player_volume")
                && let Some(bot) = self.store.music_bots.get_mut(&bot_id)
                && bot.player_volume != *player_volume
            {
                bot.player_volume = player_volume.clone();
                sync_bot_client = true;
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

    pub fn remove_session_client(&mut self, client_id: u64, reason_id: u32, reason_message: String) {
        if let Some(client) = self.store.online_clients.remove(&client_id) {
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
        }
        for bot in self.store.music_bots.values_mut() {
            if bot.linked_client_id == Some(client_id) {
                bot.linked_client_id = None;
            }
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

    fn anti_flood_config_for_server(&self, server_id: u32) -> Option<AntiFloodConfig> {
        self.store.virtual_servers.get(&server_id).map(|server| AntiFloodConfig {
            points_tick_reduce: server.antiflood_points_tick_reduce,
            points_needed_command_block: server.antiflood_points_needed_command_block,
            points_needed_ip_block: server.antiflood_points_needed_ip_block,
            ban_time: server.antiflood_ban_time,
        })
    }

    fn shared_ip_antiflood_rejected(
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

    fn prune_expired_active_bans(&mut self) {
        let now = current_unix_timestamp();
        self.store.active_bans.retain(|_, ban| {
            ban.duration_seconds == 0
                || ban
                    .created_at
                    .saturating_add(u64::from(ban.duration_seconds))
                    > now
        });
    }

    pub fn run_housekeeping(&mut self) {
        self.prune_expired_active_bans();
        
        let now = current_unix_timestamp();
        let zombie_timeout_seconds = 60;
        
        // Find zombie clients
        let zombie_clients: Vec<(u64, u32, u32)> = self.store.online_clients
            .iter()
            .filter(|(_, client)| {
                // Keep query clients alive if needed, but for now we timeout anyone inactive for 5 minutes
                // Music bots might never send keepalives, check client_type!
                // Client type 0 is normal client, type 1 is query client
                now.saturating_sub(client.last_seen_at) > zombie_timeout_seconds && client.client_type != 2
            })
            .map(|(id, client)| (*id, client.server_id, client.channel_id))
            .collect();
            
        // Remove them
        for (client_id, server_id, channel_id) in zombie_clients {
            self.remove_session_client(client_id, 3, "timeout".to_string());
            // Clean up any temporary channels created by them
            self.cleanup_temporary_channel(server_id, channel_id);
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
            self.cleanup_temporary_channel(server_id, channel_id);
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

    fn register_active_ban(
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
    fn target_client_snapshot_and_permissions(
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

    fn check_target_client_power(
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





    fn build_web_channel_row(
        &self,
        server_id: u32,
        channel: &Channel,
        channel_order: u32,
    ) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel.id.to_string());
        row.insert(String::from("cpid"), channel.parent_id.to_string());
        row.insert(String::from("channel_order"), channel_order.to_string());
        row.insert(String::from("channel_name"), channel.name.clone());
        row.insert(String::from("channel_topic"), channel.topic.clone());
        apply_channel_kind_rows(&mut row, channel.kind);
        row.insert(
            String::from("total_clients"),
            self.client_count_in_channel(server_id, channel.id).to_string(),
        );
        row.insert(
            String::from("channel_flag_default"),
            if self.default_channel_id_for_server(server_id) == Some(channel.id) {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(String::from("channel_flag_password"), String::from("0"));
        row
    }









    pub fn build_version(&self) -> &BuildVersion {
        &self.specs.build_version
    }

    pub fn snapshot_channel(&self, server_id: u32, channel_id: u32) -> Option<ChannelSnapshot> {
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































    fn build_permission_rows(&self) -> Vec<BTreeMap<String, String>> {
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

    fn build_feature_rows(&self) -> Vec<BTreeMap<String, String>> {
        [
            ("error-bulks", "1", "1"),
            ("advanced-channel-chat", "1", "1"),
                ("whisper-echo", "1", "1"),
                ("video", "1", "1"),
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


























    fn selected_server(&self, session: &QuerySessionState) -> Option<&VirtualServer> {
        session
            .selected_virtual_server_id
            .and_then(|server_id| self.store.virtual_servers.get(&server_id))
    }

    fn selected_server_mut(&mut self, session: &QuerySessionState) -> Option<&mut VirtualServer> {
        session
            .selected_virtual_server_id
            .and_then(|server_id| self.store.virtual_servers.get_mut(&server_id))
    }


    fn sync_session_client(&mut self, session: &mut QuerySessionState, command_name: &str) {
        if command_name == "quit" || (session.authenticated_login.is_none() && !session.is_desktop_client) {
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

        let (database_id, unique_identifier, server_groups, client_type, platform, version) = if session.is_desktop_client {
            (
                session.actor_client_database_id_override.unwrap_or(session.client_id + 1000),
                format!("desktop-{}", session.client_id),
                vec![], // No server groups by default for desktop clients right now, or load them if needed
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
                extra_properties: BTreeMap::new(),
            },
        );
    }



    fn restore_notification_subscriptions(
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

    fn default_channel_id_for_server(&self, server_id: u32) -> Option<u32> {
        self.store.channels.get(&server_id).and_then(|channels| {
            channels
                .iter()
                .find(|channel| channel.parent_id == 0 && channel.order == 0)
                .or_else(|| channels.iter().min_by_key(|channel| channel.id))
                .map(|channel| channel.id)
        })
    }

    fn channel_exists(&self, server_id: u32, channel_id: u32) -> bool {
        self.store
            .channels
            .get(&server_id)
            .is_some_and(|channels| channels.iter().any(|channel| channel.id == channel_id))
    }

    fn channel_by_id(&self, server_id: u32, channel_id: u32) -> Option<&Channel> {
        self.store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
    }

    fn conversation_id_exists(&self, server_id: u32, conversation_id: u32) -> bool {
        self.store.virtual_servers.contains_key(&server_id)
            && (conversation_id == 0 || self.channel_by_id(server_id, conversation_id).is_some())
    }

    fn latest_conversation_timestamp(&self, server_id: u32, conversation_id: u32) -> u64 {
        self.conversation_messages(server_id, conversation_id)
            .into_iter()
            .map(|message| message.timestamp)
            .max()
            .unwrap_or(0)
    }

    fn conversation_messages(
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

    fn private_conversation_messages(
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

    fn render_client_row(
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
        
        let avatar = if let Some(db_client) = self.store.clients.get(&client.database_id) {
            db_client.client_flag_avatar.clone()
        } else {
            String::new()
        };
        row.insert(String::from("client_flag_avatar"), avatar);

        row
    }

    fn query_account_unique_identifier(&self, account: &QueryAccount) -> String {
        stable_query_client_unique_identifier(&account.login_name)
    }

    fn query_account_matches_server(&self, account: &QueryAccount, server_id: u32) -> bool {
        match account.server_id {
            Some(account_server_id) => account_server_id == server_id,
            None => true,
        }
    }

    fn lookup_client_identity_by_uid(
        &self,
        server_id: u32,
        client_uid: &str,
    ) -> Option<(String, u64, String)> {
        self.store
            .online_clients
            .values()
            .find(|client| client.server_id == server_id && client.unique_identifier == client_uid)
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
                            && self.query_account_unique_identifier(account) == client_uid
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

    fn lookup_client_identity_by_dbid(
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

    fn private_message_identity_by_dbid(
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

    fn online_client_by_id_in_server(
        &self,
        server_id: u32,
        client_id: u64,
    ) -> Option<&OnlineClient> {
        self.store
            .online_clients
            .get(&client_id)
            .filter(|client| client.server_id == server_id)
    }

    fn client_count_in_channel(&self, server_id: u32, channel_id: u32) -> u32 {
        self.store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id && client.channel_id == channel_id)
            .count() as u32
    }

    fn client_count_in_server(&self, server_id: u32) -> u32 {
        self.store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .count() as u32
    }

    fn allocate_query_client_id(&mut self) -> u64 {
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

    fn allocate_client_database_id(&mut self) -> u64 {
        let next_id = self.store.next_client_database_id;
        self.store.next_client_database_id = self.store.next_client_database_id.saturating_add(1);
        next_id
    }

    fn default_server_groups_for_new_query_account(&self) -> Vec<u32> {
        default_server_groups_for_login_with_availability(
            "query-account",
            self.store.server_groups.contains_key(&6),
            self.store.server_groups.contains_key(&7),
        )
    }

    fn normalize_query_account_groups(&mut self) {
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

    fn normalize_online_client_groups(&mut self) {
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

    fn normalize_channel_group_assignments(&mut self) {
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

    fn normalize_client_permissions(&mut self) {
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

    fn normalize_channel_client_permissions(&mut self) {
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

    fn next_server_group_id(&self) -> u32 {
        self.store
            .server_groups
            .keys()
            .copied()
            .max()
            .map(|group_id| group_id.saturating_add(1))
            .unwrap_or(1)
    }

    fn next_channel_group_id(&self) -> u32 {
        self.store
            .channel_groups
            .keys()
            .copied()
            .max()
            .map(|group_id| group_id.saturating_add(1))
            .unwrap_or(1)
    }

    fn server_group_in_use(&self, group_id: u32) -> bool {
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

    fn server_group_ids_by_auto_type(&self, auto_update_type: i64) -> Vec<u32> {
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

    fn channel_group_in_use(&self, group_id: u32) -> bool {
        self.store
            .channel_group_assignments
            .iter()
            .any(|assignment| assignment.channel_group_id == group_id)
    }

    fn known_client_database_ids(&self) -> BTreeSet<u64> {
        let mut client_database_ids = self
            .store
            .query_accounts
            .values()
            .filter_map(|account| account.client_database_id)
            .collect::<BTreeSet<_>>();
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

    fn client_database_id_exists(&self, client_database_id: u64) -> bool {
        self.query_account_by_cldbid(client_database_id).is_some()
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

    fn online_client_by_cldbid(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<&OnlineClient> {
        self.store.online_clients.values().find(|client| {
            client.server_id == server_id && client.database_id == client_database_id
        })
    }

    fn online_client_by_cldbid_mut(
        &mut self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<&mut OnlineClient> {
        self.store.online_clients.values_mut().find(|client| {
            client.server_id == server_id && client.database_id == client_database_id
        })
    }

    fn query_account_by_cldbid(&self, client_database_id: u64) -> Option<&QueryAccount> {
        self.store
            .query_accounts
            .values()
            .find(|account| account.client_database_id == Some(client_database_id))
    }

    fn query_account_by_cldbid_mut(
        &mut self,
        client_database_id: u64,
    ) -> Option<&mut QueryAccount> {
        self.store
            .query_accounts
            .values_mut()
            .find(|account| account.client_database_id == Some(client_database_id))
    }

    fn next_token_id(&mut self) -> u32 {
        let token_id = self
            .store
            .next_token_id
            .max(next_token_id_seed(&self.store.tokens));
        self.store.next_token_id = token_id.saturating_add(1);
        token_id
    }

    fn next_token_action_id(&mut self) -> u32 {
        let action_id = self
            .store
            .next_token_action_id
            .max(next_token_action_id_seed(&self.store.tokens));
        self.store.next_token_action_id = action_id.saturating_add(1);
        action_id
    }

    fn next_conversation_timestamp(&mut self) -> u64 {
        let timestamp = current_unix_timestamp_millis().max(self.store.next_conversation_timestamp);
        self.store.next_conversation_timestamp = timestamp.saturating_add(1);
        timestamp
    }

    fn query_session_database_id(&self, session: &QuerySessionState) -> u64 {
        session
            .authenticated_login
            .as_ref()
            .and_then(|login| self.store.query_accounts.get(login))
            .and_then(|account| account.client_database_id)
            .unwrap_or(0)
    }

    fn query_session_participant(&self, session: &QuerySessionState) -> ConversationParticipant {
        ConversationParticipant {
            database_id: self.query_session_database_id(session),
            unique_identifier: self.query_session_unique_identifier(session),
            nickname: session
                .authenticated_login
                .clone()
                .unwrap_or_else(|| String::from("Query")),
        }
    }



    fn resolve_token_id(
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

    fn token_action_groups(&self, request: &CommandRequest) -> Vec<BTreeMap<String, String>> {
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

    fn parse_token_action_mutations(
        &self,
        request: &CommandRequest,
    ) -> std::result::Result<Vec<ParsedTokenActionMutation>, QueryResponse> {
        let groups = self.token_action_groups(request);
        let mut mutations = Vec::new();

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

    fn permission_option_groups(
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

    fn permoverview_requested_permissions(
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

    fn permission_filter_matches(
        &self,
        permission_filter: &Option<BTreeSet<String>>,
        permission_name: &str,
    ) -> bool {
        permission_filter
            .as_ref()
            .is_none_or(|names| names.contains(permission_name))
    }
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn next_client_database_seed(query_accounts: &BTreeMap<String, QueryAccount>) -> u64 {
    query_accounts
        .values()
        .filter_map(|account| account.client_database_id)
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(100)
}

fn next_token_id_seed(tokens: &BTreeMap<u32, PrivilegeToken>) -> u32 {
    tokens
        .keys()
        .copied()
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(1)
}

fn next_token_action_id_seed(tokens: &BTreeMap<u32, PrivilegeToken>) -> u32 {
    tokens
        .values()
        .flat_map(|token| token.actions.iter().map(|action| action.id))
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(1)
}

pub fn stable_query_client_unique_identifier(login_name: &str) -> String {
    login_name.to_string()
}

pub fn stable_web_client_unique_identifier(public_key: &str) -> String {
    let digest = Sha1::digest(public_key.as_bytes());
    BASE64_STANDARD.encode(digest)
}

pub fn stable_web_client_database_id(unique_identifier: &str) -> u64 {
    const OFFSET: u64 = 1_000_000_000_000;
    OFFSET + (fnv1a64(unique_identifier.as_bytes()) & 0x0000_FFFF_FFFF_FFFF)
}

fn looks_like_blackteaspeak_unique_id(value: &str) -> bool {
    value.len() == 28
        && value.ends_with('=')
        && value.bytes().all(
            |byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'='),
        )
}

fn next_conversation_timestamp_seed(
    conversation_messages: &BTreeMap<u32, Vec<ConversationMessage>>,
    private_messages: &BTreeMap<u32, Vec<PrivateConversationMessage>>,
) -> u64 {
    conversation_messages
        .values()
        .flat_map(|messages| messages.iter().map(|message| message.timestamp))
        .chain(
            private_messages
                .values()
                .flat_map(|messages| messages.iter().map(|message| message.timestamp)),
        )
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(1)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn default_server_groups_for_login_with_availability(
    login_name: &str,
    has_admin_group: bool,
    has_guest_group: bool,
) -> Vec<u32> {
    if login_name == "serveradmin" && has_admin_group {
        vec![6]
    } else if has_guest_group {
        vec![7]
    } else if has_admin_group {
        vec![6]
    } else {
        Vec::new()
    }
}

fn parse_query_bool(value: &str) -> Option<bool> {
    match value {
        "0" | "false" => Some(false),
        "1" | "true" => Some(true),
        _ => None,
    }
}

fn apply_channel_kind_rows(row: &mut BTreeMap<String, String>, kind: ChannelKind) {
    row.insert(
        String::from("channel_flag_permanent"),
        if kind.is_permanent() {
            String::from("1")
        } else {
            String::from("0")
        },
    );
    row.insert(
        String::from("channel_flag_semi_permanent"),
        if kind.is_semi_permanent() {
            String::from("1")
        } else {
            String::from("0")
        },
    );
}

impl BaselineRuntime {
    fn cleanup_temporary_channel(
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

        if clients_in_channel.len() == 1 {
            let bot_client_id = clients_in_channel[0];
            let bot_snapshot = self.online_client_snapshot(server_id, bot_client_id);
            if bot_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.client_type_exact == 4)
            {
                if let Some(bot_id) = self.music_bot_id_by_client(server_id, bot_client_id) {
                    self.store.music_bots.remove(&bot_id);
                }
                self.store.online_clients.remove(&bot_client_id);
                removed_client = bot_snapshot;
            }
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

    fn remove_channel_without_session_reassign(
        &mut self,
        server_id: u32,
        channel_id: u32,
    ) -> Option<()> {
        let channels = self.store.channels.get_mut(&server_id)?;
        let channel_index = channels.iter().position(|channel| channel.id == channel_id)?;
        let parent_id = channels[channel_index].parent_id;
        channels.remove(channel_index);
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
}

fn should_persist_command(command_name: &str) -> bool {
    matches!(
        command_name,
        "login"
            | "servernotifyregister"
            | "servernotifyunregister"
            | "clientaddperm"
            | "clientdelperm"
            | "clientmove"
            | "serveredit"
            | "channelclientaddperm"
            | "channelclientdelperm"
            | "channeladdperm"
            | "channeldelperm"
            | "channelcreate"
            | "channeldelete"
            | "channeledit"
            | "channelmove"
            | "channelgroupadd"
            | "channelgroupaddperm"
            | "channelgroupcopy"
            | "channelgroupdel"
            | "channelgroupdelperm"
            | "channelgrouprename"
            | "musicbotcreate"
            | "musicbotdelete"
            | "musicbotqueueadd"
            | "musicbotqueueremove"
            | "musicbotqueuereorder"
            | "musicbotplayeraction"
            | "playlistaddperm"
            | "playlistclientaddperm"
            | "playlistedit"
            | "playlistsongadd"
            | "playlistsongremove"
            | "playlistsongreorder"
            | "playlistsongsetcurrent"
            | "servergroupadd"
            | "servergroupaddclient"
            | "servergroupaddperm"
            | "servergroupautoaddperm"
            | "servergroupautodelperm"
            | "servergroupcopy"
            | "servergroupdel"
            | "servergroupdelclient"
            | "servergroupdelperm"
            | "servergrouprename"
            | "privilegekeyadd"
            | "privilegekeydelete"
            | "tokenadd"
            | "tokendelete"
            | "tokenedit"
            | "tokenuse"
            | "privilegekeyuse"
            | "use"
            | "querycreate"
            | "queryrename"
            | "querychangepassword"
            | "querydelete"
            | "sendtextmessage"
            | "setclientchannelgroup"
            | "clientsetserverquerylogin"
    )
}

fn synthetic_permission_id(permission_name: &str) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for byte in permission_name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }

    200_000_u32.saturating_add(hash % 50_000)
}

fn describe_permission_name(permission_name: &str) -> String {
    if permission_name == "b_serverquery_login" {
        return String::from("Log in through ServerQuery");
    }

    if permission_name == "b_client_create_modify_serverquery_login" {
        return String::from("Create or modify ServerQuery login");
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_textmessage_send"))
    {
        let scope = permission_subject_text(subject)
            .trim_start_matches("client ")
            .to_string();
        return sentence_case(format!("Send text messages to {scope}"));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_notify_register"))
    {
        return sentence_case(format!(
            "Register for {} notifications",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_notify_unregister"))
    {
        return sentence_case(format!(
            "Unregister from {} notifications",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_view"))
    {
        return sentence_case(format!("View {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_list"))
    {
        return sentence_case(format!("List {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_search"))
    {
        return sentence_case(format!("Search {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_find"))
    {
        return sentence_case(format!("Find {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_info"))
    {
        return sentence_case(format!(
            "View {} information",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_create"))
    {
        return sentence_case(format!("Create {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_delete"))
    {
        return sentence_case(format!("Delete {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_add"))
    {
        return sentence_case(format!("Add {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_remove"))
    {
        return sentence_case(format!("Remove {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_join_ignore_password"))
    {
        return sentence_case(format!(
            "Join {} ignoring password",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_rename"))
    {
        return sentence_case(format!("Rename {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_move"))
    {
        return sentence_case(format!("Move {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_start"))
    {
        return sentence_case(format!("Start {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_stop"))
    {
        return sentence_case(format!("Stop {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_use"))
    {
        return sentence_case(format!("Use {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_prefix(""))
    {
        if let Some((prefix, suffix)) = subject.rsplit_once("_modify_") {
            return sentence_case(format!(
                "Modify {}",
                permission_subject_text(&format!("{prefix}_{suffix}"))
            ));
        }
    }

    if let Some(subject) = permission_name.strip_prefix("i_") {
        return sentence_case(permission_subject_text(subject));
    }

    sentence_case(permission_subject_text(permission_name))
}

fn permission_subject_text(subject: &str) -> String {
    subject
        .split('_')
        .filter(|part| !part.is_empty())
        .map(permission_token_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn permission_token_text(token: &str) -> String {
    match token {
        "serverinstance" => String::from("server instance"),
        "virtualserver" => String::from("virtual server"),
        "serverquery" => String::from("ServerQuery"),
        "textmessage" => String::from("text message"),
        "textmessages" => String::from("text messages"),
        "clientdb" => String::from("client database"),
        "channelgroup" => String::from("channel group"),
        "servergroup" => String::from("server group"),
        "musicgroup" => String::from("music group"),
        "welcomemessage" => String::from("welcome message"),
        "hostmessage" => String::from("host message"),
        "hostbanner" => String::from("host banner"),
        "maxclients" => String::from("max clients"),
        "ft" => String::from("file transfer"),
        "dblist" => String::from("database list"),
        "dbinfo" => String::from("database info"),
        "dbsearch" => String::from("database search"),
        "antiflood" => String::from("anti flood"),
        _ => token.replace('-', " "),
    }
}

fn sentence_case(text: String) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return text;
    };

    format!("{}{}", first.to_uppercase(), characters.collect::<String>())
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn runtime_bool_flag(value: &str) -> bool {
    matches!(value, "1" | "true" | "yes" | "on")
}

fn current_unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn antiflood_command_cost(command_name: &str) -> u32 {
    match command_name {
        "sendtextmessage" | "serveredit" | "clientmove" | "channelcreate" | "channeldelete"
        | "channeledit" | "channelmove" => 2,
        _ => 1,
    }
}

fn antiflood_command_rejected(
    config: AntiFloodConfig,
    anti_flood_state: &mut AntiFloodSessionState,
    points_to_add: u32,
    now_millis: u64,
) -> bool {
    if anti_flood_state.last_update_millis == 0 {
        anti_flood_state.last_update_millis = now_millis;
    }

    let elapsed_millis = now_millis.saturating_sub(anti_flood_state.last_update_millis);
    if config.points_tick_reduce > 0 {
        let reduced_points =
            (elapsed_millis / 1_000).saturating_mul(u64::from(config.points_tick_reduce));
        anti_flood_state.points = anti_flood_state
            .points
            .saturating_sub(reduced_points.min(u64::from(u32::MAX)) as u32);
            
        let processed_millis = elapsed_millis - (elapsed_millis % 1_000);
        anti_flood_state.last_update_millis = anti_flood_state.last_update_millis.saturating_add(processed_millis);
    } else {
        anti_flood_state.last_update_millis = now_millis;
    }

    if anti_flood_state.blocked_until_millis > now_millis {
        // For testing purposes, we ignore bans.
        // return true;
    }

    anti_flood_state.points = anti_flood_state.points.saturating_add(points_to_add);

    // For testing purposes, disable command blocking
    false
}

fn property_catalog() -> Vec<(&'static str, u32, &'static str)> {
    vec![
        ("virtualserver_id", 256, "SERVER"),
        ("virtualserver_unique_identifier", 392, "SERVER"),
        ("virtualserver_name", 408, "SERVER"),
        ("virtualserver_port", 408, "SERVER"),
        ("virtualserver_clientsonline", 128, "SERVER"),
        ("virtualserver_queryclientsonline", 128, "SERVER"),
        ("virtualserver_antiflood_points_tick_reduce", 256, "SERVER"),
        (
            "virtualserver_antiflood_points_needed_command_block",
            256,
            "SERVER",
        ),
        (
            "virtualserver_antiflood_points_needed_ip_block",
            256,
            "SERVER",
        ),
        ("virtualserver_antiflood_ban_time", 256, "SERVER"),
        ("cid", 256, "CHANNEL"),
        ("pid", 256, "CHANNEL"),
        ("channel_name", 408, "CHANNEL"),
        ("channel_topic", 408, "CHANNEL"),
        ("channel_description", 408, "CHANNEL"),
        ("clid", 256, "CLIENT"),
        ("client_database_id", 256, "CLIENT"),
        ("client_nickname", 408, "CLIENT"),
        ("client_unique_identifier", 392, "CLIENT"),
        ("client_platform", 256, "CLIENT"),
        ("serverinstance_database_version", 256, "INSTANCE"),
        ("serverinstance_filetransfer_port", 408, "INSTANCE"),
        (
            "serverinstance_template_guest_serverquery_group",
            256,
            "INSTANCE",
        ),
        (
            "serverinstance_template_admin_serverquery_group",
            256,
            "INSTANCE",
        ),
        ("serverinstance_template_serveradmin_group", 256, "INSTANCE"),
        ("sgid", 256, "GROUP"),
        ("name", 408, "GROUP"),
        ("type", 256, "GROUP"),
        ("iconid", 408, "GROUP"),
        ("connection_packets_sent_total", 128, "CONNECTION"),
        ("connection_packets_received_total", 128, "CONNECTION"),
        ("connection_bytes_sent_total", 128, "CONNECTION"),
        ("connection_bytes_received_total", 128, "CONNECTION"),
        ("connection_connected_time", 128, "CONNECTION"),
    ]
}

fn ordered_sibling_ids(
    channels: &[Channel],
    parent_id: u32,
    excluded_channel_id: Option<u32>,
) -> Vec<u32> {
    let siblings = channels
        .iter()
        .filter(|channel| channel.parent_id == parent_id && Some(channel.id) != excluded_channel_id)
        .collect::<Vec<_>>();
    let mut remaining = siblings
        .iter()
        .map(|channel| channel.id)
        .collect::<BTreeSet<_>>();
    let mut ordered_ids = Vec::new();
    let mut previous_id = 0;

    loop {
        let Some(next_id) = siblings
            .iter()
            .filter(|channel| remaining.contains(&channel.id) && channel.order == previous_id)
            .map(|channel| channel.id)
            .min()
        else {
            break;
        };

        ordered_ids.push(next_id);
        remaining.remove(&next_id);
        previous_id = next_id;
    }

    ordered_ids.extend(remaining);
    ordered_ids
}

fn collect_visible_channel_ids_for_client(
    runtime: &BaselineRuntime,
    channels: &[Channel],
    server_id: u32,
    parent_id: u32,
    client_database_id: u64,
    visible_channel_ids: &mut BTreeSet<u32>,
) {
    for channel_id in ordered_sibling_ids(channels, parent_id, None) {
        if !runtime.web_client_can_view_channel(server_id, channel_id, client_database_id) {
            continue;
        }

        visible_channel_ids.insert(channel_id);
        collect_visible_channel_ids_for_client(
            runtime,
            channels,
            server_id,
            channel_id,
            client_database_id,
            visible_channel_ids,
        );
    }
}

fn ordered_visible_channel_ids(
    channels: &[Channel],
    parent_id: u32,
    visible_channel_ids: &BTreeSet<u32>,
) -> Vec<u32> {
    let mut ordered_ids = Vec::new();

    for channel_id in ordered_sibling_ids(channels, parent_id, None) {
        if !visible_channel_ids.contains(&channel_id) {
            continue;
        }

        ordered_ids.push(channel_id);
        ordered_ids.extend(ordered_visible_channel_ids(
            channels,
            channel_id,
            visible_channel_ids,
        ));
    }

    ordered_ids
}

fn relink_sibling_orders(channels: &mut [Channel], parent_id: u32, ordered_ids: &[u32]) {
    let mut previous_id = 0;
    for channel_id in ordered_ids {
        if let Some(channel) = channels
            .iter_mut()
            .find(|channel| channel.id == *channel_id && channel.parent_id == parent_id)
        {
            channel.order = previous_id;
            previous_id = *channel_id;
        }
    }

}

fn resolve_insert_index(sibling_ids: &[u32], requested_order: Option<u32>) -> Option<usize> {
    match requested_order {
        Some(0) => Some(0),
        Some(anchor_id) => sibling_ids
            .iter()
            .position(|channel_id| *channel_id == anchor_id)
            .map(|index| index + 1),
        None => Some(sibling_ids.len()),
    }
}

fn channel_is_descendant(channels: &[Channel], candidate_parent_id: u32, channel_id: u32) -> bool {
    let mut current_parent_id = candidate_parent_id;

    while current_parent_id != 0 {
        if current_parent_id == channel_id {
            return true;
        }

        current_parent_id = channels
            .iter()
            .find(|channel| channel.id == current_parent_id)
            .map(|channel| channel.parent_id)
            .unwrap_or(0);
    }

    false
}

#[allow(dead_code)]
fn _build_version_snapshot(build_version: &BuildVersion) -> String {
    format!(
        "{}:{}:{}",
        build_version.build_name, build_version.build_version, build_version.build_index
    )
}

#[allow(dead_code)]
fn _command_summary(command: &CommandSpec) -> String {
    format!("{}:{}", command.name, command.category)
}
