use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub private_key: String, // base64 encoded
    pub public_key: String,  // base64 encoded
    pub uid: String,         // TS3 UID base64 encoded sha1 of public_key
    pub default_nickname: String,
    
    // Audio Settings
    pub audio_input_device: Option<String>,
    pub audio_output_device: Option<String>,
    pub input_amplification: Option<f32>,
    pub output_amplification: Option<f32>,
    pub voice_transmission_mode: Option<String>, // "voice_activation", "push_to_talk", "continuous"
    pub voice_activation_threshold: Option<f32>,
    pub ptt_hotkey: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Favorite {
    pub id: String,
    pub name: String,
    pub address: String,
    pub password: Option<String>,
    pub nickname: String,
    pub identity_id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub identities: Vec<Identity>,
    pub favorites: Vec<Favorite>,
}

pub fn get_config_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    let mut path = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }
    path.push("client_config.json");
    path
}

pub fn load_config_sync(app: &tauri::AppHandle) -> AppConfig {
    let path = get_config_path(app);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    AppConfig::default()
}

pub fn save_config_sync(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app);
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(path, content)
        .map_err(|e| format!("Failed to write config file: {}", e))?;
    Ok(())
}
