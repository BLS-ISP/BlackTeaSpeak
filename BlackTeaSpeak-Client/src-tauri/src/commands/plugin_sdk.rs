use tauri::State;

use crate::btea::AppState;
use crate::plugins::PluginSdkStatus;

#[tauri::command]
pub async fn get_plugin_sdk_status(state: State<'_, AppState>) -> Result<PluginSdkStatus, String> {
    let status = state
        .plugin_sdk
        .status
        .lock()
        .map_err(|_| String::from("Plugin SDK status lock poisoned"))?;
    Ok(status.clone())
}

#[tauri::command]
pub async fn set_plugin_sdk_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<PluginSdkStatus, String> {
    let mut status = state
        .plugin_sdk
        .status
        .lock()
        .map_err(|_| String::from("Plugin SDK status lock poisoned"))?;
    status.enabled = enabled;
    Ok(status.clone())
}

#[tauri::command]
pub async fn resolve_tsdns_address(address: String) -> Result<String, String> {
    Ok(crate::network::tsdns_client::resolve_server_address_with_tsdns(&address).await)
}
