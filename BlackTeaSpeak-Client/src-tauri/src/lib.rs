pub mod audio;
pub mod btea;
pub mod commands;
pub mod config;
pub mod network;
pub mod plugins;

use btea::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();
    let sdk_state = app_state.plugin_sdk.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .manage(app_state)
        .setup(move |app| {
            plugins::spawn_plugin_sdk_server(sdk_state, Some(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connection::connect_to_server,
            commands::connection::disconnect,
            commands::connection::send_command,
            commands::connection::discover_server_cert_hash,
            commands::identity::generate_identity,
            commands::identity::load_config,
            commands::identity::save_config,
            commands::connection::toggle_microphone,
            commands::connection::toggle_speaker,
            commands::connection::set_ptt_state,
            commands::connection::set_client_volume,
            commands::connection::set_whisper_state,
            commands::audio::get_audio_devices,
            commands::audio::update_audio_settings,
            commands::audio::update_live_audio_settings,
            commands::files::upload_file,
            commands::files::download_file,
            commands::spatial_audio::set_client_3d_position,
            commands::spatial_audio::remove_client_3d_position,
            commands::spatial_audio::set_listener_3d_pose,
            commands::spatial_audio::set_spatial_audio_config,
            commands::spatial_audio::get_spatial_audio_state,
            commands::spatial_audio::reset_spatial_audio,
            commands::plugin_sdk::get_plugin_sdk_status,
            commands::plugin_sdk::set_plugin_sdk_enabled,
            commands::plugin_sdk::resolve_tsdns_address
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
