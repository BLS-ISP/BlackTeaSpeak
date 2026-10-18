use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use std::sync::{Arc, Mutex};
use sha1::{Digest, Sha1};

use crate::models::{BuildVersion, CommandSpec, PermissionCatalogEntry};
use crate::query::{CommandRequest, QueryResponse, parse_request_line, render_response};
use crate::specs::FoundationSpecs;
use crate::state::{
    PersistedChannel, PersistedChannelClientPermissionTarget, PersistedChannelGroup,
    PersistedChannelGroupAssignment, PersistedClientComplaint,
    PersistedClientPermissionTarget, PersistedConversationMessage, PersistedCustomClientProperty,
    PersistedMusicBot, PersistedMusicBotState, PersistedMusicQueueEntry,
    PersistedNotificationSubscription, PersistedOfflineMessage, PersistedPermissionAssignment,
    PersistedPlaylistClientPermissionTarget, PersistedPrivateConversationMessage,
    PersistedQueryAccount, PersistedRuntimeState, PersistedServerGroup, PersistedServerLogEntry,
    PersistedSessionSnapshot, PersistedToken, PersistedTokenAction, PersistedVirtualServer,
    RUNTIME_STATE_SCHEMA_VERSION, RuntimeStateStore, VirtualServerSnapshotPayload,
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
pub use self::musicbot::audio_engine::spawn_music_bot_audio_engine;
use self::musicbot::{MusicBot, MusicBotState, MusicQueueEntry, PlaylistClientPermissionTarget};
use self::permissions::{
    build_named_permission_map, build_permission_catalog, check_channel_delete_power_allowed, check_channel_modify_power_allowed,
    check_required_permission, load_blackteaweb_permission_ids, permission_value_or_default,
    seed_store, session_has_permission_actor,
};

pub const ERROR_CLIENT_IS_FLOODING: u32 = 524;
pub const ERROR_DATABASE_EMPTY_RESULT: u32 = 0x501;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AntiFloodConfig {
    pub(crate) points_tick_reduce: u32,
    pub(crate) points_needed_command_block: u32,
    pub(crate) points_needed_ip_block: u32,
    pub(crate) ban_time: u32,
}

#[derive(Debug, Clone)]
pub struct InMemoryStore {
    pub(crate) query_accounts: BTreeMap<String, QueryAccount>,
    pub(crate) server_groups: BTreeMap<u32, ServerGroup>,
    pub(crate) channel_groups: BTreeMap<u32, ChannelGroup>,
    pub(crate) virtual_servers: BTreeMap<u32, VirtualServer>,
    pub channels: BTreeMap<u32, Vec<Channel>>,
    pub(crate) channel_group_assignments: Vec<ChannelGroupAssignment>,
    pub(crate) channel_client_permissions: Vec<ChannelClientPermissionTarget>,
    pub(crate) client_permissions: Vec<ClientPermissionTarget>,
    pub(crate) conversation_messages: BTreeMap<u32, Vec<ConversationMessage>>,
    pub(crate) private_messages: BTreeMap<u32, Vec<PrivateConversationMessage>>,
    pub(crate) offline_messages: BTreeMap<u64, OfflineMessage>,
    pub(crate) complaints: Vec<ClientComplaint>,
    pub(crate) custom_client_properties: BTreeMap<(u32, u64), BTreeMap<String, String>>,
    pub(crate) server_logs: Vec<ServerLogEntry>,
    pub(crate) instance_properties: BTreeMap<String, String>,
    pub(crate) tokens: BTreeMap<u32, PrivilegeToken>,
    pub(crate) server_temp_passwords: Vec<ServerTempPassword>,
    pub(crate) active_bans: BTreeMap<u32, ActiveBan>,
    pub online_clients: BTreeMap<u64, OnlineClient>,
    pub(crate) clients: BTreeMap<u64, Client>,
    pub(crate) music_bots: BTreeMap<u32, MusicBot>,
    pub(crate) active_broadcasts: BTreeMap<(u32, u64, u32), ActiveMediaBroadcast>,
    pub(crate) next_query_client_id: u64,
    pub(crate) next_client_database_id: u64,
    pub(crate) next_conversation_timestamp: u64,
    pub(crate) next_ban_id: u32,
    pub(crate) next_token_id: u32,
    pub(crate) next_token_action_id: u32,
    pub(crate) next_offline_message_id: u64,
    pub(crate) next_log_id: u64,
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
    pub store: InMemoryStore,
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
    pub rtc_manager: Arc<crate::rtc_manager::RtcBroadcastManager>,
    pub next_upload_id: u64,
    pub next_download_id: u64,
    pub(crate) pending_temp_cleanups: Vec<TemporaryChannelCleanup>,
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

    let is_test = state_path.as_ref().starts_with(std::env::temp_dir());
    if !is_test {
        store
            .clients
            .retain(|_, c| !c.unique_identifier.starts_with("compat-seed-user-"));
        store
            .online_clients
            .retain(|_, c| !c.unique_identifier.starts_with("compat-seed-user-"));
        store
            .channel_group_assignments
            .retain(|a| !matches!(a.client_database_id, 40..=42));
        store.music_bots.retain(|_, b| b.client_database_id != 42);
    }

    let db_path = if is_test {
        state_path.as_ref().with_extension("db")
    } else {
        workspace_root.join("blackteaspeak.db")
    };
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    tracing::debug!("db_path resolved = {:?}", db_path);
    let db = crate::database::Database::new(db_path).expect("Failed to initialize database");
    if !is_test {
        let _ = db.purge_compat_seed_users();
    }

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
                if !loaded_channels.is_empty() {
                    store.channels = loaded_channels;
                }
            }
            if let Ok(loaded_groups) = db.load_server_groups() {
                if loaded_groups.is_empty() {
                    for group in store.server_groups.values() {
                        let _ = db.save_server_group(0, group);
                    }
                } else {
                    for (id, group) in loaded_groups {
                        store.server_groups.insert(id, group);
                    }
                }
            }
            if let Ok(loaded_c_groups) = db.load_channel_groups() {
                if loaded_c_groups.is_empty() {
                    for group in store.channel_groups.values() {
                        let _ = db.save_channel_group(0, group);
                    }
                } else {
                    for (id, group) in loaded_c_groups {
                        store.channel_groups.insert(id, group);
                    }
                }
            }
            if let Ok(loaded_assignments) = db.load_channel_group_assignments() {
                if !loaded_assignments.is_empty() {
                    store.channel_group_assignments = loaded_assignments;
                }
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

    let ft_root = if is_test {
        state_path.as_ref().with_extension("ft-workspace")
    } else {
        workspace_root.clone()
    };
    let default_ft_registry = Arc::new(crate::file_transfer::FileTransferRegistry::new(&ft_root));

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
        file_transfer_registry: Some(default_ft_registry),
        music_download_tx: None,
        webtransport_btea_media_tx: None,
        desktop_btea_media_tx: Some(desktop_tx),
        rtc_manager: Arc::new(crate::rtc_manager::RtcBroadcastManager::new()),
        next_upload_id: 1,
        next_download_id: 1,
        pending_temp_cleanups: Vec::new(),
    };
    runtime.load_persisted_state()?;

    if !is_test {
        let had_seed_bot = runtime
            .store
            .music_bots
            .values()
            .any(|b| b.client_database_id == 42);
        runtime
            .store
            .music_bots
            .retain(|_, b| b.client_database_id != 42);
        runtime.store.online_clients.retain(|_, c| {
            !c.unique_identifier.starts_with("compat-seed-user-") && c.database_id != 42
        });
        runtime
            .store
            .clients
            .retain(|_, c| !c.unique_identifier.starts_with("compat-seed-user-"));
        runtime
            .store
            .channel_group_assignments
            .retain(|a| !matches!(a.client_database_id, 40..=42));
        if had_seed_bot {
            let _ = runtime.persist_state();
        }
    }

    let music_bot_ids = runtime.store.music_bots.keys().copied().collect::<Vec<_>>();
    for bot in music_bot_ids {
        runtime.sync_music_bot_client_state(bot);
    }
    runtime.ensure_web_server_group_assignment_permission_basis();

    Ok(runtime)
}


mod client_and_ban_state;
mod session_and_identity;
mod normalization_and_helpers;
pub mod catalog_and_utils;
pub use catalog_and_utils::*;
