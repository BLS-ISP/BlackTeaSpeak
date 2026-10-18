use super::*;

pub(crate) fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn next_client_database_seed(query_accounts: &BTreeMap<String, QueryAccount>) -> u64 {
    query_accounts
        .values()
        .filter_map(|account| account.client_database_id)
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(100)
}

pub(crate) fn next_token_id_seed(tokens: &BTreeMap<u32, PrivilegeToken>) -> u32 {
    tokens
        .keys()
        .copied()
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(1)
}

pub(crate) fn next_token_action_id_seed(tokens: &BTreeMap<u32, PrivilegeToken>) -> u32 {
    tokens
        .values()
        .flat_map(|token| token.actions.iter().map(|action| action.id))
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(1)
}

pub fn stable_query_client_unique_identifier(login_name: &str) -> String {
    login_name.to_string()
}

pub fn stable_web_client_unique_identifier(public_key: &str) -> String {
    let digest = Sha1::digest(public_key.as_bytes());
    BASE64_STANDARD.encode(digest)
}

pub fn stable_web_client_database_id(unique_identifier: &str) -> u64 {
    const OFFSET: u64 = 1_000_000_000_000;
    OFFSET + (fnv1a64(unique_identifier.as_bytes()) & 0x0000_FFFF_FFFF_FFFF)
}

pub(crate) fn looks_like_blackteaspeak_unique_id(value: &str) -> bool {
    value.len() == 28
        && value.ends_with('=')
        && value.bytes().all(
            |byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'='),
        )
}

pub(crate) fn next_conversation_timestamp_seed(
    conversation_messages: &BTreeMap<u32, Vec<ConversationMessage>>,
    private_messages: &BTreeMap<u32, Vec<PrivateConversationMessage>>,
) -> u64 {
    conversation_messages
        .values()
        .flat_map(|messages| messages.iter().map(|message| message.timestamp))
        .chain(
            private_messages
                .values()
                .flat_map(|messages| messages.iter().map(|message| message.timestamp)),
        )
        .max()
        .map(|value| value.saturating_add(1))
        .unwrap_or(1)
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn default_server_groups_for_login_with_availability(
    login_name: &str,
    has_admin_group: bool,
    has_guest_group: bool,
) -> Vec<u32> {
    if login_name == "serveradmin" && has_admin_group {
        vec![6]
    } else if has_guest_group {
        vec![7]
    } else if has_admin_group {
        vec![6]
    } else {
        Vec::new()
    }
}

pub(crate) fn parse_query_bool(value: &str) -> Option<bool> {
    match value {
        "0" | "false" => Some(false),
        "1" | "true" => Some(true),
        _ => None,
    }
}

pub(crate) fn apply_channel_kind_rows(row: &mut BTreeMap<String, String>, kind: ChannelKind) {
    row.insert(
        String::from("channel_flag_permanent"),
        if kind.is_permanent() {
            String::from("1")
        } else {
            String::from("0")
        },
    );
    row.insert(
        String::from("channel_flag_semi_permanent"),
        if kind.is_semi_permanent() {
            String::from("1")
        } else {
            String::from("0")
        },
    );
}
#[allow(dead_code)]
pub(crate) fn should_persist_command(command_name: &str) -> bool {
    matches!(
        command_name,
        "login"
            | "servernotifyregister"
            | "servernotifyunregister"
            | "clientaddperm"
            | "clientdelperm"
            | "clientedit"
            | "clientmove"
            | "serveredit"
            | "servercreate"
            | "serverdelete"
            | "serverstart"
            | "serverstop"
            | "serverprocessstop"
            | "serversnapshotdeploy"
            | "logadd"
            | "instanceedit"
            | "gm"
            | "channelclientaddperm"
            | "channelclientdelperm"
            | "channeladdperm"
            | "channeldelperm"
            | "channelcreate"
            | "channeldelete"
            | "channeledit"
            | "channelmove"
            | "channelgroupadd"
            | "channelgroupaddperm"
            | "channelgroupcopy"
            | "channelgroupdel"
            | "channelgroupdelperm"
            | "channelgrouprename"
            | "musicbotcreate"
            | "musicbotdelete"
            | "musicbotqueueadd"
            | "musicbotqueueremove"
            | "musicbotqueuereorder"
            | "musicbotplayeraction"
            | "playlistaddperm"
            | "playlistclientaddperm"
            | "playlistedit"
            | "playlistsongadd"
            | "playlistsongremove"
            | "playlistsongreorder"
            | "playlistsongsetcurrent"
            | "servergroupadd"
            | "servergroupaddclient"
            | "servergroupaddperm"
            | "servergroupautoaddperm"
            | "servergroupautodelperm"
            | "servergroupcopy"
            | "servergroupdel"
            | "servergroupdelclient"
            | "servergroupdelperm"
            | "servergrouprename"
            | "privilegekeyadd"
            | "privilegekeydelete"
            | "tokenadd"
            | "tokendelete"
            | "tokenedit"
            | "tokenuse"
            | "privilegekeyuse"
            | "use"
            | "querycreate"
            | "queryrename"
            | "querychangepassword"
            | "querydelete"
            | "sendtextmessage"
            | "messageadd"
            | "messageget"
            | "messagedel"
            | "messageupdateflag"
            | "complainadd"
            | "complaindel"
            | "complaindelall"
            | "customset"
            | "customdelete"
            | "clientcustomset"
            | "clientcustomdelete"
            | "clientdbedit"
            | "clientdbdelete"
            | "setclientchannelgroup"
            | "clientsetserverquerylogin"
    )
}

pub(crate) fn synthetic_permission_id(permission_name: &str) -> u32 {
    let mut hash = 2_166_136_261_u32;
    for byte in permission_name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }

    200_000_u32.saturating_add(hash % 50_000)
}

