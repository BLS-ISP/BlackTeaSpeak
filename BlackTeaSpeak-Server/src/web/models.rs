use super::*;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;



pub const DEFAULT_TEAWEB_BIND: &str = "127.0.0.1:9989";

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
