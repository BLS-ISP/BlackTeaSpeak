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
mod permissions;
mod ytdlp;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationEventKind {
    Server,
    Channel,
    TextServer,
    TextChannel,
    TextPrivate,
}

impl NotificationEventKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "server" => Some(Self::Server),
            "channel" => Some(Self::Channel),
            "textserver" => Some(Self::TextServer),
            "textchannel" => Some(Self::TextChannel),
            "textprivate" => Some(Self::TextPrivate),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Channel => "channel",
            Self::TextServer => "textserver",
            Self::TextChannel => "textchannel",
            Self::TextPrivate => "textprivate",
        }
    }

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebServerGroupMutationError {
    InvalidClient,
    InvalidGroup,
    PermissionDenied { failed_permission_id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSubscription {
    pub event: NotificationEventKind,
    pub channel_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMessageTarget {
    pub target_mode: u32,
    pub server_id: u32,
    pub channel_id: Option<u32>,
    pub target_client_id: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AntiFloodSessionState {
    pub last_update_millis: u64,
    pub points: u32,
    pub blocked_until_millis: u64,
    pub last_points_decay_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuerySessionState {
    pub client_id: u64,
    pub connection_ip: String,
    pub authenticated_login: Option<String>,
    pub selected_virtual_server_id: Option<u32>,
    pub current_channel_id: Option<u32>,
    pub virtual_mode: bool,
    pub notification_subscriptions: Vec<NotificationSubscription>,
    pub actor_client_database_id_override: Option<u64>,
    pub client_nickname: String,
    pub client_away: bool,
    pub client_away_message: String,
    pub client_input_muted: bool,
    pub client_output_muted: bool,
    pub is_desktop_client: bool,
    pub whisper_targets: Option<crate::models::WhisperTargetSelection>,
    pub ignored_clients: Vec<u64>,
    pub points: u32,
    pub blocked_until_millis: u64,
    pub last_points_decay_millis: u64,
}

impl QuerySessionState {
    pub fn effective_nickname(&self) -> String {
        let nickname = self.client_nickname.trim();
        if !nickname.is_empty() {
            nickname.to_string()
        } else {
            self.authenticated_login
                .clone()
                .unwrap_or_else(|| String::from("Query"))
        }
    }

    pub fn reset_client_state(&mut self) {
        self.client_nickname.clear();
        self.client_away = false;
        self.client_away_message.clear();
        self.client_input_muted = false;
        self.client_output_muted = false;
        self.actor_client_database_id_override = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelKind {
    Temporary,
    SemiPermanent,
    #[default]
    Permanent,
}

impl ChannelKind {
    pub(crate) fn from_flags(flag_permanent: bool, flag_semi_permanent: bool) -> Self {
        if flag_permanent {
            Self::Permanent
        } else if flag_semi_permanent {
            Self::SemiPermanent
        } else {
            Self::Temporary
        }
    }

    pub(crate) fn to_permanent_flag(self) -> u32 {
        match self {
            Self::Permanent => 1,
            _ => 0,
        }
    }

    pub(crate) fn to_semi_permanent_flag(self) -> u32 {
        match self {
            Self::SemiPermanent => 1,
            _ => 0,
        }
    }
}

impl ChannelKind {
    fn from_request_flags(request: &CommandRequest) -> Self {
        if request
            .named_args
            .get("channel_flag_temporary")
            .is_some_and(|value| runtime_bool_flag(value))
        {
            Self::Temporary
        } else if request
            .named_args
            .get("channel_flag_semi_permanent")
            .is_some_and(|value| runtime_bool_flag(value))
        {
            Self::SemiPermanent
        } else {
            Self::Permanent
        }
    }

    pub fn is_permanent(self) -> bool {
        matches!(self, Self::Permanent)
    }

    pub fn is_semi_permanent(self) -> bool {
        matches!(self, Self::SemiPermanent)
    }
}

impl From<PersistedChannelKind> for ChannelKind {
    fn from(value: PersistedChannelKind) -> Self {
        match value {
            PersistedChannelKind::Temporary => Self::Temporary,
            PersistedChannelKind::SemiPermanent => Self::SemiPermanent,
            PersistedChannelKind::Permanent => Self::Permanent,
        }
    }
}

impl From<ChannelKind> for PersistedChannelKind {
    fn from(value: ChannelKind) -> Self {
        match value {
            ChannelKind::Temporary => Self::Temporary,
            ChannelKind::SemiPermanent => Self::SemiPermanent,
            ChannelKind::Permanent => Self::Permanent,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QueryAccount {
    login_name: String,
    password: String,
    server_id: Option<u32>,
    client_database_id: Option<u64>,
    server_groups: Vec<u32>,
    permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub(crate) struct ServerGroup {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) group_type: u32,
    pub(crate) icon_id: i64,
    pub(crate) save_db: bool,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChannelGroup {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) group_type: u32,
    pub(crate) icon_id: i64,
    pub(crate) save_db: bool,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChannelGroupAssignment {
    pub(crate) channel_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) channel_group_id: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ChannelClientPermissionTarget {
    pub(crate) channel_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub(crate) struct ClientPermissionTarget {
    pub(crate) server_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) client_unique_identifier: String,
    pub(crate) client_nickname: String,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub(crate) struct Channel {
    pub(crate) id: u32,
    pub(crate) parent_id: u32,
    pub(crate) order: u32,
    pub(crate) kind: ChannelKind,
    pub(crate) name: String,
    pub(crate) topic: String,
    pub(crate) description: String,
    pub(crate) password: String,
    pub(crate) codec: u32,
    pub(crate) codec_quality: u32,
    pub(crate) maxclients: i32,
    pub(crate) maxfamilyclients: i32,
    pub(crate) flag_default: bool,
    pub(crate) flag_password: bool,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub struct VirtualServer {
    pub(crate) id: u32,
    pub(crate) port: u16,
    pub(crate) name: String,
    pub(crate) unique_identifier: String,
    pub(crate) welcome_message: String,
    pub(crate) host_message: String,
    pub(crate) host_message_mode: u32,
    pub(crate) ask_for_privilegekey: u32,
    pub(crate) max_clients: u32,
    pub(crate) antiflood_points_tick_reduce: u32,
    pub(crate) antiflood_points_needed_command_block: u32,
    pub(crate) antiflood_points_needed_ip_block: u32,
    pub(crate) antiflood_ban_time: u32,
}

impl VirtualServer {
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn port(&self) -> u16 {
        self.port
    }
}

#[derive(Debug, Clone)]
struct ConversationMessage {
    conversation_id: u32,
    timestamp: u64,
    sender_database_id: u64,
    sender_unique_id: String,
    sender_name: String,
    message: String,
}

#[derive(Debug, Clone)]
struct ConversationParticipant {
    database_id: u64,
    unique_identifier: String,
    nickname: String,
}

#[derive(Debug, Clone)]
struct PrivateConversationMessage {
    timestamp: u64,
    sender_database_id: u64,
    sender_unique_id: String,
    sender_name: String,
    target_database_id: u64,
    target_unique_id: String,
    target_name: String,
    message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TokenAction {
    pub(crate) id: u32,
    pub(crate) action_type: u32,
    pub(crate) action_id1: u32,
    pub(crate) action_id2: u32,
    pub(crate) action_text: String,
}

#[derive(Debug, Clone)]
enum ParsedTokenActionMutation {
    Add {
        action_type: u32,
        action_id1: u32,
        action_id2: u32,
        action_text: String,
    },
    Update {
        action_id: u32,
        action_type: u32,
        action_id1: u32,
        action_id2: u32,
        action_text: String,
    },
    Remove {
        action_id: u32,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PrivilegeToken {
    pub(crate) id: u32,
    pub(crate) server_id: u32,
    pub(crate) token: String,
    pub(crate) description: String,
    pub(crate) max_uses: u32,
    pub(crate) uses: u32,
    pub(crate) created_at: u64,
    pub(crate) owner_login: String,
    pub(crate) expired_at: Option<u64>,
    pub(crate) actions: Vec<TokenAction>,
}

#[derive(Debug, Clone)]
pub(crate) struct BanTrigger {
    pub(crate) client_unique_identifier: String,
    pub(crate) client_nickname: String,
    pub(crate) client_hardware_identifier: String,
    pub(crate) connection_client_ip: String,
    pub(crate) timestamp: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveBan {
    pub(crate) id: u32,
    pub(crate) server_id: u32,
    pub(crate) name: String,
    pub(crate) unique_identifier: String,
    pub(crate) hardware_identifier: String,
    pub(crate) ip: String,
    pub(crate) reason: String,
    pub(crate) created_at: u64,
    pub(crate) duration_seconds: u32,
    pub(crate) invoker_name: String,
    pub(crate) invoker_database_id: u64,
    pub(crate) invoker_unique_identifier: String,
    pub(crate) triggers: Vec<BanTrigger>,
}

#[derive(Debug, Clone)]
pub(crate) struct OnlineClient {
    id: u64,
    database_id: u64,
    unique_identifier: String,
    nickname: String,
    away: bool,
    away_message: String,
    input_muted: bool,
    output_muted: bool,
    server_id: u32,
    channel_id: u32,
    client_type: u32,
    version: String,
    platform: String,
    country: String,
    connection_ip: String,
    server_groups: Vec<u32>,
    connected_at: u64,
    pub(crate) last_seen_at: u64,
    extra_properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Client {
    pub(crate) database_id: u64,
    pub(crate) unique_identifier: String,
    pub(crate) nickname: String,
    pub(crate) description: String,
    pub(crate) created_at: u64,
    pub(crate) last_connected_at: u64,
    pub(crate) total_connections: u32,
    pub(crate) month_bytes_uploaded: u64,
    pub(crate) month_bytes_downloaded: u64,
    pub(crate) total_bytes_uploaded: u64,
    pub(crate) total_bytes_downloaded: u64,
    pub(crate) client_flag_avatar: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineClientSnapshot {
    pub id: u64,
    pub database_id: u64,
    pub unique_identifier: String,
    pub nickname: String,
    pub away: bool,
    pub away_message: String,
    pub input_muted: bool,
    pub output_muted: bool,
    pub server_id: u32,
    pub channel_id: u32,
    pub client_type: u32,
    pub client_type_exact: u32,
    pub whisper_targets: Option<crate::models::WhisperTargetSelection>,
    pub ignored_clients: Vec<u64>,
    pub version: String,
    pub platform: String,
    pub country: String,
    pub connection_ip: String,
    pub server_groups: Vec<u32>,
    pub client_flag_avatar: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSnapshot {
    pub id: u32,
    pub parent_id: u32,
    pub order: u32,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: String,
    pub description: String,
    pub total_clients: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSnapshot {
    pub id: u32,
    pub port: u16,
    pub name: String,
    pub unique_identifier: String,
    pub welcome_message: String,
    pub host_message: String,
    pub host_message_mode: u32,
    pub ask_for_privilegekey: u32,
    pub max_clients: u32,
    pub antiflood_points_tick_reduce: u32,
    pub antiflood_points_needed_command_block: u32,
    pub antiflood_points_needed_ip_block: u32,
    pub antiflood_ban_time: u32,
}

#[derive(Debug, Clone)]
pub struct TemporaryChannelCleanup {
    pub removed_client: Option<OnlineClientSnapshot>,
    pub removed_channel: Option<ChannelSnapshot>,
}

#[derive(Debug, Clone)]
pub struct WebServerInitInfo {
    pub server_id: u32,
    pub server_name: String,
    pub server_unique_identifier: String,
    pub server_port: u16,
    pub welcome_message: String,
    pub host_message: String,
    pub host_message_mode: u32,
    pub ask_for_privilegekey: u32,
    pub antiflood_points_tick_reduce: u32,
    pub antiflood_points_needed_command_block: u32,
    pub antiflood_points_needed_ip_block: u32,
    pub antiflood_ban_time: u32,
}

#[derive(Debug, Clone, Copy)]
struct AntiFloodConfig {
    points_tick_reduce: u32,
    points_needed_command_block: u32,
    points_needed_ip_block: u32,
    ban_time: u32,
}

#[derive(Debug, Clone)]
struct InMemoryStore {
    query_accounts: BTreeMap<String, QueryAccount>,
    server_groups: BTreeMap<u32, ServerGroup>,
    channel_groups: BTreeMap<u32, ChannelGroup>,
    virtual_servers: BTreeMap<u32, VirtualServer>,
    channels: BTreeMap<u32, Vec<Channel>>,
    channel_group_assignments: Vec<ChannelGroupAssignment>,
    channel_client_permissions: Vec<ChannelClientPermissionTarget>,
    client_permissions: Vec<ClientPermissionTarget>,
    conversation_messages: BTreeMap<u32, Vec<ConversationMessage>>,
    private_messages: BTreeMap<u32, Vec<PrivateConversationMessage>>,
    tokens: BTreeMap<u32, PrivilegeToken>,
    active_bans: BTreeMap<u32, ActiveBan>,
    online_clients: BTreeMap<u64, OnlineClient>,
    clients: BTreeMap<u64, Client>,
    music_bots: BTreeMap<u32, MusicBot>,
    next_query_client_id: u64,
    next_client_database_id: u64,
    next_conversation_timestamp: u64,
    next_ban_id: u32,
    next_token_id: u32,
    next_token_action_id: u32,
}

pub type EventCallback = Box<dyn Fn(&BaselineRuntime, u32, &crate::transport::TransportNotification) + Send + Sync>;

#[derive(Clone, Debug)]
pub enum LifecycleAction {
    StartVirtualServer { server_id: u32, port: u16 },
    StopVirtualServer { server_id: u32 },
}

pub struct BaselineRuntime {
    specs: FoundationSpecs,
    store: InMemoryStore,
    permission_catalog: BTreeMap<String, PermissionCatalogEntry>,
    web_permission_base_ids: BTreeMap<String, u32>,
    session_snapshots: BTreeMap<String, PersistedSessionSnapshot>,
    anti_flood_ip_states: BTreeMap<(u32, String), AntiFloodSessionState>,
    state_store: Option<RuntimeStateStore>,
    pub db: crate::database::Database,
    event_subscribers: Arc<Mutex<Vec<EventCallback>>>,
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

    pub fn subscribe_events(&self, callback: EventCallback) {
        self.event_subscribers.lock().unwrap().push(callback);
    }

    pub fn set_file_transfer_registry(&mut self, registry: std::sync::Arc<crate::file_transfer::FileTransferRegistry>) {
        self.file_transfer_registry = Some(registry);
    }

    pub fn broadcast_event(&self, server_id: u32, notification: &crate::transport::TransportNotification) {
        let subs = self.event_subscribers.lock().unwrap();
        for sub in subs.iter() {
            sub(self, server_id, notification);
        }
    }
    pub fn execute(&mut self, input: &str, session: &mut QuerySessionState) -> String {
        let response = match parse_request_line(input) {
            Ok(request) => self.execute_request(request, session),
            Err(error) => QueryResponse::error(1536, error.to_string()),
        };
        render_response(&response)
    }

    pub fn execute_request(
        &mut self,
        request: CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        self.prune_expired_active_bans();

        let command_name = request.command.clone();
        if let Some(server_id) = session.selected_virtual_server_id
            && let Some(config) = self.anti_flood_config_for_server(server_id)
        {
            let connection_ip = session.connection_ip.trim();
            if !connection_ip.is_empty() {
                let now_millis = current_unix_timestamp_millis();
                let points_to_add = antiflood_command_cost(&command_name);
                if self.shared_ip_antiflood_rejected(
                    config,
                    server_id,
                    connection_ip,
                    points_to_add,
                    now_millis,
                    false,
                ) {
                    return QueryResponse::error(ERROR_CLIENT_IS_FLOODING, "client is flooding");
                }
            }
        }

        let before_session = session.clone();
        let response = self.dispatch(request, session);
        self.sync_session_snapshot(&before_session, session);
        self.sync_session_client(session, &command_name);
        self.persist_state_best_effort();
        response
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

    pub fn persist_state_if_configured(&self) {
        self.persist_state_best_effort();
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

    fn dispatch(
        &mut self,
        request: CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        match request.command.as_str() {
            "help" => self.handle_help(&request),
            "login" => self.handle_login(&request, session),
            "logout" => self.handle_logout(session),
            "quit" => self.handle_quit(),
            "servernotifyregister" => self.handle_servernotifyregister(&request, session),
            "servernotifyunregister" => self.handle_servernotifyunregister(session),
            "sendtextmessage" => self.handle_sendtextmessage(&request, session),
            "clientpoke" => self.handle_clientpoke(&request, session),
            "clientkick" => self.handle_clientkick(&request, session),
            "banclient" => self.handle_banclient(&request, session),
            "querylist" => self.handle_querylist(&request, session),
            "clientfind" => self.handle_clientfind(&request, session),
            "clientgetids" => self.handle_clientgetids(&request, session),
            "clientgetdbidfromuid" => self.handle_clientgetdbidfromuid(&request, session),
            "clientgetnamefromdbid" => self.handle_clientgetnamefromdbid(&request, session),
            "clientgetnamefromuid" => self.handle_clientgetnamefromuid(&request, session),
            "clientgetuidfromclid" => self.handle_clientgetuidfromclid(&request, session),
            "clientlist" => self.handle_clientlist(&request, session),
            "clientinfo" => self.handle_clientinfo(&request, session),
            "clientupdate" => self.handle_clientupdate(&request, session),
            "serveredit" => self.handle_serveredit(&request, session),
            "clientaddperm" => self.handle_clientaddperm(&request, session),
            "clientdelperm" => self.handle_clientdelperm(&request, session),
            "clientpermlist" => self.handle_clientpermlist(&request, session),
            "clientmove" => self.handle_clientmove(&request, session),
            "ftinitupload" => self.handle_ftinitupload(&request, session),
            "ftinitdownload" => self.handle_ftinitdownload(&request, session),
            "ftgetfilelist" => self.handle_ftgetfilelist(&request, session),
            "ftcreatedir" => self.handle_ftcreatedir(&request, session),
            "ftdeletefile" => self.handle_ftdeletefile(&request, session),
            "ftrenamefile" => self.handle_ftrenamefile(&request, session),
            "ftgetfileinfo" => self.handle_ftgetfileinfo(&request, session),
            "permfind" => self.handle_permfind(&request, session),
            "permget" => self.handle_permget(&request, session),
            "permidgetbyname" => self.handle_permidgetbyname(&request, session),
            "permissionlist" => self.handle_permissionlist(session),
            "permoverview" => self.handle_permoverview(&request, session),
            "channelclientaddperm" => self.handle_channelclientaddperm(&request, session),
            "channelclientdelperm" => self.handle_channelclientdelperm(&request, session),
            "channelclientpermlist" => self.handle_channelclientpermlist(&request, session),
            "channeladdperm" => self.handle_channeladdperm(&request, session),
            "channeldelperm" => self.handle_channeldelperm(&request, session),
            "channelinfo" => self.handle_channelinfo(&request, session),
            "channelpermlist" => self.handle_channelpermlist(&request, session),
            "channelcreate" => self.handle_channelcreate(&request, session),
            "channeldelete" => self.handle_channeldelete(&request, session),
            "channeledit" => self.handle_channeledit(&request, session),
            "channelmove" => self.handle_channelmove(&request, session),
            "channelgroupadd" => self.handle_channelgroupadd(&request, session),
            "channelgroupaddperm" => self.handle_channelgroupaddperm(&request, session),
            "channelgroupclientlist" => self.handle_channelgroupclientlist(&request, session),
            "channelgroupcopy" => self.handle_channelgroupcopy(&request, session),
            "channelgroupdel" => self.handle_channelgroupdel(&request, session),
            "channelgroupdelperm" => self.handle_channelgroupdelperm(&request, session),
            "channelgrouplist" => self.handle_channelgrouplist(session),
            "channelgrouppermlist" => self.handle_channelgrouppermlist(&request, session),
            "channelgrouprename" => self.handle_channelgrouprename(&request, session),
            "servergroupaddclient" => self.handle_servergroupaddclient(&request, session),
            "servergroupadd" => self.handle_servergroupadd(&request, session),
            "servergroupaddperm" => self.handle_servergroupaddperm(&request, session),
            "servergroupautoaddperm" => self.handle_servergroupautoaddperm(&request, session),
            "servergroupautodelperm" => self.handle_servergroupautodelperm(&request, session),
            "servergroupclientlist" => self.handle_servergroupclientlist(&request, session),
            "servergroupcopy" => self.handle_servergroupcopy(&request, session),
            "servergroupdel" => self.handle_servergroupdel(&request, session),
            "servergroupdelclient" => self.handle_servergroupdelclient(&request, session),
            "servergroupdelperm" => self.handle_servergroupdelperm(&request, session),
            "servergrouplist" => self.handle_servergrouplist(session),
            "servergrouppermlist" => self.handle_servergrouppermlist(&request, session),
            "servergrouprename" => self.handle_servergrouprename(&request, session),
            "servergroupsbyclientid" => self.handle_servergroupsbyclientid(&request, session),
            "privilegekeyadd" => self.handle_tokenadd(&request, session),
            "privilegekeydelete" => self.handle_tokendelete(&request, session),
            "tokenadd" => self.handle_tokenadd(&request, session),
            "tokendelete" => self.handle_tokendelete(&request, session),
            "tokenedit" => self.handle_tokenedit(&request, session),
            "tokenactionlist" => self.handle_tokenactionlist(&request, session),
            "tokenlist" => self.handle_tokenlist(&request, session),
            "tokenuse" => self.handle_tokenuse(&request, session),
            "privilegekeylist" => self.handle_privilegekeylist(&request, session),
            "privilegekeyuse" => self.handle_tokenuse(&request, session),
            "use" => self.handle_use(&request, session),
            "serverrequestconnectioninfo" => self.handle_serverrequestconnectioninfo(session),
            "serveridgetbyport" => self.handle_serveridgetbyport(&request, session),
            "hostinfo" => self.handle_hostinfo(session),
            "instanceinfo" => self.handle_instanceinfo(session),
            "listfeaturesupport" => self.handle_listfeaturesupport(),
            "bindinglist" => self.handle_bindinglist(&request, session),
            "propertylist" => self.handle_propertylist(&request),
            "serverlist" => self.handle_serverlist(&request, session),
            "version" => self.handle_version(),
            "whoami" => self.handle_whoami(session),
            "serverinfo" => self.handle_serverinfo(session),
            "channellist" => self.handle_channellist(session),
            "musicbotcreate" => self.handle_musicbotcreate(&request, session),
            "musicbotdelete" => self.handle_musicbotdelete(&request, session),
            "musicbotqueueadd" => self.handle_musicbotqueueadd(&request, session),
            "musicbotqueuelist" => self.handle_musicbotqueuelist(&request, session),
            "musicbotqueueremove" => self.handle_musicbotqueueremove(&request, session),
            "musicbotqueuereorder" => self.handle_musicbotqueuereorder(&request, session),
            "musicbotplayeraction" => self.handle_musicbotplayeraction(&request, session),
            "musicbotplayerinfo" => self.handle_musicbotplayerinfo(&request, session),
            "musicbotsetsubscription" => self.handle_musicbotsetsubscription(&request, session),
            "playlistaddperm" => self.handle_playlistaddperm(&request, session),
            "playlistclientaddperm" => self.handle_playlistclientaddperm(&request, session),
            "playlistclientlist" => self.handle_playlistclientlist(&request, session),
            "playlistclientpermlist" => self.handle_playlistclientpermlist(&request, session),
            "playlistedit" => self.handle_playlistedit(&request, session),
            "playlistlist" => self.handle_playlistlist(&request, session),
            "playlistinfo" => self.handle_playlistinfo(&request, session),
            "playlistpermlist" => self.handle_playlistpermlist(&request, session),
            "playlistsetsubscription" => self.handle_playlistsetsubscription(&request, session),
            "playlistsonglist" => self.handle_playlistsonglist(&request, session),
            "playlistsongadd" => self.handle_playlistsongadd(&request, session),
            "playlistsongremove" => self.handle_playlistsongremove(&request, session),
            "playlistsongreorder" => self.handle_playlistsongreorder(&request, session),
            "playlistsongsetcurrent" => self.handle_playlistsongsetcurrent(&request, session),
            "querycreate" => self.handle_querycreate(&request, session),
            "queryrename" => self.handle_queryrename(&request, session),
            "querychangepassword" => self.handle_querychangepassword(&request, session),
            "querydelete" => self.handle_querydelete(&request, session),
            "setclientchannelgroup" => self.handle_setclientchannelgroup(&request, session),
            "clientsetserverquerylogin" => self.handle_clientsetserverquerylogin(&request, session),
            other => QueryResponse::error(
                259,
                format!("command {} not implemented in baseline", other),
            ),
        }
    }

    fn load_persisted_state(&mut self) -> Result<()> {
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
                            name: channel.name,
                            topic: channel.topic,
                            description: channel.description,
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

    fn persist_state(&self) -> Result<()> {
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

    fn handle_help(&self, request: &CommandRequest) -> QueryResponse {
        if let Some(command_name) = request.positional_args.first() {
            if let Some(command) = self.specs.get_command(command_name) {
                let mut row = BTreeMap::new();
                row.insert(String::from("command"), command.name.clone());
                row.insert(String::from("category"), command.category.clone());
                row.insert(
                    String::from("description"),
                    normalize_text(&command.description),
                );
                row.insert(
                    String::from("implemented"),
                    self.is_command_implemented(&command.name).to_string(),
                );
                if !command.usage.is_empty() {
                    row.insert(
                        String::from("usage"),
                        normalize_text(&command.usage.join(" | ")),
                    );
                }
                if !command.permissions.is_empty() {
                    row.insert(String::from("permissions"), command.permissions.join(","));
                }
                return QueryResponse::ok_row(row);
            }
            return QueryResponse::error(768, format!("command {} not found", command_name));
        }

        let rows = self
            .specs
            .baseline_profile
            .essential_commands
            .iter()
            .map(|command| {
                let mut row = BTreeMap::new();
                row.insert(String::from("command"), command.name.clone());
                row.insert(String::from("category"), command.category.clone());
                row.insert(
                    String::from("implemented"),
                    self.is_command_implemented(&command.name).to_string(),
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_login(
        &self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let username = request
            .named_args
            .get("client_login_name")
            .cloned()
            .or_else(|| request.positional_args.first().cloned());
        let password = request
            .named_args
            .get("client_login_password")
            .cloned()
            .or_else(|| request.positional_args.get(1).cloned());

        let (username, password) = match (username, password) {
            (Some(username), Some(password)) => (username, password),
            _ => return QueryResponse::error(512, "missing login credentials"),
        };

        match self.store.query_accounts.get(&username) {
            Some(account) if account.password == password => {
                session.reset_client_state();
                session.authenticated_login = Some(account.login_name.clone());
                self.restore_session_from_snapshot(&account.login_name, account.server_id, session);
                QueryResponse::ok()
            }
            _ => QueryResponse::error(520, "authentication failed"),
        }
    }

    fn handle_logout(&self, session: &mut QuerySessionState) -> QueryResponse {
        session.reset_client_state();
        session.authenticated_login = None;
        session.selected_virtual_server_id = None;
        session.current_channel_id = None;
        session.virtual_mode = false;
        session.notification_subscriptions.clear();
        QueryResponse::ok()
    }

    fn handle_quit(&self) -> QueryResponse {
        QueryResponse::ok()
    }

    fn handle_servernotifyregister(
        &self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        }

        let Some(event_name) = request.named_args.get("event") else {
            return QueryResponse::error(512, "event is required");
        };
        let Some(event) = NotificationEventKind::parse(event_name) else {
            return QueryResponse::error(512, "unsupported notify event");
        };

        let channel_id = request
            .named_args
            .get("id")
            .and_then(|value| value.parse::<u32>().ok())
            .or_else(|| {
                if matches!(
                    event,
                    NotificationEventKind::Channel | NotificationEventKind::TextChannel
                ) {
                    session.current_channel_id
                } else {
                    None
                }
            });
        let subscription = NotificationSubscription {
            event,
            channel_id: if matches!(
                event,
                NotificationEventKind::Channel | NotificationEventKind::TextChannel
            ) {
                channel_id
            } else {
                None
            },
        };

        if !session.notification_subscriptions.contains(&subscription) {
            session.notification_subscriptions.push(subscription);
        }

        QueryResponse::ok()
    }

    fn handle_servernotifyunregister(&self, session: &mut QuerySessionState) -> QueryResponse {
        session.notification_subscriptions.clear();
        QueryResponse::ok()
    }

    fn handle_sendtextmessage(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        match self.text_message_target(request, session) {
            Ok(target) => {
                let sender = self.query_session_participant(session);
                if target.target_mode == 1 {
                    let Some(target_client_id) = target.target_client_id else {
                        return QueryResponse::error(
                            512,
                            "target is required for private text messages",
                        );
                    };
                    let Some((target_unique_id, target_database_id, target_name)) =
                        self.online_client_identity(target.server_id, target_client_id)
                    else {
                        return QueryResponse::error(768, "target client not found");
                    };
                    self.record_private_message(
                        target.server_id,
                        sender,
                        ConversationParticipant {
                            database_id: target_database_id,
                            unique_identifier: target_unique_id,
                            nickname: target_name,
                        },
                        target.message.clone(),
                    );
                } else {
                    self.record_text_message(
                        &target,
                        sender.database_id,
                        sender.unique_identifier,
                        sender.nickname,
                    );
                }
                QueryResponse::ok()
            }
            Err((error_id, message)) => QueryResponse::error(error_id, message),
        }
    }

    fn handle_clientpoke(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        if target_client_id == session.client_id {
            return QueryResponse::error(512, "cannot poke yourself");
        }
        if self.online_client_identity(server_id, target_client_id).is_none() {
            return QueryResponse::error(768, "target client not found");
        }

        QueryResponse::ok()
    }

    fn handle_clientkick(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(reason_id) = request
            .named_args
            .get("reasonid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "reasonid is required");
        };
        let reason_message = request.named_args.get("reasonmsg").cloned().unwrap_or_default();
        if reason_message.len() > 40 {
            return QueryResponse::error(512, "reasonmsg is too long");
        }

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let (target_snapshot, target_permissions) =
            match self.target_client_snapshot_and_permissions(server_id, target_client_id) {
                Ok(target) => target,
                Err(response) => return response,
            };

        match reason_id {
            4 => {
                if let Some(response) = self.check_target_client_power(
                    &actor_permissions,
                    &target_permissions,
                    &["i_client_kick_from_channel_power"],
                    &["i_client_needed_kick_from_channel_power"],
                    "i_client_kick_from_channel_power",
                ) {
                    return response;
                }

                let Some(default_channel_id) = self.default_channel_id_for_server(server_id) else {
                    return QueryResponse::error(768, "target channel not found");
                };
                let Some(target_client) = self.store.online_clients.get_mut(&target_client_id) else {
                    return QueryResponse::error(768, "target client not found");
                };
                target_client.channel_id = default_channel_id;
                QueryResponse::ok()
            }
            5 => {
                if let Some(response) = self.check_target_client_power(
                    &actor_permissions,
                    &target_permissions,
                    &["i_client_kick_from_server_power"],
                    &["i_client_needed_kick_from_server_power"],
                    "i_client_kick_from_server_power",
                ) {
                    return response;
                }

                self.remove_session_client(target_snapshot.id, 5, reason_message.clone());
                QueryResponse::ok()
            }
            _ => QueryResponse::error(512, "reasonid must be 4 or 5"),
        }
    }

    fn handle_banclient(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let requested_ban_time = request
            .named_args
            .get("time")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let ban_reason = request.named_args.get("banreason").cloned().unwrap_or_default();
        if ban_reason.len() > 40 {
            return QueryResponse::error(512, "banreason is too long");
        }

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let (target_snapshot, target_permissions) =
            match self.target_client_snapshot_and_permissions(server_id, target_client_id) {
                Ok(target) => target,
                Err(response) => return response,
            };

        if let Some(response) = self.check_target_client_power(
            &actor_permissions,
            &target_permissions,
            &["i_client_ban_power"],
            &["i_client_needed_ban_power"],
            "i_client_ban_power",
        ) {
            return response;
        }

        let max_ban_time =
            permission_value_or_default(&actor_permissions, &["i_client_ban_max_bantime"]);
        if max_ban_time > 0 && i64::from(requested_ban_time) > max_ban_time {
            return self.insufficient_permission_response("i_client_ban_max_bantime");
        }

        let ban_id = self.register_active_ban(&target_snapshot, requested_ban_time, ban_reason.clone());
        self.remove_session_client(target_snapshot.id, 6, ban_reason);

        let mut row = BTreeMap::new();
        row.insert(String::from("banid"), ban_id.to_string());
        QueryResponse::ok_row(row)
    }

    fn handle_querylist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let current_login = session.authenticated_login.as_deref();
        let current_actor_database_id = session.actor_client_database_id_override;

        let requested_server_id = match request.named_args.get("server_id") {
            Some(value) => match value.parse::<i32>() {
                Ok(server_id) => server_id,
                Err(_) => return QueryResponse::error(512, "server_id must be an integer"),
            },
            None => -1,
        };
        let list_all_servers = requested_server_id < 0;

        let rows = self
            .store
            .query_accounts
            .values()
            .filter(|account| {
                list_all_servers || account.server_id == Some(requested_server_id as u32)
            })
            .map(|account| {
                let mut row = BTreeMap::new();
                row.insert(
                    String::from("client_bounded_server"),
                    account.server_id.unwrap_or(0).to_string(),
                );
                row.insert(
                    String::from("client_login_name"),
                    account.login_name.clone(),
                );
                row.insert(
                    String::from("client_unique_identifier"),
                    self.query_account_unique_identifier(account),
                );
                row.insert(
                    String::from("flag_all"),
                    if list_all_servers {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row.insert(
                    String::from("flag_own"),
                    if current_login.is_some_and(|login| account.login_name == login)
                        || current_actor_database_id
                            .is_some_and(|database_id| {
                                account.client_database_id == Some(database_id)
                            })
                    {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row.insert(String::from("server_id"), requested_server_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_clientfind(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(pattern) = request.named_args.get("pattern") else {
            return QueryResponse::error(512, "pattern is required");
        };
        let pattern = pattern.to_ascii_lowercase();

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .filter(|client| client.nickname.to_ascii_lowercase().contains(&pattern))
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("client_nickname"), client.nickname.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_clientgetids(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| {
                client.server_id == server_id && client.unique_identifier == *client_uid
            })
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("cluid"), client.unique_identifier.clone());
                row.insert(String::from("name"), client.nickname.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_clientgetdbidfromuid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some((resolved_uid, client_database_id, _)) =
            self.lookup_client_identity_by_uid(server_id, client_uid)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        QueryResponse::ok_row(row)
    }

    fn handle_clientgetnamefromdbid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let Some((client_uid, resolved_database_id, name)) =
            self.lookup_client_identity_by_dbid(server_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), resolved_database_id.to_string());
        row.insert(String::from("cluid"), client_uid);
        row.insert(String::from("name"), name);
        QueryResponse::ok_row(row)
    }

    fn handle_clientgetnamefromuid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some((resolved_uid, client_database_id, name)) =
            self.lookup_client_identity_by_uid(server_id, client_uid)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        row.insert(String::from("name"), name);
        QueryResponse::ok_row(row)
    }

    fn handle_clientgetuidfromclid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(client) = self.online_client_by_id_in_server(server_id, client_id) else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("clid"), client.id.to_string());
        row.insert(String::from("cluid"), client.unique_identifier.clone());
        row.insert(String::from("nickname"), client.nickname.clone());
        QueryResponse::ok_row(row)
    }

    fn handle_clientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .map(|client| self.render_client_row(client, request, false))
            .collect::<Vec<_>>();
        QueryResponse::ok_rows(rows)
    }

    fn handle_clientinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        }

        let Some(client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(client) = self.store.online_clients.get(&client_id) else {
            return QueryResponse::error(768, "client not found");
        };

        QueryResponse::ok_row(self.render_client_row(client, request, true))
    }

    fn handle_clientupdate(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if request.named_args.is_empty() {
            return QueryResponse::error(512, "at least one client property is required");
        }

        let mut applied = false;

        if let Some(client_nickname) = request.named_args.get("client_nickname") {
            session.client_nickname = client_nickname.clone();
            applied = true;
        }

        if let Some(client_away) = request.named_args.get("client_away") {
            let Some(client_away) = parse_query_bool(client_away) else {
                return QueryResponse::error(512, "client_away must be 0 or 1");
            };
            session.client_away = client_away;
            applied = true;
        }

        if let Some(client_away_message) = request.named_args.get("client_away_message") {
            session.client_away_message = client_away_message.clone();
            applied = true;
        }

        if let Some(client_input_muted) = request.named_args.get("client_input_muted") {
            let Some(client_input_muted) = parse_query_bool(client_input_muted) else {
                return QueryResponse::error(512, "client_input_muted must be 0 or 1");
            };
            session.client_input_muted = client_input_muted;
            applied = true;
        }

        if let Some(client_output_muted) = request.named_args.get("client_output_muted") {
            let Some(client_output_muted) = parse_query_bool(client_output_muted) else {
                return QueryResponse::error(512, "client_output_muted must be 0 or 1");
            };
            session.client_output_muted = client_output_muted;
            applied = true;
        }

        let mut avatar_changed = false;
        if let Some(client_flag_avatar) = request.named_args.get("client_flag_avatar") {
            if let Some(db_id) = session.client_database_id {
                if let Some(client) = self.store.clients.get_mut(&db_id) {
                    client.client_flag_avatar = client_flag_avatar.clone();
                    let _ = self.db.update_client_avatar(&client.unique_identifier, client_flag_avatar);
                }
            }
            if let Some(snapshot) = self.store.online_clients.get_mut(&session.client_id) {
                // Wait, online_clients doesn't hold client_flag_avatar, but it's generated from Client struct.
            }
            avatar_changed = true;
            applied = true;
        }

        if !applied {
            return QueryResponse::error(512, "no supported client properties provided");
        }

        // Broadcast to others
        if let Some(online_client) = self.store.online_clients.get(&session.client_id) {
            let row = self.render_client_row(online_client, request, false);
            let mut update_row = BTreeMap::new();
            update_row.insert("clid".to_string(), session.client_id.to_string());
            if request.named_args.contains_key("client_nickname") {
                update_row.insert("client_nickname".to_string(), session.client_nickname.clone());
            }
            if request.named_args.contains_key("client_away") {
                update_row.insert("client_away".to_string(), if session.client_away { "1".to_string() } else { "0".to_string() });
            }
            if request.named_args.contains_key("client_away_message") {
                update_row.insert("client_away_message".to_string(), session.client_away_message.clone());
            }
            if request.named_args.contains_key("client_input_muted") {
                update_row.insert("client_input_muted".to_string(), if session.client_input_muted { "1".to_string() } else { "0".to_string() });
            }
            if request.named_args.contains_key("client_output_muted") {
                update_row.insert("client_output_muted".to_string(), if session.client_output_muted { "1".to_string() } else { "0".to_string() });
            }
            if let Some(avatar) = request.named_args.get("client_flag_avatar") {
                update_row.insert("client_flag_avatar".to_string(), avatar.clone());
            }
            
            let Some(server_id) = session.selected_virtual_server_id else {
                return QueryResponse::ok();
            };
            self.broadcast_event(
                server_id,
                Some(session.client_id),
                crate::transport::TransportNotification::ClientUpdated {
                    client_id: session.client_id,
                    invoker_id: session.client_id,
                    invoker_name: session.client_nickname.clone(),
                    invoker_uid: session.unique_identifier.clone(),
                    properties: update_row,
                },
            );
        }

        QueryResponse::ok()
    }

    fn handle_channelinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(channel) = self.channel_by_id(server_id, channel_id) else {
            return QueryResponse::error(768, "channel not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel.id.to_string());
        row.insert(String::from("pid"), channel.parent_id.to_string());
        row.insert(String::from("channel_order"), channel.order.to_string());
        row.insert(String::from("channel_name"), channel.name.clone());
        row.insert(String::from("channel_topic"), channel.topic.clone());
        row.insert(
            String::from("channel_description"),
            channel.description.clone(),
        );
        row.insert(String::from("channel_password"), channel.password.clone());
        row.insert(String::from("channel_codec"), channel.codec.to_string());
        row.insert(String::from("channel_codec_quality"), channel.codec_quality.to_string());
        row.insert(String::from("channel_maxclients"), channel.maxclients.to_string());
        row.insert(String::from("channel_maxfamilyclients"), channel.maxfamilyclients.to_string());
        
        apply_channel_kind_rows(&mut row, channel.kind);
        row.insert(
            String::from("channel_flag_default"),
            if self.default_channel_id_for_server(server_id) == Some(channel.id) {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(String::from("channel_flag_password"), if channel.flag_password { String::from("1") } else { String::from("0") });
        row.insert(String::from("channel_needed_talk_power"), String::from("0"));
        row.insert(
            String::from("total_clients"),
            self.client_count_in_channel(server_id, channel.id)
                .to_string(),
        );
        QueryResponse::ok_row(row)
    }

    fn handle_serverrequestconnectioninfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server) = self.selected_server(session) else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("virtualserver_id"), server.id.to_string());
        row.insert(String::from("virtualserver_port"), server.port.to_string());
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
        row.insert(String::from("connection_connected_time"), String::from("0"));
        row.insert(
            String::from("connection_packets_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_packets_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_sent"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_received"),
            String::from("0"),
        );
        QueryResponse::ok_row(row)
    }

    fn handle_serveridgetbyport(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(port) = request
            .named_args
            .get("virtualserver_port")
            .and_then(|value| value.parse::<u16>().ok())
        else {
            return QueryResponse::error(512, "virtualserver_port is required");
        };
        let Some(server) = self
            .store
            .virtual_servers
            .values()
            .find(|server| server.port == port)
        else {
            return QueryResponse::error(768, "virtual server not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("server_id"), server.id.to_string());
        QueryResponse::ok_row(row)
    }

    fn handle_hostinfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("instance_uptime"), String::from("0"));
        row.insert(
            String::from("host_timestamp_utc"),
            current_unix_timestamp().to_string(),
        );
        row.insert(
            String::from("virtualservers_running_total"),
            self.store.virtual_servers.len().to_string(),
        );
        row.insert(
            String::from("virtualservers_online_total"),
            self.store.virtual_servers.len().to_string(),
        );
        row.insert(
            String::from("connection_packets_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_packets_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_sent_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_bytes_received_total"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_sent"),
            String::from("0"),
        );
        row.insert(
            String::from("connection_filetransfer_bandwidth_received"),
            String::from("0"),
        );
        QueryResponse::ok_row(row)
    }

    fn handle_instanceinfo(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let default_server_port = self
            .store
            .virtual_servers
            .values()
            .min_by_key(|server| server.id)
            .map(|server| server.port)
            .unwrap_or(9987);

        let mut row = BTreeMap::new();
        row.insert(
            String::from("serverinstance_database_version"),
            String::from("11"),
        );
        row.insert(
            String::from("serverinstance_filetransfer_port"),
            String::from("30303"),
        );
        row.insert(
            String::from("serverinstance_template_guest_serverquery_group"),
            if self.store.server_groups.contains_key(&7) {
                String::from("7")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("serverinstance_template_admin_serverquery_group"),
            if self.store.server_groups.contains_key(&6) {
                String::from("6")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("serverinstance_template_serveradmin_group"),
            self.store.server_groups.values().find(|g| g.name == "Server Admin").map(|g| g.id).unwrap_or(0).to_string(),
        );
        row.insert(
            String::from("serverinstance_default_virtualserver_port"),
            default_server_port.to_string(),
        );
        row.insert(
            String::from("serverinstance_query_port"),
            String::from("10101"),
        );
        QueryResponse::ok_row(row)
    }

    fn handle_listfeaturesupport(&self) -> QueryResponse {
        QueryResponse::ok_rows(self.build_feature_rows())
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

    fn handle_bindinglist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let subsystem = request
            .named_args
            .get("subsystem")
            .map(String::as_str)
            .unwrap_or("voice");
        if !matches!(subsystem, "voice" | "query" | "filetransfer") {
            return QueryResponse::error(512, "unsupported subsystem");
        }

        let rows = ["0.0.0.0", "0::0"]
            .into_iter()
            .map(|ip| {
                let mut row = BTreeMap::new();
                row.insert(String::from("ip"), String::from(ip));
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_propertylist(&self, request: &CommandRequest) -> QueryResponse {
        let include_all = request.flags.is_empty() || request.flags.contains("all");
        let rows = property_catalog()
            .into_iter()
            .filter(|(_, _, property_type)| {
                include_all
                    || (request.flags.contains("server") && *property_type == "SERVER")
                    || (request.flags.contains("channel") && *property_type == "CHANNEL")
                    || (request.flags.contains("client") && *property_type == "CLIENT")
                    || (request.flags.contains("instance") && *property_type == "INSTANCE")
                    || (request.flags.contains("group") && *property_type == "GROUP")
                    || (request.flags.contains("connection") && *property_type == "CONNECTION")
            })
            .map(|(name, flags, property_type)| {
                let mut row = BTreeMap::new();
                row.insert(String::from("flags"), flags.to_string());
                row.insert(String::from("name"), String::from(name));
                row.insert(String::from("type"), String::from(property_type));
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_tokenadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(owner_login) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };
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
                "b_virtualserver_token_add",
                "b_virtualserver_token_limit",
                "b_virtualserver_token_edit_all",
            ],
            "b_virtualserver_token_add",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let max_uses = if let Some(value) = request.named_args.get("token_max_uses") {
            let Some(parsed) = value.parse::<u32>().ok() else {
                return QueryResponse::error(512, "token_max_uses must be an integer");
            };
            parsed
        } else {
            0
        };
        let expired_at = if let Some(value) = request.named_args.get("token_expired") {
            let Some(parsed) = value.parse::<u64>().ok() else {
                return QueryResponse::error(512, "token_expired must be an integer");
            };
            (parsed != 0).then_some(parsed)
        } else {
            None
        };

        let action_mutations = match self.parse_token_action_mutations(request) {
            Ok(mutations) => mutations,
            Err(response) => return response,
        };
        if action_mutations
            .iter()
            .any(|mutation| !matches!(mutation, ParsedTokenActionMutation::Add { .. }))
        {
            return QueryResponse::error(512, "tokenadd only supports new actions");
        }

        let token_id = self.next_token_id();
        let created_at = current_unix_timestamp();
        let token_value = format!("compat{:08x}{:016x}", token_id, created_at);
        let mut next_action_ids = (0..action_mutations.len())
            .map(|_| self.next_token_action_id())
            .collect::<Vec<_>>()
            .into_iter();
        let mut actions = Vec::new();
        let mut rows = Vec::new();

        for mutation in action_mutations {
            if let ParsedTokenActionMutation::Add {
                action_type,
                action_id1,
                action_id2,
                action_text,
            } = mutation
            {
                let action_id = next_action_ids.next().unwrap_or(0);
                actions.push(TokenAction {
                    id: action_id,
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                });

                let mut row = BTreeMap::new();
                row.insert(String::from("action_id"), action_id.to_string());
                if rows.is_empty() {
                    row.insert(String::from("token"), token_value.clone());
                    row.insert(String::from("token_id"), token_id.to_string());
                }
                rows.push(row);
            }
        }

        self.store.tokens.insert(
            token_id,
            PrivilegeToken {
                id: token_id,
                server_id,
                token: token_value.clone(),
                description: request
                    .named_args
                    .get("token_description")
                    .cloned()
                    .unwrap_or_default(),
                max_uses,
                uses: 0,
                created_at,
                owner_login: owner_login.clone(),
                expired_at,
                actions,
            },
        );

        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("token"), token_value);
            row.insert(String::from("token_id"), token_id.to_string());
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    fn handle_tokendelete(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
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
            &["b_virtualserver_token_delete_all"],
            "b_virtualserver_token_delete_all",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let token_id = match self.resolve_token_id(request, server_id) {
            Ok(token_id) => token_id,
            Err(response) => return response,
        };
        self.store.tokens.remove(&token_id);
        QueryResponse::ok_rows(Vec::new())
    }

    fn handle_tokenedit(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
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
            &["b_virtualserver_token_edit_all"],
            "b_virtualserver_token_edit_all",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let token_id = match self.resolve_token_id(request, server_id) {
            Ok(token_id) => token_id,
            Err(response) => return response,
        };
        let max_uses_update = if let Some(value) = request.named_args.get("token_max_uses") {
            let Some(parsed) = value.parse::<u32>().ok() else {
                return QueryResponse::error(512, "token_max_uses must be an integer");
            };
            Some(parsed)
        } else {
            None
        };
        let expired_at_update = if let Some(value) = request.named_args.get("token_expired") {
            let Some(parsed) = value.parse::<u64>().ok() else {
                return QueryResponse::error(512, "token_expired must be an integer");
            };
            Some((parsed != 0).then_some(parsed))
        } else {
            None
        };
        let action_mutations = match self.parse_token_action_mutations(request) {
            Ok(mutations) => mutations,
            Err(response) => return response,
        };
        let mut next_action_ids = (0..action_mutations
            .iter()
            .filter(|mutation| matches!(mutation, ParsedTokenActionMutation::Add { .. }))
            .count())
            .map(|_| self.next_token_action_id())
            .collect::<Vec<_>>()
            .into_iter();

        let Some(token) = self.store.tokens.get_mut(&token_id) else {
            return QueryResponse::error(768, "token not found");
        };

        if let Some(description) = request.named_args.get("token_description") {
            token.description = description.clone();
        }
        if let Some(max_uses) = max_uses_update {
            token.max_uses = max_uses;
        }
        if let Some(expired_at) = expired_at_update {
            token.expired_at = expired_at;
        }

        let mut rows = Vec::new();
        for mutation in action_mutations {
            match mutation {
                ParsedTokenActionMutation::Add {
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                } => {
                    let action_id = next_action_ids.next().unwrap_or(0);
                    token.actions.push(TokenAction {
                        id: action_id,
                        action_type,
                        action_id1,
                        action_id2,
                        action_text,
                    });

                    let mut row = BTreeMap::new();
                    row.insert(String::from("action_id"), action_id.to_string());
                    rows.push(row);
                }
                ParsedTokenActionMutation::Update {
                    action_id,
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                } => {
                    let Some(action) = token
                        .actions
                        .iter_mut()
                        .find(|action| action.id == action_id)
                    else {
                        return QueryResponse::error(768, "token action not found");
                    };
                    action.action_type = action_type;
                    action.action_id1 = action_id1;
                    action.action_id2 = action_id2;
                    action.action_text = action_text;
                }
                ParsedTokenActionMutation::Remove { action_id } => {
                    let Some(position) = token
                        .actions
                        .iter()
                        .position(|action| action.id == action_id)
                    else {
                        return QueryResponse::error(768, "token action not found");
                    };
                    token.actions.remove(position);
                }
            }
        }

        QueryResponse::ok_rows(rows)
    }

    fn handle_tokenactionlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let token_id = match self.resolve_token_id(request, server_id) {
            Ok(token_id) => token_id,
            Err(response) => return response,
        };
        let Some(token) = self.store.tokens.get(&token_id) else {
            return QueryResponse::error(768, "token not found");
        };
        if token.owner_login != *login_name
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_token_list_all"],
                "b_virtualserver_token_list_all",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let rows = token
            .actions
            .iter()
            .map(|action| {
                let mut row = BTreeMap::new();
                row.insert(String::from("action_id"), action.id.to_string());
                row.insert(String::from("action_type"), action.action_type.to_string());
                row.insert(String::from("action_id1"), action.action_id1.to_string());
                row.insert(String::from("action_id2"), action.action_id2.to_string());
                row.insert(String::from("action_text"), action.action_text.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_tokenlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let offset = if let Some(value) = request.named_args.get("offset") {
            let Some(offset) = value.parse::<usize>().ok() else {
                return QueryResponse::error(512, "offset must be an integer");
            };
            offset
        } else {
            0
        };
        let limit = if let Some(value) = request.named_args.get("limit") {
            let Some(limit) = value.parse::<usize>().ok() else {
                return QueryResponse::error(512, "limit must be an integer");
            };
            Some(limit)
        } else {
            None
        };
        let list_all_tokens = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_token_list_all"],
            "b_virtualserver_token_list_all",
        )
        .is_ok();
        let own_only = request.flags.contains("own-only") || !list_all_tokens;

        let tokens = self
            .store
            .tokens
            .values()
            .filter(|token| token.server_id == server_id)
            .filter(|token| !own_only || token.owner_login == *login_name)
            .collect::<Vec<_>>();
        let token_count = tokens.len();
        let rows = tokens
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .map(|token| {
                let mut row = BTreeMap::new();
                row.insert(String::from("token_count"), token_count.to_string());
                row.insert(String::from("token_created"), token.created_at.to_string());
                row.insert(String::from("token_description"), token.description.clone());
                row.insert(
                    String::from("token_expired"),
                    token.expired_at.unwrap_or(0).to_string(),
                );
                row.insert(String::from("token_id"), token.id.to_string());
                row.insert(String::from("token_max_uses"), token.max_uses.to_string());
                row.insert(String::from("token"), token.token.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_tokenuse(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(token_value) = request.named_args.get("token") else {
            return QueryResponse::error(512, "token is required");
        };

        let Some(token_id) = self.store.tokens.iter().find_map(|(token_id, token)| {
            (token.server_id == server_id && token.token == *token_value).then_some(*token_id)
        }) else {
            return QueryResponse::error(768, "token not found");
        };

        let current_timestamp = current_unix_timestamp();
        let (expired_at, max_uses, uses, actions) = match self.store.tokens.get(&token_id) {
            Some(token) => (
                token.expired_at,
                token.max_uses,
                token.uses,
                token.actions.clone(),
            ),
            None => return QueryResponse::error(768, "token not found"),
        };

        if expired_at.is_some_and(|timestamp| timestamp <= current_timestamp) {
            return QueryResponse::error(768, "token expired");
        }
        if max_uses != 0 && uses >= max_uses {
            return QueryResponse::error(768, "token exhausted");
        }

        let mut server_groups_to_add = Vec::new();
        for action in &actions {
            match action.action_type {
                2 => {
                    if !self.store.server_groups.contains_key(&action.action_id1) {
                        return QueryResponse::error(4864, "Invalid group id");
                    }
                    server_groups_to_add.push(action.action_id1);
                }
                1 => {}
                other => {
                    return QueryResponse::error(
                        512,
                        format!("token action type {} not supported in baseline", other),
                    );
                }
            }
        }

        if let Some(login_name) = session.authenticated_login.as_ref() {
            let Some(account) = self.store.query_accounts.get_mut(login_name) else {
                return QueryResponse::error(768, "query account not found");
            };
            for group_id in &server_groups_to_add {
                if !account.server_groups.contains(group_id) {
                    account.server_groups.push(*group_id);
                }
            }
            account.server_groups.sort_unstable();
            account.server_groups.dedup();
        } else if let Some(actor_client_database_id) = session.actor_client_database_id_override {
            let Some(client) = self.store.online_clients.values_mut().find(|client| {
                client.server_id == server_id && client.database_id == actor_client_database_id
            }) else {
                return QueryResponse::error(768, "client not found");
            };
            for group_id in &server_groups_to_add {
                if !client.server_groups.contains(group_id) {
                    client.server_groups.push(*group_id);
                }
            }
            client.server_groups.sort_unstable();
            client.server_groups.dedup();
        }

        let should_delete = match self.store.tokens.get_mut(&token_id) {
            Some(token) => {
                token.uses = token.uses.saturating_add(1);
                token.max_uses != 0 && token.uses >= token.max_uses
            }
            None => false,
        };
        if should_delete {
            self.store.tokens.remove(&token_id);
        }

        QueryResponse::ok()
    }

    fn handle_privilegekeylist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        self.handle_tokenlist(request, session)
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

    pub fn record_text_message(
        &mut self,
        target: &TextMessageTarget,
        sender_database_id: u64,
        sender_unique_id: String,
        sender_name: String,
    ) -> u64 {
        let timestamp = self.next_conversation_timestamp();
        let Some(conversation_id) = (match target.target_mode {
            2 => target.channel_id,
            3 => Some(0),
            _ => None,
        }) else {
            return timestamp;
        };

        self.store
            .conversation_messages
            .entry(target.server_id)
            .or_default()
            .push(ConversationMessage {
                conversation_id,
                timestamp,
                sender_database_id,
                sender_unique_id,
                sender_name,
                message: target.message.clone(),
            });
        timestamp
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

    fn record_private_message(
        &mut self,
        server_id: u32,
        sender: ConversationParticipant,
        target: ConversationParticipant,
        message: String,
    ) -> u64 {
        let timestamp = self.next_conversation_timestamp();
        self.store
            .private_messages
            .entry(server_id)
            .or_default()
            .push(PrivateConversationMessage {
                timestamp,
                sender_database_id: sender.database_id,
                sender_unique_id: sender.unique_identifier,
                sender_name: sender.nickname,
                target_database_id: target.database_id,
                target_unique_id: target.unique_identifier,
                target_name: target.nickname,
                message,
            });
        timestamp
    }

    fn handle_ftinitupload(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(id) => id,
            None => return QueryResponse::error(1024, "invalid serverID"),
        };
        
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };

        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        let size = row.get("size").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        
        let transfer_id = self.next_upload_id;
        self.next_upload_id += 1;
        
        let transfer_id_str = transfer_id.to_string();
        match registry.prepare_upload(cid, "/", name, size, false, false, None, Some(transfer_id_str.as_str()), None) {
            Ok(transfer) => {
                let mut resp = BTreeMap::new();
                resp.insert("clientftfid".to_string(), transfer_id.to_string());
                resp.insert("serverftfid".to_string(), transfer.server_transfer_id.to_string());
                resp.insert("ftkey".to_string(), transfer.transfer_key);
                resp.insert("port".to_string(), transfer.port.to_string());
                resp.insert("size".to_string(), transfer.size.to_string());
                resp.insert("proto".to_string(), "0".to_string());
                QueryResponse::ok_row(resp)
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_ftinitdownload(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(id) => id,
            None => return QueryResponse::error(1024, "invalid serverID"),
        };
        
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };

        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        
        let transfer_id = self.next_download_id;
        self.next_download_id += 1;
        
        let transfer_id_str = transfer_id.to_string();
        match registry.prepare_download(cid, "/", name, 0, false, None, Some(transfer_id_str.as_str()), None) {
            Ok(transfer) => {
                let mut resp = BTreeMap::new();
                resp.insert("clientftfid".to_string(), transfer_id.to_string());
                resp.insert("serverftfid".to_string(), transfer.server_transfer_id.to_string());
                resp.insert("ftkey".to_string(), transfer.transfer_key);
                resp.insert("port".to_string(), transfer.port.to_string());
                resp.insert("size".to_string(), transfer.size.to_string());
                resp.insert("proto".to_string(), "0".to_string());
                QueryResponse::ok_row(resp)
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_ftgetfilelist(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let path = row.get("path").map(|v| v.as_str()).unwrap_or("/");
        
        match registry.list_entries(cid, path) {
            Ok(entries) => {
                let mut out_rows = Vec::new();
                for entry in entries {
                    let mut r = BTreeMap::new();
                    r.insert("cid".to_string(), cid.to_string());
                    r.insert("path".to_string(), path.to_string());
                    r.insert("name".to_string(), entry.name);
                    r.insert("size".to_string(), entry.size.to_string());
                    r.insert("datetime".to_string(), entry.datetime.to_string());
                    r.insert("type".to_string(), entry.entry_type.to_string());
                    out_rows.push(r);
                }
                if out_rows.is_empty() {
                    QueryResponse::ok()
                } else {
                    QueryResponse::ok_rows(out_rows)
                }
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_ftcreatedir(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let dirname = row.get("dirname").map(|v| v.as_str()).unwrap_or("");
        
        match registry.create_directory(cid, dirname) {
            Ok(_) => QueryResponse::ok(),
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_ftdeletefile(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        
        match registry.delete_entry(cid, "/", name, None) {
            Ok(_) => QueryResponse::ok(),
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_ftrenamefile(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let tcid = row.get("tcid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let oldname = row.get("oldname").map(|v| v.as_str()).unwrap_or("");
        let newname = row.get("newname").map(|v| v.as_str()).unwrap_or("");
        
        match registry.rename_entry(cid, oldname, tcid, newname) {
            Ok(_) => QueryResponse::ok(),
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_ftgetfileinfo(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        
        match registry.stat_entry(cid, name, None) {
            Ok(entry) => {
                let mut r = BTreeMap::new();
                r.insert("cid".to_string(), cid.to_string());
                r.insert("name".to_string(), entry.name);
                r.insert("size".to_string(), entry.size.to_string());
                r.insert("datetime".to_string(), entry.datetime.to_string());
                QueryResponse::ok_row(r)
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    fn handle_clientmove(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };

        if let Some(requested_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            && requested_client_id != session.client_id
        {
            let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session)
            {
                Ok(actor) => actor,
                Err(response) => return response,
            };
            let (_target_snapshot, target_permissions) =
                match self.target_client_snapshot_and_permissions(server_id, requested_client_id) {
                    Ok(target) => target,
                    Err(response) => return response,
                };
            if let Some(response) = self.check_target_client_power(
                &actor_permissions,
                &target_permissions,
                &["i_client_move_power"],
                &["i_client_needed_move_power"],
                "i_client_move_power",
            ) {
                return response;
            }

            let Some(target_client) = self.store.online_clients.get_mut(&requested_client_id) else {
                return QueryResponse::error(768, "target client not found");
            };
            target_client.channel_id = target_channel_id;
            return QueryResponse::ok();
        }

        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        if !channels
            .iter()
            .any(|channel| channel.id == target_channel_id)
        {
            return QueryResponse::error(768, "target channel not found");
        }

        if let Some(target_client) = self.store.online_clients.get_mut(&session.client_id) {
            target_client.channel_id = target_channel_id;
        }
        session.current_channel_id = Some(target_channel_id);
        QueryResponse::ok()
    }

    fn handle_channelcreate(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_name) = request.named_args.get("channel_name").cloned() else {
            return QueryResponse::error(512, "channel_name is required");
        };

        let parent_id = request
            .named_args
            .get("cpid")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let requested_order = request
            .named_args
            .get("order")
            .and_then(|value| value.parse::<u32>().ok());
        let channel_kind = ChannelKind::from_request_flags(request);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let create_permission = match channel_kind {
            ChannelKind::Temporary => "b_channel_create_temporary",
            ChannelKind::SemiPermanent => "b_channel_create_semi_permanent",
            ChannelKind::Permanent => "b_channel_create_permanent",
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[create_permission],
            create_permission,
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        if parent_id != 0
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_child"],
                "b_channel_create_child",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_topic")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_with_topic"],
                "b_channel_create_with_topic",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_description")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_with_description"],
                "b_channel_create_with_description",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if requested_order.is_some()
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_with_sortorder"],
                "b_channel_create_with_sortorder",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        if parent_id != 0 && !channels.iter().any(|channel| channel.id == parent_id) {
            return QueryResponse::error(768, "parent channel not found");
        }

        let sibling_ids = ordered_sibling_ids(channels, parent_id, None);
        let insert_index = match resolve_insert_index(&sibling_ids, requested_order) {
            Some(insert_index) => insert_index,
            None => return QueryResponse::error(768, "sort order anchor not found"),
        };
        let channel_id = channels
            .iter()
            .map(|channel| channel.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);

        channels.push(Channel {
            id: channel_id,
            parent_id,
            order: 0,
            kind: channel_kind,
            name: channel_name,
            topic: request
                .named_args
                .get("channel_topic")
                .cloned()
                .unwrap_or_default(),
            description: request
                .named_args
                .get("channel_description")
                .cloned()
                .unwrap_or_default(),
            permissions: BTreeMap::new(),
        });

        let mut ordered_ids = sibling_ids;
        ordered_ids.insert(insert_index, channel_id);
        relink_sibling_orders(channels, parent_id, &ordered_ids);

        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel_id.to_string());
        QueryResponse::ok_row(row)
    }

    fn handle_channeldelete(
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
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(force) = request
            .named_args
            .get("force")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "force is required");
        };

        let channel_client_count = self.client_count_in_channel(server_id, channel_id);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| channel.permissions.clone())
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_channel_delete_permanent"],
            "b_channel_delete_permanent",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        if force == 1
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_delete_flag_force"],
                "b_channel_delete_flag_force",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if let Err(permission_name) =
            check_channel_delete_power_allowed(&actor_permissions, &target_permissions)
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let Some(channel_index) = channels.iter().position(|channel| channel.id == channel_id)
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if channel_id == 1 {
            return QueryResponse::error(770, "default channel cannot be deleted in baseline");
        }
        if channels
            .iter()
            .any(|channel| channel.parent_id == channel_id)
        {
            return QueryResponse::error(770, "channel has child channels");
        }
        if channel_client_count > 0 {
            return QueryResponse::error(
                770,
                if force == 1 {
                    "baseline cannot force-delete occupied channels yet"
                } else {
                    "channel is not empty"
                },
            );
        }

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

        if session.current_channel_id == Some(channel_id) {
            session.current_channel_id = self.default_channel_id_for_server(server_id);
        }

        QueryResponse::ok()
    }

    fn handle_channeledit(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| channel.permissions.clone())
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if let Err(permission_name) =
            check_channel_modify_power_allowed(&actor_permissions, &target_permissions)
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_name")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_modify_name"],
                "b_channel_modify_name",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_topic")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_modify_topic"],
                "b_channel_modify_topic",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_description")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_modify_description"],
                "b_channel_modify_description",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(channel) = self
            .store
            .channels
            .get_mut(&server_id)
            .and_then(|channels| channels.iter_mut().find(|channel| channel.id == channel_id))
        else {
            return QueryResponse::error(768, "channel not found");
        };

        if let Some(channel_name) = request.named_args.get("channel_name") {
            channel.name = channel_name.clone();
        }
        if let Some(channel_topic) = request.named_args.get("channel_topic") {
            channel.topic = channel_topic.clone();
        }
        if let Some(channel_description) = request.named_args.get("channel_description") {
            channel.description = channel_description.clone();
        }
        if let Some(channel_password) = request.named_args.get("channel_password") {
            channel.password = channel_password.clone();
            channel.flag_password = !channel.password.is_empty();
        }
        if let Some(codec) = request.named_args.get("channel_codec").and_then(|v| v.parse().ok()) {
            channel.codec = codec;
        }
        if let Some(quality) = request.named_args.get("channel_codec_quality").and_then(|v| v.parse().ok()) {
            channel.codec_quality = quality;
        }
        if let Some(maxclients) = request.named_args.get("channel_maxclients").and_then(|v| v.parse().ok()) {
            channel.maxclients = maxclients;
        }
        
        let mut is_semi = false;
        let mut is_perm = false;
        if request.named_args.get("channel_flag_semi_permanent").map(|v| v.as_str()) == Some("1") { is_semi = true; }
        if request.named_args.get("channel_flag_permanent").map(|v| v.as_str()) == Some("1") { is_perm = true; }
        if request.named_args.contains_key("channel_flag_semi_permanent") || request.named_args.contains_key("channel_flag_permanent") {
            channel.kind = ChannelKind::from_flags(is_perm, is_semi);
        }
        
        // Save to DB
        let _ = self.store.db.save_channel(server_id, channel);

        QueryResponse::ok()
    }

    fn handle_channelmove(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(parent_id) = request
            .named_args
            .get("cpid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cpid is required");
        };
        let requested_order = request
            .named_args
            .get("order")
            .and_then(|value| value.parse::<u32>().ok());

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let Some(channel_index) = channels.iter().position(|channel| channel.id == channel_id)
        else {
            return QueryResponse::error(768, "channel not found");
        };
        let Some(parent_id) = (if parent_id == 0 { Some(0) } else { channels.iter().find(|c| c.id == parent_id).map(|c| c.id) }) else {
            return QueryResponse::error(768, "parent channel not found");
        };
        if channel_id == parent_id || channel_is_descendant(channels, parent_id, channel_id) {
            return QueryResponse::error(770, "channel cannot be moved below itself");
        }

        let previous_parent_id = channels[channel_index].parent_id;
        let mut sibling_ids = ordered_sibling_ids(channels, parent_id, Some(channel_id));
        let insert_index = match resolve_insert_index(&sibling_ids, requested_order) {
            Some(insert_index) => insert_index,
            None => return QueryResponse::error(768, "sort order anchor not found"),
        };

        channels[channel_index].parent_id = parent_id;
        sibling_ids.insert(insert_index, channel_id);
        relink_sibling_orders(channels, parent_id, &sibling_ids);

        if previous_parent_id != parent_id {
            let previous_sibling_ids =
                ordered_sibling_ids(channels, previous_parent_id, Some(channel_id));
            relink_sibling_orders(channels, previous_parent_id, &previous_sibling_ids);
        }

        QueryResponse::ok()
    }

    fn handle_use(
        &self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let sid = request
            .named_args
            .get("sid")
            .and_then(|value| value.parse::<u32>().ok())
            .or_else(|| {
                request
                    .positional_args
                    .first()
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .or_else(|| {
                request
                    .named_args
                    .get("port")
                    .and_then(|value| value.parse::<u16>().ok())
                    .and_then(|port| {
                        self.store
                            .virtual_servers
                            .values()
                            .find(|server| server.port == port)
                            .map(|server| server.id)
                    })
            });

        match sid.and_then(|server_id| {
            self.store
                .virtual_servers
                .get(&server_id)
                .map(|_| server_id)
        }) {
            Some(server_id) => {
                session.selected_virtual_server_id = Some(server_id);
                session.current_channel_id = self.default_channel_id_for_server(server_id);
                session.virtual_mode = request.flags.contains("virtual");
                QueryResponse::ok()
            }
            None => QueryResponse::error(768, "virtual server not found"),
        }
    }

    fn handle_serverlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let rows = self
            .store
            .virtual_servers
            .values()
            .map(|server| {
                let mut row = BTreeMap::new();
                row.insert(String::from("virtualserver_id"), server.id.to_string());
                row.insert(String::from("virtualserver_port"), server.port.to_string());
                row.insert(String::from("virtualserver_status"), String::from("online"));
                row.insert(
                    String::from("virtualserver_clientsonline"),
                    self.client_count_in_server(server.id).to_string(),
                );
                row.insert(String::from("virtualserver_name"), server.name.clone());
                if request.flags.contains("uid") || !request.flags.contains("short") {
                    row.insert(
                        String::from("virtualserver_unique_identifier"),
                        server.unique_identifier.clone(),
                    );
                }
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    fn handle_version(&self) -> QueryResponse {
        let mut row = BTreeMap::new();
        row.insert(
            String::from("version"),
            self.specs.build_version.build_version.clone(),
        );
        row.insert(
            String::from("build"),
            self.specs.build_version.build_index.to_string(),
        );
        row.insert(String::from("platform"), String::from("compat-rust"));
        row.insert(
            String::from("build_name"),
            self.specs.build_version.build_name.clone(),
        );
        QueryResponse::ok_row(row)
    }

    fn handle_whoami(&self, session: &QuerySessionState) -> QueryResponse {
        let mut row = BTreeMap::new();
        let current_account = session
            .authenticated_login
            .as_ref()
            .and_then(|login| self.store.query_accounts.get(login));
        row.insert(
            String::from("client_login_name"),
            session
                .authenticated_login
                .clone()
                .unwrap_or_else(|| String::from("anonymous")),
        );
        row.insert(String::from("clid"), session.client_id.to_string());
        row.insert(
            String::from("virtualserver_id"),
            session
                .selected_virtual_server_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| String::from("0")),
        );
        if let Some(channel_id) = session.current_channel_id {
            row.insert(String::from("client_channel_id"), channel_id.to_string());
        }
        row.insert(
            String::from("virtual"),
            if session.virtual_mode {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("notify_subscription_count"),
            session.notification_subscriptions.len().to_string(),
        );
        if let Some(account) = current_account {
            row.insert(
                String::from("permission_count"),
                self.effective_permissions_for_account(account)
                    .len()
                    .to_string(),
            );
            if !account.server_groups.is_empty() {
                row.insert(
                    String::from("client_servergroups"),
                    account
                        .server_groups
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
        } else {
            row.insert(String::from("permission_count"), String::from("0"));
        }
        if let Some(client_database_id) =
            current_account.and_then(|account| account.client_database_id)
        {
            row.insert(
                String::from("client_database_id"),
                client_database_id.to_string(),
            );
        }
        QueryResponse::ok_row(row)
    }

    fn handle_serverinfo(&self, session: &QuerySessionState) -> QueryResponse {
        let server = match self.selected_server(session) {
            Some(server) => server,
            None => return QueryResponse::error(522, "virtual server selection required"),
        };

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
        QueryResponse::ok_row(row)
    }

    fn handle_serveredit(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if request.named_args.is_empty() {
            return QueryResponse::error(512, "at least one server property is required");
        }

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if request.named_args.contains_key("virtualserver_name")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_name"],
                "b_virtualserver_modify_name",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request
            .named_args
            .contains_key("virtualserver_welcomemessage")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_welcomemessage"],
                "b_virtualserver_modify_welcomemessage",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if (request.named_args.contains_key("virtualserver_hostmessage")
            || request
                .named_args
                .contains_key("virtualserver_hostmessage_mode"))
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_hostmessage"],
                "b_virtualserver_modify_hostmessage",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("virtualserver_maxclients")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_maxclients"],
                "b_virtualserver_modify_maxclients",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request
            .named_args
            .keys()
            .any(|key| key.starts_with("virtualserver_antiflood_"))
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_antiflood"],
                "b_virtualserver_modify_antiflood",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request
            .named_args
            .contains_key("virtualserver_ask_for_privilegekey")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_modify_name"],
                "b_virtualserver_modify_name",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(server) = self.selected_server_mut(session) else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let mut applied = false;

        if let Some(server_name) = request.named_args.get("virtualserver_name") {
            server.name = server_name.clone();
            applied = true;
        }
        if let Some(welcome_message) = request.named_args.get("virtualserver_welcomemessage") {
            server.welcome_message = welcome_message.clone();
            applied = true;
        }
        if let Some(host_message) = request.named_args.get("virtualserver_hostmessage") {
            server.host_message = host_message.clone();
            applied = true;
        }
        if let Some(host_message_mode) = request.named_args.get("virtualserver_hostmessage_mode") {
            let Ok(host_message_mode) = host_message_mode.parse::<u32>() else {
                return QueryResponse::error(512, "virtualserver_hostmessage_mode must be numeric");
            };
            server.host_message_mode = host_message_mode;
            applied = true;
        }
        if let Some(ask_for_privilegekey) =
            request.named_args.get("virtualserver_ask_for_privilegekey")
        {
            let Some(ask_for_privilegekey) = parse_query_bool(ask_for_privilegekey) else {
                return QueryResponse::error(
                    512,
                    "virtualserver_ask_for_privilegekey must be 0 or 1",
                );
            };
            server.ask_for_privilegekey = if ask_for_privilegekey { 1 } else { 0 };
            applied = true;
        }
        if let Some(max_clients) = request.named_args.get("virtualserver_maxclients") {
            let Ok(max_clients) = max_clients.parse::<u32>() else {
                return QueryResponse::error(512, "virtualserver_maxclients must be numeric");
            };
            server.max_clients = max_clients;
            applied = true;
        }
        if let Some(value) = request
            .named_args
            .get("virtualserver_antiflood_points_tick_reduce")
        {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_points_tick_reduce must be numeric",
                );
            };
            server.antiflood_points_tick_reduce = value;
            applied = true;
        }
        if let Some(value) = request
            .named_args
            .get("virtualserver_antiflood_points_needed_command_block")
        {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_points_needed_command_block must be numeric",
                );
            };
            server.antiflood_points_needed_command_block = value;
            applied = true;
        }
        if let Some(value) = request
            .named_args
            .get("virtualserver_antiflood_points_needed_ip_block")
        {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_points_needed_ip_block must be numeric",
                );
            };
            server.antiflood_points_needed_ip_block = value;
            applied = true;
        }
        if let Some(value) = request.named_args.get("virtualserver_antiflood_ban_time") {
            let Ok(value) = value.parse::<u32>() else {
                return QueryResponse::error(
                    512,
                    "virtualserver_antiflood_ban_time must be numeric",
                );
            };
            server.antiflood_ban_time = value;
            applied = true;
        }

        if !applied {
            return QueryResponse::error(512, "no supported server properties provided");
        }

        QueryResponse::ok()
    }

    fn handle_channellist(&self, session: &QuerySessionState) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(server_id) => server_id,
            None => return QueryResponse::error(522, "virtual server selection required"),
        };

        let rows = self
            .store
            .channels
            .get(&server_id)
            .map(|channels| {
                channels
                    .iter()
                    .map(|channel| {
                        let mut row = BTreeMap::new();
                        row.insert(String::from("cid"), channel.id.to_string());
                        row.insert(String::from("pid"), channel.parent_id.to_string());
                        row.insert(String::from("channel_order"), channel.order.to_string());
                        row.insert(String::from("channel_name"), channel.name.clone());
                        row.insert(String::from("channel_topic"), channel.topic.clone());
                        apply_channel_kind_rows(&mut row, channel.kind);
                        row.insert(
                            String::from("total_clients"),
                            self.client_count_in_channel(server_id, channel.id)
                                .to_string(),
                        );
                        row
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        QueryResponse::ok_rows(rows)
    }

    fn handle_querycreate(
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

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        if self.store.query_accounts.contains_key(&login_name) {
            return QueryResponse::error(769, "query account already exists");
        }

        let password = request
            .named_args
            .get("client_login_password")
            .cloned()
            .unwrap_or_else(|| format!("generated-{}", login_name));
        let server_id = request
            .named_args
            .get("server_id")
            .and_then(|value| value.parse::<u32>().ok());
        let client_database_id = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| self.allocate_client_database_id());
        let creating_own_identity = client_database_id == actor.client_database_id;
        let required_permission_name = if creating_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_create", "b_client_query_create_own"],
                "b_client_query_create_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_create"],
                "b_client_query_create",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        self.store.query_accounts.insert(
            login_name.clone(),
            QueryAccount {
                login_name: login_name.clone(),
                password: password.clone(),
                server_id,
                client_database_id: Some(client_database_id),
                server_groups: self.default_server_groups_for_new_query_account(),
                permissions: BTreeMap::new(),
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), login_name);
        row.insert(String::from("client_login_password"), password);
        row.insert(String::from("cldbid"), client_database_id.to_string());
        if let Some(server_id) = server_id {
            row.insert(String::from("client_bounded_server"), server_id.to_string());
        }
        QueryResponse::ok_row(row)
    }

    fn handle_queryrename(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };
        let Some(new_login_name) = request.named_args.get("client_new_login_name").cloned() else {
            return QueryResponse::error(512, "client_new_login_name is required");
        };

        if self.store.query_accounts.contains_key(&new_login_name) {
            return QueryResponse::error(769, "target query account already exists");
        }

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let renaming_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if renaming_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_rename", "b_client_query_rename_own"],
                "b_client_query_rename_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_rename"],
                "b_client_query_rename",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(mut account) = self.store.query_accounts.remove(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        account.login_name = new_login_name.clone();
        self.store
            .query_accounts
            .insert(new_login_name.clone(), account);
        if let Some(snapshot) = self.session_snapshots.remove(&login_name) {
            self.session_snapshots
                .insert(new_login_name.clone(), snapshot);
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), new_login_name.clone());
        if session.authenticated_login.as_deref() == Some(login_name.as_str()) {
            session.authenticated_login = Some(new_login_name.clone());
            row.insert(String::from("renamed_current_login"), String::from("1"));
        }
        QueryResponse::ok_row(row)
    }

    fn handle_querychangepassword(
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

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let changing_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if changing_own_identity {
            match check_required_permission(
                &actor_permissions,
                &[
                    "b_client_query_change_password",
                    "b_client_query_change_own_password",
                ],
                "b_client_query_change_own_password",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_change_password"],
                "b_client_query_change_password",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(account) = self.store.query_accounts.get_mut(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };

        let next_secret = request
            .named_args
            .get("client_login_password")
            .cloned()
            .unwrap_or_else(|| format!("generated-{}", login_name));
        account.password = next_secret.clone();

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), login_name);
        row.insert(String::from("client_login_password"), next_secret);
        QueryResponse::ok_row(row)
    }

    fn handle_querydelete(
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

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let deleting_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if deleting_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_delete", "b_client_query_delete_own"],
                "b_client_query_delete_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_delete"],
                "b_client_query_delete",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        self.session_snapshots.remove(&login_name);
        match self.store.query_accounts.remove(&login_name) {
            Some(account) => {
                if let Some(client_database_id) = account.client_database_id {
                    self.store
                        .channel_group_assignments
                        .retain(|assignment| assignment.client_database_id != client_database_id);
                    self.store
                        .channel_client_permissions
                        .retain(|target| target.client_database_id != client_database_id);
                }
                QueryResponse::ok()
            }
            None => QueryResponse::error(768, "query account not found"),
        }
    }

    fn handle_clientsetserverquerylogin(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let Some(current_login) = session.authenticated_login.clone() else {
            return QueryResponse::error(521, "login required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_create_modify_serverquery_login"],
            "b_client_create_modify_serverquery_login",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(new_login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        if self.store.query_accounts.contains_key(&new_login_name) {
            return QueryResponse::error(769, "target query account already exists");
        }

        let Some(mut account) = self.store.query_accounts.remove(&current_login) else {
            return QueryResponse::error(768, "current query account not found");
        };

        account.login_name = new_login_name.clone();
        account.password = format!("generated-{}", new_login_name);
        session.authenticated_login = Some(new_login_name.clone());
        self.store
            .query_accounts
            .insert(new_login_name.clone(), account.clone());
        if let Some(snapshot) = self.session_snapshots.remove(&current_login) {
            self.session_snapshots
                .insert(new_login_name.clone(), snapshot);
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), new_login_name);
        row.insert(String::from("client_login_password"), account.password);
        QueryResponse::ok_row(row)
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

    fn is_command_implemented(&self, name: &str) -> bool {
        matches!(
            name,
            "help"
                | "login"
                | "logout"
                | "quit"
                | "servernotifyregister"
                | "servernotifyunregister"
                | "sendtextmessage"
                | "querylist"
                | "clientfind"
                | "clientgetids"
                | "clientgetdbidfromuid"
                | "clientgetnamefromdbid"
                | "clientgetnamefromuid"
                | "clientgetuidfromclid"
                | "clientlist"
                | "clientinfo"
                | "clientpoke"
                | "clientkick"
                | "clientaddperm"
                | "clientdelperm"
                | "clientpermlist"
                | "banclient"
                | "clientmove"
                | "permoverview"
                | "channelclientaddperm"
                | "channelclientdelperm"
                | "channelclientpermlist"
                | "channeladdperm"
                | "channeldelperm"
                | "channelinfo"
                | "channelpermlist"
                | "permfind"
                | "permget"
                | "permidgetbyname"
                | "permissionlist"
                | "channelcreate"
                | "channeldelete"
                | "channeledit"
                | "channelmove"
                | "channelgroupadd"
                | "channelgroupaddperm"
                | "channelgroupclientlist"
                | "channelgroupcopy"
                | "channelgroupdel"
                | "channelgroupdelperm"
                | "channelgrouplist"
                | "channelgrouppermlist"
                | "channelgrouprename"
                | "servergroupadd"
                | "servergroupaddclient"
                | "servergroupaddperm"
                | "servergroupautoaddperm"
                | "servergroupautodelperm"
                | "servergroupdel"
                | "servergroupclientlist"
                | "servergroupcopy"
                | "servergroupdelclient"
                | "servergroupdelperm"
                | "servergrouplist"
                | "servergrouppermlist"
                | "servergrouprename"
                | "servergroupsbyclientid"
                | "privilegekeyadd"
                | "privilegekeydelete"
                | "tokenadd"
                | "tokendelete"
                | "tokenedit"
                | "tokenactionlist"
                | "tokenlist"
                | "tokenuse"
                | "privilegekeylist"
                | "privilegekeyuse"
                | "setclientchannelgroup"
                | "use"
                | "serverrequestconnectioninfo"
                | "serveridgetbyport"
                | "hostinfo"
                | "instanceinfo"
                | "listfeaturesupport"
                | "bindinglist"
                | "propertylist"
                | "serverlist"
                | "version"
                | "whoami"
                | "serverinfo"
                | "channellist"
                | "musicbotplayeraction"
                | "querycreate"
                | "queryrename"
                | "querychangepassword"
                | "querydelete"
                | "clientsetserverquerylogin"
        )
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

    fn sync_session_snapshot(
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

    fn restore_session_from_snapshot(
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

fn runtime_bool_flag(value: &str) -> bool {
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
