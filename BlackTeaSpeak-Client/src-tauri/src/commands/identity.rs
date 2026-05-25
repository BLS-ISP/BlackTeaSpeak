use crate::config::{AppConfig, Identity, load_config_sync, save_config_sync};
use rand::rngs::OsRng;
use ed25519_dalek::SigningKey;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use sha1::{Sha1, Digest};

#[tauri::command]
pub fn generate_identity(name: String) -> Result<Identity, String> {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let pub_key = signing_key.verifying_key();
    
    let priv_bytes = signing_key.to_bytes();
    let pub_bytes = pub_key.to_bytes();
    
    let mut hasher = Sha1::new();
    hasher.update(&pub_bytes);
    let hash = hasher.finalize();
    let uid = BASE64.encode(hash);
    
    let id = format!("id_{}", uuid::Uuid::new_v4().simple());
    
    Ok(Identity {
        id,
        name,
        private_key: BASE64.encode(priv_bytes),
        public_key: BASE64.encode(pub_bytes),
        uid,
        default_nickname: "BlackTeaUser".to_string(),
        audio_input_device: None,
        audio_output_device: None,
        input_amplification: Some(1.0),
        output_amplification: Some(1.0),
        voice_transmission_mode: Some("voice_activation".to_string()),
        voice_activation_threshold: Some(0.05),
        ptt_hotkey: None,
        whisper_hotkey: None,
        whisper_targets: None,
        noise_suppression: Some(true),
        auto_gain_control: Some(true),
        echo_cancellation: Some(true),
    })
}

#[tauri::command]
pub fn load_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    Ok(load_config_sync(&app))
}

#[tauri::command]
pub fn save_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    save_config_sync(&app, &config)
}
