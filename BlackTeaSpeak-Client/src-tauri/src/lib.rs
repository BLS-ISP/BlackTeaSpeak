mod audio;
mod btea;
mod config;
mod commands;
use btea::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()

        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::connection::connect_to_server,
            commands::connection::disconnect,
            commands::connection::send_command,
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
            commands::files::download_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
