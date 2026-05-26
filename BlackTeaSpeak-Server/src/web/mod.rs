pub mod models;
pub mod server;
pub mod session;
pub mod broadcast;
pub mod visibility;
pub mod frames;

pub use models::*;
pub use server::*;
pub use session::*;
pub use broadcast::*;
pub use visibility::*;
pub use frames::*;

#[cfg(test)]
mod tests;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tungstenite::error::Error as WebSocketError;
use tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use tungstenite::{Message, accept};
use wtransport::{Endpoint, ServerConfig as WTransportServerConfig, Identity};
use std::path::PathBuf;
use crate::file_transfer::{
    FileEntryInfo, FileTransferError, FileTransferEvent, FileTransferRegistry,
    PreparedFileTransfer,
};
use crate::query::{CommandRequest, QueryResponse, encode_query_value};
use crate::models::{
    WhisperTargetSelection, WHISPER_TARGET_CHANNEL, WHISPER_TARGET_CLIENT,
    WHISPER_TARGET_SERVER_GROUP, WHISPER_TARGET_SELF,
};
use crate::runtime::{
    AntiFloodSessionState, BaselineRuntime, ChannelSnapshot, MusicBotNotifyPayload,
    OnlineClientSnapshot, QuerySessionState, ServerSnapshot,
    WebServerGroupMutationError, WebServerInitInfo, create_baseline_runtime,
    create_baseline_runtime_with_state_path, stable_web_client_database_id,
    stable_web_client_unique_identifier,
};
use crate::transport::{SessionPresence, TransportNotification};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
