use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use super::*;
pub const DEFAULT_FILE_TRANSFER_BIND: &str = "127.0.0.1:30303";
pub const FILE_TRANSFER_STATUS_COMPLETE: u32 = 0x811;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTransferDirection {
    Upload,
    Download,
}
#[derive(Debug, Clone)]
pub struct PreparedFileTransfer {
    pub transfer_key: String,
    pub server_transfer_id: u64,
    pub direction: FileTransferDirection,
    pub port: u16,
    pub ip: Option<String>,
    pub seek_position: u64,
    pub size: u64,
}
#[derive(Debug, Clone)]
pub struct FileEntryInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub datetime: u64,
    pub entry_type: u32,
    pub empty: bool,
}
#[derive(Debug, Clone)]
pub enum FileTransferEvent {
    Started {
        client_id: u64,
        client_transfer_id: String,
    },
    Progress {
        client_id: u64,
        client_transfer_id: String,
        file_bytes_transferred: u64,
        file_current_offset: u64,
        file_start_offset: u64,
        file_total_size: u64,
        network_bytes_received: u64,
        network_bytes_send: u64,
        network_current_speed: u64,
        network_average_speed: u64,
    },
    Status {
        client_id: u64,
        client_transfer_id: String,
        status: u32,
        message: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTransferError {
    NotFound,
    AlreadyExists,
    InvalidPath,
    InvalidPayload,
    Io,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileArea {
    Channel(u32),
    Icons,
    Avatars,
    Music,
}
#[derive(Debug, Clone)]
pub(crate) struct PendingTransfer {
    pub(crate) server_transfer_id: u64,
    pub(crate) direction: FileTransferDirection,
    pub(crate) file_path: PathBuf,
    pub(crate) seek_position: u64,
    pub(crate) size: u64,
    pub(crate) notify_client_events: bool,
    pub(crate) client_id: Option<u64>,
    pub(crate) client_transfer_id: Option<String>,
}
impl PendingTransfer {
    pub(crate) fn should_emit_client_events(&self) -> bool {
        self.notify_client_events
            || (self.direction == FileTransferDirection::Upload
                && self.client_id.is_some()
                && self.client_transfer_id.is_some())
    }
}
pub(crate) type FileTransferNotifier = Arc<dyn Fn(&FileTransferEvent) + Send + Sync + 'static>;
#[derive(Debug, Clone)]
pub(crate) struct FileTransferEndpoint {
    pub(crate) ip: Option<String>,
    pub(crate) port: u16,
}
#[derive(Debug, Clone)]
pub(crate) struct ParsedHttpRequest {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: Vec<u8>,
}
