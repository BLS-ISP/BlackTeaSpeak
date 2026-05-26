use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::{Context, Result};
use crate::query::*;
use crate::runtime::*;
use super::*;
pub const DEFAULT_QUERY_BIND: &str = "127.0.0.1:10101";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPresence {
    pub client_id: u64,
    pub login_name: String,
    pub unique_identifier: String,
    pub client_type: u32,
    pub server_id: u32,
    pub channel_id: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportNotification {
    ClientEnterView {
        presence: SessionPresence,
        from_channel_id: Option<u32>,
        reason_id: u32,
    },
    ClientUpdated {
        server_id: u32,
        before: OnlineClientSnapshot,
        after: OnlineClientSnapshot,
    },
    ClientPoke {
        server_id: u32,
        target_client_id: u64,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
        message: String,
    },
    ClientMoved {
        presence: SessionPresence,
        from_channel_id: u32,
        reason_id: u32,
        reason_message: String,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
    },
    ClientLeftView {
        presence: SessionPresence,
        to_channel_id: Option<u32>,
        reason_id: u32,
        reason_message: String,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
        ban_time: Option<u32>,
    },
    ChannelEdited {
        server_id: u32,
        channel: ChannelSnapshot,
        description_changed: bool,
        invoker_id: u64,
        invoker_name: String,
    },
    ChannelCreated {
        server_id: u32,
        channel: ChannelSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    ChannelDeleted {
        server_id: u32,
        channel: ChannelSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    ChannelMoved {
        server_id: u32,
        previous_parent_id: u32,
        channel: ChannelSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    ServerEdited {
        server_id: u32,
        before: ServerSnapshot,
        after: ServerSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    TalkStatus {
        server_id: u32,
        channel_id: u32,
        client_id: u64,
        is_talking: bool,
        is_whisper: bool,
        whisper_targets: Option<crate::models::WhisperTargetSelection>,
    },
    TextMessage {
        target: TextMessageTarget,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
    },
}