pub(crate) fn describe_permission_name(permission_name: &str) -> String {
    if permission_name == "b_serverquery_login" {
        return String::from("Log in through ServerQuery");
    }

    if permission_name == "b_client_create_modify_serverquery_login" {
        return String::from("Create or modify ServerQuery login");
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_textmessage_send"))
    {
        let scope = permission_subject_text(subject)
            .trim_start_matches("client ")
            .to_string();
        return sentence_case(format!("Send text messages to {scope}"));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_notify_register"))
    {
        return sentence_case(format!(
            "Register for {} notifications",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_notify_unregister"))
    {
        return sentence_case(format!(
            "Unregister from {} notifications",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_view"))
    {
        return sentence_case(format!("View {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_list"))
    {
        return sentence_case(format!("List {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_search"))
    {
        return sentence_case(format!("Search {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_find"))
    {
        return sentence_case(format!("Find {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_info"))
    {
        return sentence_case(format!(
            "View {} information",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_create"))
    {
        return sentence_case(format!("Create {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_delete"))
    {
        return sentence_case(format!("Delete {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_add"))
    {
        return sentence_case(format!("Add {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_remove"))
    {
        return sentence_case(format!("Remove {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_join_ignore_password"))
    {
        return sentence_case(format!(
            "Join {} ignoring password",
            permission_subject_text(subject)
        ));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_rename"))
    {
        return sentence_case(format!("Rename {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_move"))
    {
        return sentence_case(format!("Move {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_start"))
    {
        return sentence_case(format!("Start {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_stop"))
    {
        return sentence_case(format!("Stop {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_suffix("_use"))
    {
        return sentence_case(format!("Use {}", permission_subject_text(subject)));
    }

    if let Some(subject) = permission_name
        .strip_prefix("b_")
        .and_then(|value| value.strip_prefix(""))
    {
        if let Some((prefix, suffix)) = subject.rsplit_once("_modify_") {
            return sentence_case(format!(
                "Modify {}",
                permission_subject_text(&format!("{prefix}_{suffix}"))
            ));
        }
    }

    if let Some(subject) = permission_name.strip_prefix("i_") {
        return sentence_case(permission_subject_text(subject));
    }

    sentence_case(permission_subject_text(permission_name))
}

pub(crate) fn permission_subject_text(subject: &str) -> String {
    subject
        .split('_')
        .filter(|part| !part.is_empty())
        .map(permission_token_text)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn permission_token_text(token: &str) -> String {
    match token {
        "serverinstance" => String::from("server instance"),
        "virtualserver" => String::from("virtual server"),
        "serverquery" => String::from("ServerQuery"),
        "textmessage" => String::from("text message"),
        "textmessages" => String::from("text messages"),
        "clientdb" => String::from("client database"),
        "channelgroup" => String::from("channel group"),
        "servergroup" => String::from("server group"),
        "musicgroup" => String::from("music group"),
        "welcomemessage" => String::from("welcome message"),
        "hostmessage" => String::from("host message"),
        "hostbanner" => String::from("host banner"),
        "maxclients" => String::from("max clients"),
        "ft" => String::from("file transfer"),
        "dblist" => String::from("database list"),
        "dbinfo" => String::from("database info"),
        "dbsearch" => String::from("database search"),
        "antiflood" => String::from("anti flood"),
        _ => token.replace('-', " "),
    }
}

pub(crate) fn sentence_case(text: String) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return text;
    };

    format!("{}{}", first.to_uppercase(), characters.collect::<String>())
}

pub(crate) fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn runtime_bool_flag(value: &str) -> bool {
    matches!(value, "1" | "true" | "yes" | "on")
}

pub(crate) fn current_unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

pub(crate) fn antiflood_command_cost(command_name: &str) -> u32 {
    match command_name {
        "musicbotlist"
        | "mbotlist"
        | "musicbotplayerinfo"
        | "musicbotplayeraction"
        | "mbotplayer"
        | "musicbotqueuelist"
        | "playlistlist"
        | "playlistinfo"
        | "playlistsonglist"
        | "serverrequestconnectioninfo"
        | "whoami" => 0,
        "sendtextmessage" | "serveredit" | "clientmove" | "channelcreate" | "channeldelete"
        | "channeledit" | "channelmove" => 2,
        _ => 1,
    }
}

pub(crate) fn antiflood_command_rejected(
    config: AntiFloodConfig,
    anti_flood_state: &mut AntiFloodSessionState,
    points_to_add: u32,
    now_millis: u64,
) -> bool {
    if points_to_add == 0 {
        return false;
    }
    if anti_flood_state.last_update_millis == 0 {
        anti_flood_state.last_update_millis = now_millis;
    }

    let elapsed_millis = now_millis.saturating_sub(anti_flood_state.last_update_millis);
    if config.points_tick_reduce > 0 {
        let reduced_points =
            (elapsed_millis / 1_000).saturating_mul(u64::from(config.points_tick_reduce));
        if reduced_points > 0 {
            anti_flood_state.points = anti_flood_state
                .points
                .saturating_sub(reduced_points.min(u64::from(u32::MAX)) as u32);
            let processed_millis = elapsed_millis - (elapsed_millis % 1_000);
            anti_flood_state.last_update_millis = anti_flood_state.last_update_millis.saturating_add(processed_millis);
        }
    } else {
        anti_flood_state.last_update_millis = now_millis;
    }

    if anti_flood_state.blocked_until_millis > now_millis {
        return true;
    }

    anti_flood_state.points = anti_flood_state.points.saturating_add(points_to_add);

    if config.points_needed_ip_block > 0 && anti_flood_state.points >= config.points_needed_ip_block {
        anti_flood_state.blocked_until_millis = now_millis.saturating_add(u64::from(config.ban_time).saturating_mul(1_000));
        return true;
    }

    if config.points_needed_command_block > 0 && anti_flood_state.points >= config.points_needed_command_block {
        return true;
    }

    false
}

pub(crate) fn property_catalog() -> Vec<(&'static str, u32, &'static str)> {
    vec![
        ("virtualserver_id", 256, "SERVER"),
        ("virtualserver_unique_identifier", 392, "SERVER"),
        ("virtualserver_name", 408, "SERVER"),
        ("virtualserver_port", 408, "SERVER"),
        ("virtualserver_clientsonline", 128, "SERVER"),
        ("virtualserver_queryclientsonline", 128, "SERVER"),
        ("virtualserver_antiflood_points_tick_reduce", 256, "SERVER"),
        (
            "virtualserver_antiflood_points_needed_command_block",
            256,
            "SERVER",
        ),
        (
            "virtualserver_antiflood_points_needed_ip_block",
            256,
            "SERVER",
        ),
        ("virtualserver_antiflood_ban_time", 256, "SERVER"),
        ("cid", 256, "CHANNEL"),
        ("pid", 256, "CHANNEL"),
        ("channel_name", 408, "CHANNEL"),
        ("channel_topic", 408, "CHANNEL"),
        ("channel_description", 408, "CHANNEL"),
        ("clid", 256, "CLIENT"),
        ("client_database_id", 256, "CLIENT"),
        ("client_nickname", 408, "CLIENT"),
        ("client_unique_identifier", 392, "CLIENT"),
        ("client_platform", 256, "CLIENT"),
        ("serverinstance_database_version", 256, "INSTANCE"),
        ("serverinstance_filetransfer_port", 408, "INSTANCE"),
        (
            "serverinstance_template_guest_serverquery_group",
            256,
            "INSTANCE",
        ),
        (
            "serverinstance_template_admin_serverquery_group",
            256,
            "INSTANCE",
        ),
        ("serverinstance_template_serveradmin_group", 256, "INSTANCE"),
        ("sgid", 256, "GROUP"),
        ("name", 408, "GROUP"),
        ("type", 256, "GROUP"),
        ("iconid", 408, "GROUP"),
        ("connection_packets_sent_total", 128, "CONNECTION"),
        ("connection_packets_received_total", 128, "CONNECTION"),
        ("connection_bytes_sent_total", 128, "CONNECTION"),
        ("connection_bytes_received_total", 128, "CONNECTION"),
        ("connection_connected_time", 128, "CONNECTION"),
    ]
}

pub(crate) fn ordered_sibling_ids(
    channels: &[Channel],
    parent_id: u32,
    excluded_channel_id: Option<u32>,
) -> Vec<u32> {
    let siblings = channels
        .iter()
        .filter(|channel| channel.parent_id == parent_id && Some(channel.id) != excluded_channel_id)
        .collect::<Vec<_>>();
    let mut remaining = siblings
        .iter()
        .map(|channel| channel.id)
        .collect::<BTreeSet<_>>();
    let mut ordered_ids = Vec::new();
    let mut previous_id = 0;

    loop {
        let Some(next_id) = siblings
            .iter()
            .filter(|channel| remaining.contains(&channel.id) && channel.order == previous_id)
            .map(|channel| channel.id)
            .min()
        else {
            break;
        };

        ordered_ids.push(next_id);
        remaining.remove(&next_id);
        previous_id = next_id;
    }

    ordered_ids.extend(remaining);
    ordered_ids
}

pub(crate) fn collect_visible_channel_ids_for_client(
    runtime: &BaselineRuntime,
    channels: &[Channel],
    server_id: u32,
    parent_id: u32,
    client_database_id: u64,
    visible_channel_ids: &mut BTreeSet<u32>,
) {
    for channel_id in ordered_sibling_ids(channels, parent_id, None) {
        if !runtime.web_client_can_view_channel(server_id, channel_id, client_database_id) {
            continue;
        }

        visible_channel_ids.insert(channel_id);
        collect_visible_channel_ids_for_client(
            runtime,
            channels,
            server_id,
            channel_id,
            client_database_id,
            visible_channel_ids,
        );
    }
}

pub(crate) fn ordered_visible_channel_ids(
    channels: &[Channel],
    parent_id: u32,
    visible_channel_ids: &BTreeSet<u32>,
) -> Vec<u32> {
    let mut ordered_ids = Vec::new();

    for channel_id in ordered_sibling_ids(channels, parent_id, None) {
        if !visible_channel_ids.contains(&channel_id) {
            continue;
        }

        ordered_ids.push(channel_id);
        ordered_ids.extend(ordered_visible_channel_ids(
            channels,
            channel_id,
            visible_channel_ids,
        ));
    }

    ordered_ids
}

pub(crate) fn relink_sibling_orders(channels: &mut [Channel], parent_id: u32, ordered_ids: &[u32]) {
    let mut previous_id = 0;
    for channel_id in ordered_ids {
        if let Some(channel) = channels
            .iter_mut()
            .find(|channel| channel.id == *channel_id && channel.parent_id == parent_id)
        {
            channel.order = previous_id;
            previous_id = *channel_id;
        }
    }

}

pub(crate) fn resolve_insert_index(sibling_ids: &[u32], requested_order: Option<u32>) -> Option<usize> {
    match requested_order {
        Some(0) => Some(0),
        Some(anchor_id) => sibling_ids
            .iter()
            .position(|channel_id| *channel_id == anchor_id)
            .map(|index| index + 1),
        None => Some(sibling_ids.len()),
    }
}

pub(crate) fn channel_is_descendant(channels: &[Channel], candidate_parent_id: u32, channel_id: u32) -> bool {
    let mut current_parent_id = candidate_parent_id;

    while current_parent_id != 0 {
        if current_parent_id == channel_id {
            return true;
        }

        current_parent_id = channels
            .iter()
            .find(|channel| channel.id == current_parent_id)
            .map(|channel| channel.parent_id)
            .unwrap_or(0);
    }

    false
}

#[allow(dead_code)]
pub(crate) fn _build_version_snapshot(build_version: &BuildVersion) -> String {
    format!(
        "{}:{}:{}",
        build_version.build_name, build_version.build_version, build_version.build_index
    )
}

#[allow(dead_code)]
pub(crate) fn _command_summary(command: &CommandSpec) -> String {
    format!("{}:{}", command.name, command.category)
}
