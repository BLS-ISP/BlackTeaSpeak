use super::*;
pub(crate) fn session_has_permission_actor(session: &QuerySessionState) -> bool {
    session.authenticated_login.is_some() || session.actor_client_database_id_override.is_some()
}

pub(crate) fn build_permission_map(
    group: Option<&PermissionGroupSpec>,
) -> BTreeMap<String, PermissionAssignment> {
    group
        .map(|group| {
            group
                .permissions
                .iter()
                .map(|permission| {
                    (
                        permission.name.clone(),
                        PermissionAssignment {
                            value: permission.value,
                            negated: permission.negated != 0,
                            skipped: permission.skipped != 0,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default()
}

pub(crate) fn build_named_permission_map(
    specs: &FoundationSpecs,
    group_name: &str,
    target_name: &str,
) -> BTreeMap<String, PermissionAssignment> {
    build_permission_map(
        specs
            .permission_groups
            .iter()
            .find(|group| group.name == group_name && group.target_name == target_name),
    )
}

pub(crate) fn build_permission_catalog(
    specs: &FoundationSpecs,
) -> BTreeMap<String, PermissionCatalogEntry> {
    specs
        .permission_catalog
        .iter()
        .map(|entry| (entry.name.clone(), entry.clone()))
        .collect()
}

pub(crate) fn load_blackteaweb_permission_ids(workspace_root: &Path) -> BTreeMap<String, u32> {
    let path = workspace_root
        .join("BlackTeaWeb")
        .join("shared")
        .join("js")
        .join("permission")
        .join("PermissionType.ts");
    parse_blackteaweb_permission_ids(&path).unwrap_or_default()
}

pub(crate) fn parse_blackteaweb_permission_ids(path: &Path) -> Option<BTreeMap<String, u32>> {
    let content = fs::read_to_string(path).ok()?;
    let mut ids_by_name = BTreeMap::new();

    for line in content.lines() {
        let Some(id_start) = line.find("Permission ID:") else {
            continue;
        };
        let segments = line.split('"').collect::<Vec<_>>();
        if segments.len() < 3 {
            continue;
        }

        let permission_name = segments[1].trim();
        if permission_name.is_empty() {
            continue;
        }

        let Some(id_text) = line[id_start + "Permission ID:".len()..]
            .split("*/")
            .next()
            .map(str::trim)
        else {
            continue;
        };
        let Some(permission_id) = id_text.parse::<u32>().ok() else {
            continue;
        };
        ids_by_name.insert(permission_name.to_string(), permission_id);
    }

    Some(ids_by_name)
}

pub(crate) fn build_web_group_row(
    id_key: &str,
    id: u32,
    name: &str,
    group_type: u32,
    icon_id: i64,
    save_db: bool,
    required_member_add_power: i64,
    required_member_remove_power: i64,
    required_modify_power: i64,
) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    row.insert(String::from(id_key), id.to_string());
    row.insert(String::from("name"), String::from(name));
    row.insert(String::from("type"), group_type.to_string());
    row.insert(String::from("iconid"), icon_id.to_string());
    row.insert(
        String::from("savedb"),
        if save_db {
            String::from("1")
        } else {
            String::from("0")
        },
    );
    row.insert(String::from("sortid"), String::from("0"));
    row.insert(String::from("namemode"), String::from("0"));
    row.insert(
        String::from("n_member_addp"),
        required_member_add_power.to_string(),
    );
    row.insert(
        String::from("n_member_removep"),
        required_member_remove_power.to_string(),
    );
    row.insert(String::from("n_modifyp"), required_modify_power.to_string());
    row
}

pub(crate) fn permission_value_or_default(
    permissions: &BTreeMap<String, PermissionAssignment>,
    candidate_names: &[&str],
) -> i64 {
    candidate_names
        .iter()
        .find_map(|permission_name| {
            permissions
                .get(*permission_name)
                .map(|assignment| assignment.value)
        })
        .unwrap_or(0)
}

pub(crate) fn permission_power_max_or_default(
    permissions: &BTreeMap<String, PermissionAssignment>,
    candidate_names: &[&str],
) -> i64 {
    candidate_names
        .iter()
        .filter_map(|permission_name| {
            permissions
                .get(*permission_name)
                .map(|assignment| assignment.value)
        })
        .max()
        .unwrap_or(0)
}

pub(crate) fn check_server_group_membership_change(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    self_target: bool,
    add_group: bool,
) -> std::result::Result<(), &'static str> {
    let (actor_power_names, needed_power_names, failed_permission_name) =
        match (add_group, self_target) {
            (true, true) => (
                &["i_server_group_self_add_power", "i_group_member_add_power"][..],
                &[
                    "i_server_group_needed_member_add_power",
                    "i_group_needed_member_add_power",
                ][..],
                "i_server_group_self_add_power",
            ),
            (true, false) => (
                &[
                    "i_server_group_member_add_power",
                    "i_group_member_add_power",
                ][..],
                &[
                    "i_server_group_needed_member_add_power",
                    "i_group_needed_member_add_power",
                ][..],
                "i_group_member_add_power",
            ),
            (false, true) => (
                &[
                    "i_server_group_self_remove_power",
                    "i_group_member_remove_power",
                ][..],
                &[
                    "i_server_group_needed_member_remove_power",
                    "i_group_needed_member_remove_power",
                ][..],
                "i_server_group_self_remove_power",
            ),
            (false, false) => (
                &[
                    "i_server_group_member_remove_power",
                    "i_group_member_remove_power",
                ][..],
                &[
                    "i_server_group_needed_member_remove_power",
                    "i_group_needed_member_remove_power",
                ][..],
                "i_group_member_remove_power",
            ),
        };

    if permission_power_max_or_default(actor_permissions, actor_power_names)
        < permission_power_max_or_default(target_permissions, needed_power_names)
    {
        return Err(failed_permission_name);
    }

    Ok(())
}

pub(crate) fn check_permission_edit_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    target_needed_modify_names: &[&str],
) -> std::result::Result<(), &'static str> {
    check_group_modify_allowed(
        actor_permissions,
        target_permissions,
        target_needed_modify_names,
    )?;

    if permission_power_max_or_default(actor_permissions, &["b_permission_modify_power_ignore"]) > 0
    {
        return Ok(());
    }

    if permission_power_max_or_default(
        actor_permissions,
        &[
            "i_permission_modify_power",
            "i_channel_permission_modify_power",
            "i_client_permission_modify_power",
        ],
    ) < 1
    {
        return Err("i_permission_modify_power");
    }

    Ok(())
}

pub(crate) fn check_group_modify_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    target_needed_modify_names: &[&str],
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(
        actor_permissions,
        &[
            "i_group_modify_power",
            "i_server_group_modify_power",
            "i_channel_group_modify_power",
        ],
    ) < permission_power_max_or_default(target_permissions, target_needed_modify_names)
    {
        return Err("i_group_modify_power");
    }

    Ok(())
}

pub(crate) fn check_required_permission(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    candidate_names: &[&str],
    failed_permission_name: &'static str,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, candidate_names) < 1 {
        return Err(failed_permission_name);
    }

    Ok(())
}

