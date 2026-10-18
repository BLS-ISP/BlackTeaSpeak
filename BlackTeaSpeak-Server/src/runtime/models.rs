use std::collections::{BTreeMap, BTreeSet};
use crate::query::CommandRequest;
use crate::runtime::permissions::PermissionAssignment;
use crate::state::*;

use crate::runtime::runtime_bool_flag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationEventKind {
    Server,
    Channel,
    TextServer,
    TextChannel,
    TextPrivate,
}

impl NotificationEventKind {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "server" => Some(Self::Server),
            "channel" => Some(Self::Channel),
            "textserver" => Some(Self::TextServer),
            "textchannel" => Some(Self::TextChannel),
            "textprivate" => Some(Self::TextPrivate),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
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
    pub subscribed_channels: BTreeSet<u32>,
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
        self.subscribed_channels.clear();
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
    pub(crate) fn from_request_flags(request: &CommandRequest) -> Self {
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
        } else if request
            .named_args
            .get("channel_flag_permanent")
            .is_some_and(|value| !runtime_bool_flag(value))
        {
            Self::Temporary
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

impl From<PersistedChannelKind> for ChannelKind { fn from(value: PersistedChannelKind) -> Self {
        match value {
            PersistedChannelKind::Temporary => Self::Temporary,
            PersistedChannelKind::SemiPermanent => Self::SemiPermanent,
            PersistedChannelKind::Permanent => Self::Permanent,
        }
    }
}

impl From<ChannelKind> for PersistedChannelKind { fn from(value: ChannelKind) -> Self {
        match value {
            ChannelKind::Temporary => Self::Temporary,
            ChannelKind::SemiPermanent => Self::SemiPermanent,
            ChannelKind::Permanent => Self::Permanent,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QueryAccount {
    pub(crate) login_name: String,
    pub(crate) password: String,
    pub(crate) server_id: Option<u32>,
    pub(crate) client_database_id: Option<u64>,
    pub(crate) server_groups: Vec<u32>,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub struct ServerGroup {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) group_type: u32,
    pub(crate) icon_id: i64,
    pub(crate) save_db: bool,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub struct ChannelGroup {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) group_type: u32,
    pub(crate) icon_id: i64,
    pub(crate) save_db: bool,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelGroupAssignment {
    pub(crate) channel_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) channel_group_id: u32,
}

#[derive(Debug, Clone)]
pub struct ChannelClientPermissionTarget {
    pub(crate) channel_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub struct ClientPermissionTarget {
    pub(crate) server_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) client_unique_identifier: String,
    pub(crate) client_nickname: String,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: u32,
    pub parent_id: u32,
    pub order: u32,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: String,
    pub description: String,
    pub password: String,
    pub codec: u32,
    pub codec_quality: u32,
    pub maxclients: i32,
    pub maxfamilyclients: i32,
    pub flag_default: bool,
    pub flag_password: bool,
    pub permissions: BTreeMap<String, PermissionAssignment>,
}

impl Channel {
    pub fn needed_talk_power(&self) -> i64 {
        self.permission_power("i_channel_needed_talk_power")
    }

    pub fn set_needed_talk_power(&mut self, value: i64) {
        self.set_permission_power("i_channel_needed_talk_power", value);
    }

    pub fn permission_power(&self, perm_name: &str) -> i64 {
        self.permissions
            .get(perm_name)
            .map(|p| p.value)
            .unwrap_or(0)
    }

    pub fn set_permission_power(&mut self, perm_name: &str, value: i64) {
        if (value <= 0 && perm_name != "i_icon_id") || (value == 0 && perm_name == "i_icon_id") {
            self.permissions.remove(perm_name);
        } else {
            self.permissions.insert(
                String::from(perm_name),
                PermissionAssignment {
                    value,
                    negated: false,
                    skipped: false,
                },
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerTempPassword {
    pub server_id: u32,
    pub password: String,
    pub password_clear: String,
    pub description: String,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub target_channel_id: u32,
    pub target_channel_password: String,
    pub invoker_name: String,
    pub invoker_unique_id: String,
}

#[derive(Debug, Clone)]
pub struct VirtualServer {
    pub(crate) id: u32,
    pub(crate) port: u16,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) password: String,
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
    pub fn new_default(id: u32, port: u16) -> Self {
        Self {
            id,
            port,
            name: "BlackTeaSpeak Server".to_string(),
            status: "online".to_string(),
            password: String::new(),
            unique_identifier: crate::desktop_crypto::get_stable_server_uid(),
            welcome_message: "Welcome to BlackTeaSpeak!".to_string(),
            host_message: "".to_string(),
            host_message_mode: 0,
            ask_for_privilegekey: 0,
            max_clients: 32,
            antiflood_points_tick_reduce: 5,
            antiflood_points_needed_command_block: 150,
            antiflood_points_needed_ip_block: 250,
            antiflood_ban_time: 20,
        }
    }
    
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn port(&self) -> u16 {
        self.port
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConversationMessage {
    pub(crate) conversation_id: u32,
    pub(crate) timestamp: u64,
    pub(crate) sender_database_id: u64,
    pub(crate) sender_unique_id: String,
    pub(crate) sender_name: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConversationParticipant {
    pub(crate) database_id: u64,
    pub(crate) unique_identifier: String,
    pub(crate) nickname: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PrivateConversationMessage {
    pub(crate) timestamp: u64,
    pub(crate) sender_database_id: u64,
    pub(crate) sender_unique_id: String,
    pub(crate) sender_name: String,
    pub(crate) target_database_id: u64,
    pub(crate) target_unique_id: String,
    pub(crate) target_name: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineMessage {
    pub id: u64,
    pub server_id: u32,
    pub sender_database_id: u64,
    pub sender_unique_identifier: String,
    pub sender_nickname: String,
    pub recipient_database_id: u64,
    pub recipient_unique_identifier: String,
    pub subject: String,
    pub message: String,
    pub timestamp: u64,
    pub flag_read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientComplaint {
    pub server_id: u32,
    pub target_database_id: u64,
    pub target_name: String,
    pub from_database_id: u64,
    pub from_name: String,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLogEntry {
    pub id: u64,
    pub server_id: u32,
    pub timestamp: u64,
    pub level: u32,
    pub channel: String,
    pub message: String,
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
pub(crate) enum ParsedTokenActionMutation {
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
pub struct PrivilegeToken {
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
pub struct BanTrigger {
    pub(crate) client_unique_identifier: String,
    pub(crate) client_nickname: String,
    pub(crate) client_hardware_identifier: String,
    pub(crate) connection_client_ip: String,
    pub(crate) timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct ActiveBan {
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
pub struct OnlineClient {
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
    pub version: String,
    pub platform: String,
    pub country: String,
    pub connection_ip: String,
    pub server_groups: Vec<u32>,
    pub connected_at: u64,
    pub last_seen_at: u64,
    pub whisper_targets: Option<crate::models::WhisperTargetSelection>,
    pub extra_properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Client {
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
    pub extra_properties: BTreeMap<String, String>,
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
    pub codec: u32,
    pub codec_quality: u32,
    pub maxclients: i32,
    pub maxfamilyclients: i32,
    pub flag_default: bool,
    pub flag_password: bool,
    pub needed_talk_power: i64,
    pub needed_join_power: i64,
    pub needed_subscribe_power: i64,
    pub needed_description_view_power: i64,
    pub needed_modify_power: i64,
    pub needed_delete_power: i64,
    pub icon_id: i64,
    pub total_clients: u32,
}

impl ChannelSnapshot {
    pub fn from_channel(channel: &Channel, total_clients: u32) -> Self {
        Self {
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
            flag_default: channel.flag_default,
            flag_password: channel.flag_password,
            needed_talk_power: channel.needed_talk_power(),
            needed_join_power: channel.permission_power("i_channel_needed_join_power"),
            needed_subscribe_power: channel.permission_power("i_channel_needed_subscribe_power"),
            needed_description_view_power: channel
                .permission_power("i_channel_needed_description_view_power"),
            needed_modify_power: channel.permission_power("i_channel_needed_modify_power"),
            needed_delete_power: channel.permission_power("i_channel_needed_delete_power"),
            icon_id: channel.permission_power("i_icon_id"),
            total_clients,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveMediaBroadcast {
    pub server_id: u32,
    pub channel_id: u32,
    pub broadcaster_client_id: u64,
    /// `0` = Camera Video, `1` = Screen Share, `2` = Audio Broadcast
    pub broadcast_type: u32,
    pub ssrc: u32,
    pub joined_client_ids: BTreeSet<u64>,
}


