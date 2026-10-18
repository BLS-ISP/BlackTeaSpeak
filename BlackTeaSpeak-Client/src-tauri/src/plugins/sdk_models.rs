use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audio::spatial_3d::{ListenerPose, SpatialAudioEngine, SpatialAudioStateSnapshot, Vec3};

pub const DEFAULT_PLUGIN_SDK_BIND: &str = "127.0.0.1:25639";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSdkStatus {
    pub enabled: bool,
    pub bind_address: String,
    pub connected_plugins: usize,
    pub active_game_name: Option<String>,
    pub packets_processed: u64,
    pub last_update_ms: u64,
}

impl Default for PluginSdkStatus {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: String::from(DEFAULT_PLUGIN_SDK_BIND),
            connected_plugins: 0,
            active_game_name: None,
            packets_processed: 0,
            last_update_ms: 0,
        }
    }
}

#[derive(Clone)]
pub struct PluginSdkSharedState {
    pub status: Arc<Mutex<PluginSdkStatus>>,
    pub spatial_audio: Arc<Mutex<SpatialAudioEngine>>,
    pub client_volumes: Arc<Mutex<HashMap<u16, f32>>>,
}

impl Default for PluginSdkSharedState {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(PluginSdkStatus::default())),
            spatial_audio: Arc::new(Mutex::new(SpatialAudioEngine::default())),
            client_volumes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginBulkClientEntry {
    pub client_id: u16,
    pub position: Vec3,
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    pub channel_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PluginSdkRequest {
    RegisterPlugin {
        plugin_name: String,
        version: Option<String>,
    },
    SetListenerPose {
        position: Vec3,
        forward: Option<Vec3>,
        up: Option<Vec3>,
    },
    SetClientPosition {
        client_id: u16,
        position: Vec3,
        channel_name: Option<String>,
    },
    SetClientMute {
        client_id: u16,
        muted: bool,
    },
    BulkUpdate {
        game_name: Option<String>,
        listener: Option<ListenerPose>,
        clients: Vec<PluginBulkClientEntry>,
    },
    GetState,
    Reset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSdkResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PluginSdkStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spatial: Option<SpatialAudioStateSnapshot>,
}