pub(crate) fn check_channel_group_membership_change(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    add_group: bool,
) -> std::result::Result<(), &'static str> {
    let (actor_power_names, needed_power_names, failed_permission_name) = if add_group {
        (
            &["i_group_member_add_power"][..],
            &[
                "i_channel_group_needed_member_add_power",
                "i_group_needed_member_add_power",
            ][..],
            "i_group_member_add_power",
        )
    } else {
        (
            &["i_group_member_remove_power"][..],
            &[
                "i_channel_group_needed_member_remove_power",
                "i_group_needed_member_remove_power",
            ][..],
            "i_group_member_remove_power",
        )
    };

    if permission_power_max_or_default(actor_permissions, actor_power_names)
        < permission_power_max_or_default(target_permissions, needed_power_names)
    {
        return Err(failed_permission_name);
    }

    Ok(())
}

pub(crate) fn check_channel_modify_power_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, &["i_channel_modify_power"])
        < permission_power_max_or_default(target_permissions, &["i_channel_needed_modify_power"])
    {
        return Err("i_channel_modify_power");
    }

    Ok(())
}

pub(crate) fn check_channel_delete_power_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, &["i_channel_delete_power"])
        < permission_power_max_or_default(target_permissions, &["i_channel_needed_delete_power"])
    {
        return Err("i_channel_delete_power");
    }

    Ok(())
}

