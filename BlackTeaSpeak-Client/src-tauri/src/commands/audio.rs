use crate::btea::AppState;
use tauri::State;

#[derive(serde::Serialize)]
pub struct AudioDevices {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct AudioSettings {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub input_amplification: f32,
    pub output_amplification: f32,
    pub transmission_mode: String,
    pub vad_threshold: f32,
    pub ptt_hotkey: Option<String>,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    pub echo_cancellation: bool,
}

#[derive(serde::Deserialize)]
pub struct LiveAudioSettings {
    pub input_amplification: f32,
    pub output_amplification: f32,
    pub transmission_mode: String,
    pub vad_threshold: f32,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    pub echo_cancellation: bool,
}

#[tauri::command]
pub fn get_audio_devices() -> Result<AudioDevices, String> {
    let (inputs, outputs) = crate::audio::AudioManager::list_devices()?;
    Ok(AudioDevices { inputs, outputs })
}

#[tauri::command]
pub async fn update_audio_settings(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    settings: AudioSettings,
) -> Result<(), String> {
    let mut am_lock = state.audio_manager.lock().await;
    if let Some(am) = am_lock.as_mut() {
        am.input_amp.store(crate::audio::manager::f32_to_bits(settings.input_amplification), std::sync::atomic::Ordering::Relaxed);
        am.output_amp.store(crate::audio::manager::f32_to_bits(settings.output_amplification), std::sync::atomic::Ordering::Relaxed);
        am.vad_threshold.store(crate::audio::manager::f32_to_bits(settings.vad_threshold), std::sync::atomic::Ordering::Relaxed);
        am.noise_suppression.store(settings.noise_suppression, std::sync::atomic::Ordering::Relaxed);
        am.auto_gain_control.store(settings.auto_gain_control, std::sync::atomic::Ordering::Relaxed);
        am.echo_cancellation.store(settings.echo_cancellation, std::sync::atomic::Ordering::Relaxed);
        
        if let Ok(mut mode) = am.transmission_mode.lock() {
            *mode = settings.transmission_mode;
        }

        let _ = am.set_input_device(settings.input_device, Some(app_handle.clone()));
        let _ = am.set_output_device(settings.output_device, Some(app_handle));
        
        Ok(())
    } else {
        Err("Not connected".into())
    }
}

#[tauri::command]
pub async fn update_live_audio_settings(
    state: tauri::State<'_, AppState>,
    settings: LiveAudioSettings,
) -> Result<(), String> {
    let mut am_lock = state.audio_manager.lock().await;
    if let Some(am) = am_lock.as_mut() {
        am.input_amp.store(crate::audio::manager::f32_to_bits(settings.input_amplification), std::sync::atomic::Ordering::Relaxed);
        am.output_amp.store(crate::audio::manager::f32_to_bits(settings.output_amplification), std::sync::atomic::Ordering::Relaxed);
        am.vad_threshold.store(crate::audio::manager::f32_to_bits(settings.vad_threshold), std::sync::atomic::Ordering::Relaxed);
        am.noise_suppression.store(settings.noise_suppression, std::sync::atomic::Ordering::Relaxed);
        am.auto_gain_control.store(settings.auto_gain_control, std::sync::atomic::Ordering::Relaxed);
        am.echo_cancellation.store(settings.echo_cancellation, std::sync::atomic::Ordering::Relaxed);
        
        if let Ok(mut mode) = am.transmission_mode.lock() {
            *mode = settings.transmission_mode;
        }
        
        Ok(())
    } else {
        Err("Not connected".into())
    }
}
