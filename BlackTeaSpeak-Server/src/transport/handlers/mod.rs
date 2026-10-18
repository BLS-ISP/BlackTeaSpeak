pub use crate::teaspeak_ts3 as ts3;
pub mod btea;

use crate::runtime::{BaselineRuntime, QuerySessionState};
use crate::query::CommandRequest;

#[derive(Clone)]
pub struct BootstrapData {
    pub server_id: u64,
    pub server_name: String,
    pub welcome_message: String,
    pub default_cid: u64,
    pub client_nickname: String,
    pub client_id: u64,
    pub dbid: u64,
    pub client_uid: String,
    pub channels: Vec<crate::runtime::models::Channel>,
}

pub fn perform_client_bootstrap(
    runtime: &mut BaselineRuntime,
    session: &mut QuerySessionState,
    request: &CommandRequest,
    unique_identifier: &str,
    _is_native_ts3: bool,
) -> Result<BootstrapData, String> {
    let client_nickname = request
        .named_args
        .get("client_nickname")
        .cloned()
        .unwrap_or_else(|| session.client_nickname.clone());

    session.client_nickname = client_nickname.clone();

    let info = runtime.web_server_init_info().ok_or("Server info not found")?;
    let dbid = session
        .actor_client_database_id_override
        .unwrap_or(session.client_id + 1000);
    let provided_pw = request
        .named_args
        .get("client_server_password")
        .or_else(|| request.named_args.get("password"))
        .map(String::as_str)
        .unwrap_or("");
    let temp_target = runtime
        .verify_server_password(info.server_id, provided_pw, Some(dbid))
        .map_err(|_| String::from("invalid server password"))?;
    let mut default_cid = runtime.web_default_channel_id(info.server_id).unwrap_or(1);
    if let Some((target_cid, _)) = temp_target
        && runtime.channel_exists_for_server(info.server_id, target_cid)
    {
        default_cid = target_cid;
    }

    session.selected_virtual_server_id = Some(info.server_id);
    session.current_channel_id = Some(default_cid);
    session.actor_client_database_id_override = Some(dbid);
    session.subscribed_channels.insert(default_cid);

    runtime.upsert_web_client(
        session.client_id,
        info.server_id,
        default_cid,
        client_nickname.clone(),
        unique_identifier.to_string(),
        dbid,
        "BTEA".to_string(),
        "Windows".to_string(),
        session.connection_ip.clone(),
    );

    let channels = runtime
        .store
        .channels
        .get(&info.server_id)
        .cloned()
        .unwrap_or_default();

    Ok(BootstrapData {
        server_id: info.server_id as u64,
        server_name: info.server_name.clone(),
        welcome_message: info.welcome_message.clone(),
        default_cid: default_cid as u64,
        client_nickname,
        client_id: session.client_id,
        dbid,
        client_uid: unique_identifier.to_string(),
        channels,
    })
}