pub(crate) fn check_playlist_power_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    actor_power_names: &[&str],
    needed_power_names: &[&str],
    failed_permission_name: &'static str,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, actor_power_names)
        < permission_power_max_or_default(target_permissions, needed_power_names)
    {
        return Err(failed_permission_name);
    }

    Ok(())
}





// RECOVERED DROPPED LINES:
pub(crate) fn seed_store(
    admin_permissions: &BTreeMap<String, PermissionAssignment>,
    guest_permissions: &BTreeMap<String, PermissionAssignment>,
    server_admin_permissions: &BTreeMap<String, PermissionAssignment>,
    channel_admin_permissions: &BTreeMap<String, PermissionAssignment>,
    channel_operator_permissions: &BTreeMap<String, PermissionAssignment>,
    channel_guest_permissions: &BTreeMap<String, PermissionAssignment>,
) -> InMemoryStore {
    let mut query_accounts = BTreeMap::new();
    query_accounts.insert(
        String::from("serveradmin"),
        QueryAccount {
            login_name: String::from("serveradmin"),
            password: String::from("serveradmin"),
            server_id: Some(1),
            client_database_id: Some(1),
            server_groups: vec![6],
            permissions: BTreeMap::new(),
        },
    );

    let mut server_groups = BTreeMap::new();
    server_groups.insert(
        6,
        ServerGroup {
            id: 6,
            name: String::from("Admin Server Query"),
            group_type: 1,
            icon_id: 300,
            save_db: true,
            permissions: admin_permissions.clone(),
        },
    );
    server_groups.insert(
        7,
        ServerGroup {
            id: 7,
            name: String::from("Guest Server Query"),
            group_type: 1,
            icon_id: 0,
            save_db: true,
            permissions: guest_permissions.clone(),
        },
    );
    server_groups.insert(
        8,
        ServerGroup {
            id: 8,
            name: String::from("Normal"),
            group_type: 1,
            icon_id: 0,
            save_db: true,
            permissions: BTreeMap::new(),
        },
    );
    server_groups.insert(
        9,
        ServerGroup {
            id: 9,
            name: String::from("Server Admin"),
            group_type: 1,
            icon_id: server_admin_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(300),
            save_db: true,
            permissions: server_admin_permissions.clone(),
        },
    );

    let mut channel_groups = BTreeMap::new();
    channel_groups.insert(
        8,
        ChannelGroup {
            id: 8,
            name: String::from("Channel Admin"),
            group_type: 2,
            icon_id: channel_admin_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(100),
            save_db: true,
            permissions: channel_admin_permissions.clone(),
        },
    );
    channel_groups.insert(
        9,
        ChannelGroup {
            id: 9,
            name: String::from("Operator"),
            group_type: 1,
            icon_id: channel_operator_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(200),
            save_db: true,
            permissions: channel_operator_permissions.clone(),
        },
    );
    channel_groups.insert(
        10,
        ChannelGroup {
            id: 10,
            name: String::from("Guest"),
            group_type: 2,
            icon_id: channel_guest_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(0),
            save_db: true,
            permissions: channel_guest_permissions.clone(),
        },
    );

    let mut virtual_servers = BTreeMap::new();
    virtual_servers.insert(
        1,
        VirtualServer {
            id: 1,
            port: 9987,
            name: String::from("BlackTeaSpeak Compat"),
            unique_identifier: String::from("compat-baseline-uid"),
            welcome_message: String::from("Welcome to BlackTeaSpeak Compat"),
            host_message: String::new(),
            host_message_mode: 0,
            ask_for_privilegekey: 0,
            max_clients: 128,
            antiflood_points_tick_reduce: 10,
            antiflood_points_needed_command_block: 150,
            antiflood_points_needed_ip_block: 250,
            antiflood_ban_time: 300,
        },
    );

    let mut channels = BTreeMap::new();
    channels.insert(
        1,
        vec![
            Channel {
                id: 1,
                parent_id: 0,
                order: 0,
                kind: ChannelKind::Permanent,
                name: String::from("Default Channel"),
                topic: String::from("Default Channel has no topic"),
                description: String::new(),
                password: String::new(),
                codec: 4,
                codec_quality: 10,
                maxclients: -1,
                maxfamilyclients: -1,
                flag_default: true,
                flag_password: false,
                permissions: BTreeMap::new(),
            },
            Channel {
                id: 2,
                parent_id: 0,
                order: 1,
                kind: ChannelKind::Permanent,
                name: String::from("Music Lounge"),
                topic: String::from("Music bot staging area"),
                description: String::new(),
                password: String::new(),
                codec: 4,
                codec_quality: 10,
                maxclients: -1,
                maxfamilyclients: -1,
                flag_default: false,
                flag_password: false,
                permissions: BTreeMap::new(),
            },
        ],
    );

    let mut clients = BTreeMap::new();
    clients.insert(
        40,
        Client {
            database_id: 40,
            unique_identifier: String::from("compat-seed-user-40"),
            nickname: String::from("ScP"),
            description: String::new(),
            created_at: current_unix_timestamp(),
            last_connected_at: current_unix_timestamp(),
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
            client_flag_avatar: String::new(),
        },
    );
    clients.insert(
        41,
        Client {
            database_id: 41,
            unique_identifier: String::from("compat-seed-user-41"),
            nickname: String::from("Rabe85"),
            description: String::new(),
            created_at: current_unix_timestamp(),
            last_connected_at: current_unix_timestamp(),
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
            client_flag_avatar: String::new(),
        },
    );
    clients.insert(
        42,
        Client {
            database_id: 42,
            unique_identifier: String::from("compat-seed-user-42"),
            nickname: String::from("DJ Mix"),
            description: String::new(),
            created_at: current_unix_timestamp(),
            last_connected_at: current_unix_timestamp(),
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
            client_flag_avatar: String::new(),
        },
    );

    let mut online_clients = BTreeMap::new();
    online_clients.insert(
        10,
        OnlineClient {
            id: 10,
            database_id: 40,
            unique_identifier: String::from("compat-seed-user-40"),
            nickname: String::from("ScP"),
            last_seen_at: current_unix_timestamp(),
            away: false,
            away_message: String::new(),
            input_muted: false,
            output_muted: false,
            server_id: 1,
            channel_id: 1,
            client_type: 0,
            version: String::from("BlackTeaSpeak 1.5.6 compat-seed"),
            platform: String::from("Windows"),
            country: String::from("DE"),
            connection_ip: String::from("198.51.100.10"),
            server_groups: vec![8],
            connected_at: current_unix_timestamp(),
            extra_properties: BTreeMap::new(),
        },
    );
    online_clients.insert(
        11,
        OnlineClient {
            id: 11,
            database_id: 41,
            unique_identifier: String::from("compat-seed-user-41"),
            nickname: String::from("Rabe85"),
            last_seen_at: current_unix_timestamp(),
            away: false,
            away_message: String::new(),
            input_muted: false,
            output_muted: false,
            server_id: 1,
            channel_id: 1,
            client_type: 0,
            version: String::from("BlackTeaSpeak 1.5.6 compat-seed"),
            platform: String::from("Linux"),
            country: String::from("AT"),
            connection_ip: String::from("198.51.100.11"),
            server_groups: vec![8],
            connected_at: current_unix_timestamp(),
            extra_properties: BTreeMap::new(),
        },
    );
    online_clients.insert(
        12,
        OnlineClient {
            id: 12,
            database_id: 42,
            unique_identifier: String::from("compat-seed-user-42"),
            nickname: String::from("DJ Mix"),
            last_seen_at: current_unix_timestamp(),
            away: false,
            away_message: String::new(),
            input_muted: false,
            output_muted: false,
            server_id: 1,
            channel_id: 2,
            client_type: 0,
            version: String::from("BlackTeaSpeak 1.5.6 compat-seed"),
            platform: String::from("macOS"),
            country: String::from("CH"),
            connection_ip: String::from("198.51.100.12"),
            server_groups: vec![8],
            connected_at: current_unix_timestamp(),
            extra_properties: {
                let mut row = BTreeMap::new();
                row.insert(String::from("client_type_exact"), String::from("4"));
                row.insert(String::from("player_state"), String::from("4"));
                row.insert(String::from("player_volume"), String::from("1"));
                row.insert(String::from("client_playlist_id"), String::from("0"));
                row.insert(String::from("client_disabled"), String::from("0"));
                row.insert(
                    String::from("client_flag_notify_song_change"),
                    String::from("0"),
                );
                row.insert(String::from("client_bot_type"), String::from("0"));
                row.insert(String::from("client_uptime_mode"), String::from("0"));
                row
            },
        },
    );

    let channel_group_assignments = vec![
        ChannelGroupAssignment {
            channel_id: 1,
            client_database_id: 40,
            channel_group_id: 10,
        },
        ChannelGroupAssignment {
            channel_id: 1,
            client_database_id: 41,
            channel_group_id: 10,
        },
        ChannelGroupAssignment {
            channel_id: 2,
            client_database_id: 42,
            channel_group_id: 10,
        },
    ];

    let mut music_bots = BTreeMap::new();
    music_bots.insert(
        1,
        MusicBot {
            id: 1,
            server_id: 1,
            client_database_id: 42,
            linked_client_id: Some(12),
            playlist_id: 1,
            current_song_id: None,
            next_song_id: 1,
            state: MusicBotState::Stopped,
            player_volume: String::from("1"),
            playlist_title: String::new(),
            playlist_description: String::new(),
            playlist_flag_delete_played: false,
            playlist_flag_finished: false,
            playlist_replay_mode: 0,
            playlist_max_songs: 0,
            permissions: BTreeMap::new(),
            client_permissions: Vec::new(),
            current_song_started_at_millis: None,
            current_song_progress_millis: 0,
            queue: Vec::new(),
        },
    );

    InMemoryStore {
        query_accounts,
        server_groups,
        channel_groups,
        virtual_servers,
        channels,
        channel_group_assignments,
        channel_client_permissions: Vec::new(),
        client_permissions: Vec::new(),
        conversation_messages: BTreeMap::new(),
        private_messages: BTreeMap::new(),
        tokens: BTreeMap::new(),
        active_bans: BTreeMap::new(),
        online_clients,
        clients,
        music_bots,
        next_query_client_id: 1,
        next_client_database_id: 100,
        next_conversation_timestamp: 1,
        next_ban_id: 1,
        next_token_id: 1,
        next_token_action_id: 1,
        db: std::sync::Arc::new(crate::database::Database::new(":memory:").unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .to_path_buf()
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "BlackTeaSpeak-Server-permissions-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    fn create_test_runtime(label: &str) -> BaselineRuntime {
        let state_path = unique_temp_dir(label).join("runtime-state.json");
        create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should load")
    }

    fn login_query_serveradmin(runtime: &mut BaselineRuntime, client_id: u64) -> QuerySessionState {
        let mut session = QuerySessionState {
            client_id,
            ..QuerySessionState::default()
        };
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );
        session
    }

    fn extract_field<'a>(rendered: &'a str, field_name: &str) -> Option<&'a str> {
        let prefix = format!("{field_name}=");
        rendered
            .split(|character: char| matches!(character, ' ' | '|' | '\n' | '\r' | '\t'))
            .find_map(|segment| segment.strip_prefix(&prefix))
    }

    #[test]
    fn server_admin_group_is_seeded_and_reported_via_instanceinfo() {
        let mut runtime = create_test_runtime("server-admin-seeded");
        let mut session = login_query_serveradmin(&mut runtime, 9001);

        let instanceinfo = runtime.execute("instanceinfo", &mut session);
        assert!(instanceinfo.contains("serverinstance_template_serveradmin_group=9"));

        let groups = runtime.execute("servergrouplist", &mut session);
        assert!(groups.contains("sgid=9"));
        assert!(groups.contains(r"name=Server\sAdmin"));
    }

    #[test]
    fn privilegekeyuse_promotes_normal_online_client_to_server_admin() {
        let mut runtime = create_test_runtime("privilegekey-server-admin");
        let mut admin_session = login_query_serveradmin(&mut runtime, 9001);
        let server_admin_group_id = runtime
            .store
            .server_groups
            .iter()
            .find(|(_, group)| group.name == "Server Admin")
            .map(|(group_id, _)| *group_id)
            .expect("server admin group should be seeded");

        let mut client_session = QuerySessionState {
            client_id: 9002,
            actor_client_database_id_override: Some(40),
            selected_virtual_server_id: Some(1),
            ..QuerySessionState::default()
        };

        let denied_edit = runtime.execute(
            r"serveredit virtualserver_name=Denied\sPromotion",
            &mut client_session,
        );
        assert!(denied_edit.contains("error id=2568"));

        let created_key = runtime.execute(
            &format!(
                r"privilegekeyadd token_description=Server\sAdmin\sGrant token_max_uses=1 action_type=2 action_id1={}",
                server_admin_group_id
            ),
            &mut admin_session,
        );
        let token_value = extract_field(&created_key, "token")
            .unwrap_or_else(|| panic!("privilegekeyadd should expose token, got: {}", created_key))
            .to_string();

        let use_response = runtime.execute(
            &format!("privilegekeyuse token={token_value}"),
            &mut client_session,
        );
        assert!(use_response.contains("error id=0 msg=ok"));

        let groups_after_use = runtime.execute("servergroupsbyclientid cldbid=40", &mut admin_session);
        assert!(groups_after_use.contains(&format!("sgid={server_admin_group_id}")));

        let promoted_edit = runtime.execute(
            r"serveredit virtualserver_name=Promoted\sServer\sAdmin",
            &mut client_session,
        );
        assert!(promoted_edit.contains("error id=0 msg=ok"));

        let serverinfo = runtime.execute("serverinfo", &mut admin_session);
        assert!(serverinfo.contains(r"virtualserver_name=Promoted\sServer\sAdmin"));
    }

    #[test]
    fn persisted_state_backfills_server_admin_group_without_overwriting_existing_group_ids() {
        let state_path = unique_temp_dir("server-admin-backfill").join("runtime-state.json");

        {
            let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
                .expect("runtime should load");
            let mut session = login_query_serveradmin(&mut runtime, 9001);
            assert!(runtime
                .execute(
                    r"serveredit virtualserver_name=Persisted\sServer\sAdmin\sProbe",
                    &mut session,
                )
                .contains("error id=0 msg=ok"));
        }

        let mut persisted: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&state_path).expect("persisted state should exist"),
        )
        .expect("persisted state should parse");
        let groups = persisted["server_groups"]
            .as_object_mut()
            .expect("server_groups should be an object");
        let mut legacy_group = groups
            .remove("9")
            .expect("fresh state should persist server admin group under 9");
        legacy_group["name"] = serde_json::Value::String(String::from("Legacy Custom"));
        groups.insert(String::from("9"), legacy_group);
        fs::write(
            &state_path,
            serde_json::to_vec_pretty(&persisted).expect("persisted state should serialize"),
        )
        .expect("mutated state should write");

        let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload from mutated state");
        assert_eq!(
            runtime
                .store
                .server_groups
                .get(&9)
                .expect("legacy custom group should remain")
                .name,
            "Legacy Custom"
        );

        let server_admin_group = runtime
            .store
            .server_groups
            .iter()
            .find(|(_, group)| group.name == "Server Admin")
            .expect("server admin group should be backfilled");
        assert_ne!(*server_admin_group.0, 9);
        assert!(!server_admin_group.1.permissions.is_empty());
    }

    #[test]
    fn query_servergroup_commands_support_normal_online_clients() {
        let mut runtime = create_test_runtime("query-servergroup-online-client");
        let mut session = login_query_serveradmin(&mut runtime, 9001);

        let initial_groups = runtime.execute("servergroupsbyclientid cldbid=40", &mut session);
        assert!(initial_groups.contains("sgid=8"));
        assert!(!initial_groups.contains("query account not found"));

        let add_response = runtime.execute("servergroupaddclient sgid=6 cldbid=40", &mut session);
        assert!(add_response.contains("error id=0 msg=ok"));
        assert!(
            runtime
                .store
                .online_clients
                .values()
                .find(|client| client.database_id == 40)
                .expect("online client 40 should exist")
                .server_groups
                .contains(&6)
        );

        let groups_after_add = runtime.execute("servergroupsbyclientid cldbid=40", &mut session);
        assert!(groups_after_add.contains("sgid=6"));
        assert!(groups_after_add.contains("sgid=8"));

        let del_response = runtime.execute("servergroupdelclient sgid=6 cldbid=40", &mut session);
        assert!(del_response.contains("error id=0 msg=ok"));
        let client = runtime
            .store
            .online_clients
            .values()
            .find(|client| client.database_id == 40)
            .expect("online client 40 should exist after delete");
        assert!(!client.server_groups.contains(&6));
        assert!(client.server_groups.contains(&8));

        let groups_after_del = runtime.execute("servergroupsbyclientid cldbid=40", &mut session);
        assert!(!groups_after_del.contains("sgid=6"));
        assert!(groups_after_del.contains("sgid=8"));
        assert!(!groups_after_del.contains("query account not found"));
    }
}