use tauri::State;

use crate::audio::{SpatialAudioStateSnapshot, Vec3};
use crate::btea::AppState;

#[tauri::command]
pub async fn set_client_3d_position(
    client_id: u16,
    x: f32,
    y: f32,
    z: f32,
    state: State<'_, AppState>,
) -> Result<SpatialAudioStateSnapshot, String> {
    let mut engine = state
        .plugin_sdk
        .spatial_audio
        .lock()
        .map_err(|_| String::from("Spatial audio lock poisoned"))?;
    engine.set_client_position(client_id, Vec3::new(x, y, z), None);
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn remove_client_3d_position(
    client_id: u16,
    state: State<'_, AppState>,
) -> Result<SpatialAudioStateSnapshot, String> {
    let mut engine = state
        .plugin_sdk
        .spatial_audio
        .lock()
        .map_err(|_| String::from("Spatial audio lock poisoned"))?;
    engine.remove_client_position(client_id);
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn set_listener_3d_pose(
    x: f32,
    y: f32,
    z: f32,
    forward_x: f32,
    forward_y: f32,
    forward_z: f32,
    state: State<'_, AppState>,
) -> Result<SpatialAudioStateSnapshot, String> {
    let mut engine = state
        .plugin_sdk
        .spatial_audio
        .lock()
        .map_err(|_| String::from("Spatial audio lock poisoned"))?;
    engine.set_listener_pose(
        Vec3::new(x, y, z),
        Some(Vec3::new(forward_x, forward_y, forward_z)),
        None,
    );
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn set_spatial_audio_config(
    enabled: bool,
    min_distance: f32,
    max_distance: f32,
    rolloff_factor: f32,
    state: State<'_, AppState>,
) -> Result<SpatialAudioStateSnapshot, String> {
    let mut engine = state
        .plugin_sdk
        .spatial_audio
        .lock()
        .map_err(|_| String::from("Spatial audio lock poisoned"))?;
    engine.enabled = enabled;
    engine.min_distance = min_distance.clamp(0.1, 100.0);
    engine.max_distance = max_distance.clamp(engine.min_distance + 1.0, 500.0);
    engine.rolloff_factor = rolloff_factor.clamp(0.2, 5.0);
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn get_spatial_audio_state(
    state: State<'_, AppState>,
) -> Result<SpatialAudioStateSnapshot, String> {
    let engine = state
        .plugin_sdk
        .spatial_audio
        .lock()
        .map_err(|_| String::from("Spatial audio lock poisoned"))?;
    Ok(engine.snapshot())
}

#[tauri::command]
pub async fn reset_spatial_audio(
    state: State<'_, AppState>,
) -> Result<SpatialAudioStateSnapshot, String> {
    let mut engine = state
        .plugin_sdk
        .spatial_audio
        .lock()
        .map_err(|_| String::from("Spatial audio lock poisoned"))?;
    engine.reset();
    Ok(engine.snapshot())
}
