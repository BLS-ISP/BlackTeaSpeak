use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const RUNTIME_STATE_SCHEMA_VERSION: u32 = 13;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PersistedChannelKind {
    Temporary,
    SemiPermanent,
    #[default]
    Permanent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedPermissionAssignment {
    pub value: i64,
    #[serde(default)]
    pub negated: bool,
    #[serde(default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedQueryAccount {
    pub login_name: String,
    pub password: String,
    pub server_id: Option<u32>,
    pub client_database_id: Option<u64>,
    #[serde(default)]
    pub server_groups: Vec<u32>,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedServerGroup {
    pub id: u32,
    pub name: String,
    pub group_type: u32,
    pub icon_id: i64,
    #[serde(default = "default_savedb")]
    pub save_db: bool,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedChannelGroup {
    pub id: u32,
    pub name: String,
    pub group_type: u32,
    pub icon_id: i64,
    #[serde(default = "default_savedb")]
    pub save_db: bool,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedChannelGroupAssignment {
    pub channel_id: u32,
    pub client_database_id: u64,
    pub channel_group_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedChannelClientPermissionTarget {
    pub channel_id: u32,
    pub client_database_id: u64,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedClientPermissionTarget {
    pub client_database_id: u64,
    #[serde(default)]
    pub client_unique_identifier: String,
    #[serde(default)]
    pub client_nickname: String,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedChannel {
    pub id: u32,
    pub parent_id: u32,
    pub order: u32,
    #[serde(default)]
    pub kind: PersistedChannelKind,
    pub name: String,
    pub topic: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedVirtualServer {
    pub id: u32,
    pub port: u16,
    pub name: String,
    pub unique_identifier: String,
    #[serde(default)]
    pub welcome_message: String,
    #[serde(default)]
    pub host_message: String,
    #[serde(default)]
    pub host_message_mode: u32,
    #[serde(default)]
    pub ask_for_privilegekey: u32,
    #[serde(default = "default_max_clients")]
    pub max_clients: u32,
    #[serde(default = "default_antiflood_points_tick_reduce")]
    pub antiflood_points_tick_reduce: u32,
    #[serde(default = "default_antiflood_points_needed_command_block")]
    pub antiflood_points_needed_command_block: u32,
    #[serde(default = "default_antiflood_points_needed_ip_block")]
    pub antiflood_points_needed_ip_block: u32,
    #[serde(default = "default_antiflood_ban_time")]
    pub antiflood_ban_time: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedConversationMessage {
    pub conversation_id: u32,
    pub timestamp: u64,
    #[serde(default)]
    pub sender_database_id: u64,
    #[serde(default)]
    pub sender_unique_id: String,
    #[serde(default)]
    pub sender_name: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedPrivateConversationMessage {
    pub timestamp: u64,
    #[serde(default)]
    pub sender_database_id: u64,
    #[serde(default)]
    pub sender_unique_id: String,
    #[serde(default)]
    pub sender_name: String,
    #[serde(default)]
    pub target_database_id: u64,
    #[serde(default)]
    pub target_unique_id: String,
    #[serde(default)]
    pub target_name: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PersistedMusicBotState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMusicQueueEntry {
    pub song_id: u32,
    #[serde(default)]
    pub song_previous_song_id: u32,
    #[serde(default)]
    pub song_url: String,
    #[serde(default)]
    pub song_url_loader: String,
    #[serde(default)]
    pub song_invoker: u64,
    #[serde(default)]
    pub song_loaded: bool,
    #[serde(default)]
    pub song_metadata: String,
    #[serde(default)]
    pub song_title: String,
    #[serde(default)]
    pub song_description: String,
    #[serde(default)]
    pub song_thumbnail: String,
    #[serde(default)]
    pub song_length: u32,
    #[serde(default)]
    pub song_seekable: bool,
    #[serde(default)]
    pub song_is_live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedPlaylistClientPermissionTarget {
    pub client_database_id: u64,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMusicBot {
    pub id: u32,
    pub server_id: u32,
    pub client_database_id: u64,
    pub linked_client_id: Option<u64>,
    #[serde(default)]
    pub playlist_id: u32,
    #[serde(default)]
    pub current_song_id: u32,
    #[serde(default = "default_next_song_id")]
    pub next_song_id: u32,
    #[serde(default)]
    pub state: PersistedMusicBotState,
    #[serde(default = "default_music_bot_volume")]
    pub player_volume: String,
    #[serde(default)]
    pub playlist_title: String,
    #[serde(default)]
    pub playlist_description: String,
    #[serde(default)]
    pub playlist_flag_delete_played: bool,
    #[serde(default)]
    pub playlist_flag_finished: bool,
    #[serde(default)]
    pub playlist_replay_mode: u32,
    #[serde(default)]
    pub playlist_max_songs: u32,
    #[serde(default)]
    pub permissions: BTreeMap<String, PersistedPermissionAssignment>,
    #[serde(default)]
    pub client_permissions: Vec<PersistedPlaylistClientPermissionTarget>,
    #[serde(default)]
    pub queue: Vec<PersistedMusicQueueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTokenAction {
    pub id: u32,
    pub action_type: u32,
    pub action_id1: u32,
    pub action_id2: u32,
    #[serde(default)]
    pub action_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedToken {
    pub id: u32,
    pub server_id: u32,
    pub token: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub max_uses: u32,
    #[serde(default)]
    pub uses: u32,
    pub created_at: u64,
    pub owner_login: String,
    pub expired_at: Option<u64>,
    #[serde(default)]
    pub actions: Vec<PersistedTokenAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedNotificationSubscription {
    pub event: String,
    pub channel_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedSessionSnapshot {
    pub selected_virtual_server_id: Option<u32>,
    pub current_channel_id: Option<u32>,
    pub virtual_mode: bool,
    #[serde(default)]
    pub notification_subscriptions: Vec<PersistedNotificationSubscription>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRuntimeState {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub query_accounts: BTreeMap<String, PersistedQueryAccount>,
    #[serde(default)]
    pub server_groups: BTreeMap<u32, PersistedServerGroup>,
    #[serde(default)]
    pub channel_groups: BTreeMap<u32, PersistedChannelGroup>,
    #[serde(default)]
    pub virtual_servers: BTreeMap<u32, PersistedVirtualServer>,
    #[serde(default)]
    pub channels: BTreeMap<u32, Vec<PersistedChannel>>,
    #[serde(default)]
    pub channel_group_assignments: Vec<PersistedChannelGroupAssignment>,
    #[serde(default)]
    pub channel_client_permissions: Vec<PersistedChannelClientPermissionTarget>,
    #[serde(default)]
    pub client_permissions: Vec<PersistedClientPermissionTarget>,
    #[serde(default)]
    pub conversation_messages: BTreeMap<u32, Vec<PersistedConversationMessage>>,
    #[serde(default)]
    pub private_messages: BTreeMap<u32, Vec<PersistedPrivateConversationMessage>>,
    #[serde(default)]
    pub music_bots: BTreeMap<u32, PersistedMusicBot>,
    #[serde(default)]
    pub tokens: BTreeMap<u32, PersistedToken>,
    #[serde(default)]
    pub session_snapshots: BTreeMap<String, PersistedSessionSnapshot>,
    #[serde(default = "default_next_client_database_id")]
    pub next_client_database_id: u64,
    #[serde(default = "default_next_conversation_timestamp")]
    pub next_conversation_timestamp: u64,
    #[serde(default = "default_next_token_id")]
    pub next_token_id: u32,
    #[serde(default = "default_next_token_action_id")]
    pub next_token_action_id: u32,
}

impl Default for PersistedRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            query_accounts: BTreeMap::new(),
            server_groups: BTreeMap::new(),
            channel_groups: BTreeMap::new(),
            virtual_servers: BTreeMap::new(),
            channels: BTreeMap::new(),
            channel_group_assignments: Vec::new(),
            channel_client_permissions: Vec::new(),
            client_permissions: Vec::new(),
            conversation_messages: BTreeMap::new(),
            private_messages: BTreeMap::new(),
            music_bots: BTreeMap::new(),
            tokens: BTreeMap::new(),
            session_snapshots: BTreeMap::new(),
            next_client_database_id: default_next_client_database_id(),
            next_conversation_timestamp: default_next_conversation_timestamp(),
            next_token_id: default_next_token_id(),
            next_token_action_id: default_next_token_action_id(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeStateStore {
    path: PathBuf,
}

impl RuntimeStateStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn default_path(workspace_root: impl AsRef<Path>) -> PathBuf {
        workspace_root
            .as_ref()
            .join("BlackTeaSpeak-Server")
            .join("data")
            .join("runtime-state.json")
    }

    pub fn load(&self) -> Result<Option<PersistedRuntimeState>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let mut content = fs::read(&self.path)
            .with_context(|| format!("failed to read runtime state {}", self.path.display()))?;
        if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            content.drain(..3);
        }

        serde_json::from_slice(&content)
            .with_context(|| format!("failed to parse runtime state {}", self.path.display()))
            .map(Some)
    }

    pub fn save(&self, state: &PersistedRuntimeState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create runtime state directory {}",
                    parent.display()
                )
            })?;
        }

        let content = serde_json::to_vec_pretty(state).with_context(|| {
            format!("failed to serialize runtime state {}", self.path.display())
        })?;
        fs::write(&self.path, content)
            .with_context(|| format!("failed to write runtime state {}", self.path.display()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn default_schema_version() -> u32 {
    RUNTIME_STATE_SCHEMA_VERSION
}

fn default_next_client_database_id() -> u64 {
    100
}

fn default_next_conversation_timestamp() -> u64 {
    1
}

fn default_next_token_id() -> u32 {
    1
}

fn default_next_token_action_id() -> u32 {
    1
}

fn default_next_song_id() -> u32 {
    1
}

fn default_music_bot_volume() -> String {
    String::from("1")
}

fn default_max_clients() -> u32 {
    128
}

fn default_antiflood_points_tick_reduce() -> u32 {
    10
}

fn default_antiflood_points_needed_command_block() -> u32 {
    150
}

fn default_antiflood_points_needed_ip_block() -> u32 {
    250
}

fn default_antiflood_ban_time() -> u32 {
    300
}

fn default_savedb() -> bool {
    true
}
