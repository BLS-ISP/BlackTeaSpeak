use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tungstenite::error::Error as WebSocketError;
use tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
use tungstenite::{accept, Message};
use wtransport::{Endpoint, Identity, ServerConfig as WTransportServerConfig};

use crate::file_transfer::{
    FileEntryInfo, FileTransferError, FileTransferEvent, FileTransferRegistry, PreparedFileTransfer,
};
use crate::models::{
    WhisperTargetSelection, WHISPER_TARGET_CHANNEL, WHISPER_TARGET_CLIENT, WHISPER_TARGET_SELF,
    WHISPER_TARGET_SERVER_GROUP,
};
use crate::query::{encode_query_value, CommandRequest, QueryResponse};
use crate::runtime::{
    create_baseline_runtime, create_baseline_runtime_with_state_path,
    stable_web_client_database_id, stable_web_client_unique_identifier, AntiFloodSessionState,
    BaselineRuntime, ChannelSnapshot, MusicBotNotifyPayload, OnlineClientSnapshot,
    QuerySessionState, ServerSnapshot, WebServerGroupMutationError, WebServerInitInfo,
};
use crate::transport::{SessionPresence, TransportNotification};

pub const DEFAULT_TEAWEB_BIND: &str = "127.0.0.1:9988";

pub(crate) const LOCALHOST_CERTIFICATE_NAMES: [&str; 2] = ["localhost", "127.0.0.1"];

pub(crate) const WEB_CLIENT_ID_BASE: u64 = 20_000;

pub(crate) const TEAWEB_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) const TEAWEB_TIMEOUT_CLOSE_REASON: &str = "connection-ping-timeout";

pub(crate) type SharedBlackTeaWebSessions = Arc<Mutex<HashMap<u64, RegisteredBlackTeaWebSession>>>;

pub(crate) type SharedPendingFrames = Arc<Mutex<Vec<String>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlackTeaWebDisconnectKind {
    LeftServer,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlackTeaWebPresence {
    pub(crate) client_id: u64,
    pub(crate) server_id: u32,
    pub(crate) channel_id: u32,
    pub(crate) client_state: CommandRow,
}

#[derive(Clone)]
pub(crate) struct RegisteredBlackTeaWebSession {
    pub(crate) presence: BlackTeaWebPresence,
    pub(crate) client_database_id: u64,
    pub(crate) visible_channel_ids: BTreeSet<u32>,
    pub(crate) pending_frames: SharedPendingFrames,
    pub wtransport_session: Option<wtransport::Connection>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PermissionRefreshScope {
    pub(crate) needed_permissions: bool,
    pub(crate) server_groups: bool,
    pub(crate) channel_groups: bool,
}

impl PermissionRefreshScope {
    pub(crate) fn is_empty(self) -> bool {
        !self.needed_permissions && !self.server_groups && !self.channel_groups
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.needed_permissions |= other.needed_permissions;
        self.server_groups |= other.server_groups;
        self.channel_groups |= other.channel_groups;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlackTeaWebPermissionRefresh {
    pub(crate) server_id: u32,
    pub(crate) scope: PermissionRefreshScope,
}

#[derive(Debug, Clone)]
pub(crate) enum BlackTeaWebFrameBroadcast {
    Server {
        server_id: u32,
        exclude_client_id: Option<u64>,
        frame: String,
    },
    Client {
        client_id: u64,
        frame: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoginPhase {
    AwaitHandshake,
    AwaitIdentityProof,
    AwaitClientInit,
    Connected,
}

#[derive(Debug, Clone)]
pub(crate) enum PresenceBroadcast {
    PeerEnter {
        server_id: u32,
        exclude_client_id: Option<u64>,
        presence: BlackTeaWebPresence,
        from_channel_id: Option<u32>,
        reason_id: u32,
    },
    PeerMove {
        server_id: u32,
        exclude_client_id: Option<u64>,
        presence: BlackTeaWebPresence,
        from_channel_id: u32,
        reason_id: u32,
        reason_message: String,
    },
    PeerUpdate {
        server_id: u32,
        exclude_client_id: Option<u64>,
        before: BlackTeaWebPresence,
        after: BlackTeaWebPresence,
    },
    PeerLeft {
        server_id: u32,
        exclude_client_id: Option<u64>,
        presence: BlackTeaWebPresence,
        to_channel_id: Option<u32>,
        reason_id: u32,
        reason_message: String,
    },
}
