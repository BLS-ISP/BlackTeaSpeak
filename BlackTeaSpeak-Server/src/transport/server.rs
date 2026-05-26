use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::{Context, Result};
use crate::query::*;
use crate::runtime::*;
use super::*;
pub(crate) fn matches_channel_tree_subscription(
    subscription_channel_id: Option<u32>,
    channel: &ChannelSnapshot,
    previous_parent_id: Option<u32>,
) -> bool {
    subscription_channel_id.is_some_and(|channel_id| {
        channel_id == channel.id
            || channel_id == channel.parent_id
            || previous_parent_id == Some(channel_id)
    })
}
pub fn render_notification(notification: &TransportNotification) -> String {
    match notification {
        TransportNotification::ClientEnterView {
            presence,
            from_channel_id,
            reason_id,
        } => render_message(
            "notifycliententerview",
            &[
                ("clid", presence.client_id.to_string()),
                ("client_nickname", presence.login_name.clone()),
                ("client_type", presence.client_type.to_string()),
                ("ctid", presence.channel_id.to_string()),
                ("cfid", from_channel_id.unwrap_or(0).to_string()),
                ("reasonid", reason_id.to_string()),
            ],
        ),
        TransportNotification::ClientUpdated { before, after, .. } => {
            render_message_owned("notifyclientupdated", client_update_fields(before, after))
        }
        TransportNotification::ClientPoke {
            invoker_id,
            invoker_name,
            invoker_uid,
            message,
            ..
        } => render_message(
            "notifyclientpoke",
            &[
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
                ("msg", message.clone()),
            ],
        ),
        TransportNotification::ClientMoved {
            presence,
            from_channel_id,
            reason_id,
            reason_message,
            invoker_id,
            invoker_name,
            invoker_uid,
        } => render_message(
            "notifyclientmoved",
            &[
                ("clid", presence.client_id.to_string()),
                ("cfid", from_channel_id.to_string()),
                ("ctid", presence.channel_id.to_string()),
                ("reasonid", reason_id.to_string()),
                ("reasonmsg", reason_message.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
            ],
        ),
        TransportNotification::ClientLeftView {
            presence,
            to_channel_id,
            reason_id,
            reason_message,
            invoker_id,
            invoker_name,
            invoker_uid,
            ban_time,
        } => render_message(
            "notifyclientleftview",
            &[
                ("clid", presence.client_id.to_string()),
                ("cfid", presence.channel_id.to_string()),
                ("ctid", to_channel_id.unwrap_or(0).to_string()),
                ("reasonid", reason_id.to_string()),
                ("reasonmsg", reason_message.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
                ("bantime", ban_time.unwrap_or(0).to_string()),
            ],
        ),
        TransportNotification::ChannelEdited {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychanneledited",
            &[
                ("cid", channel.id.to_string()),
                ("channel_name", channel.name.clone()),
                ("channel_topic", channel.topic.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ChannelCreated {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychannelcreated",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("channel_order", channel.order.to_string()),
                ("channel_name", channel.name.clone()),
                ("channel_topic", channel.topic.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ChannelDeleted {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychanneldeleted",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ChannelMoved {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychannelmoved",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("order", channel.order.to_string()),
                ("channel_name", channel.name.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ServerEdited {
            before,
            after,
            invoker_id,
            invoker_name,
            ..
        } => render_message_owned(
            "notifyserveredited",
            server_edited_fields(before, after, *invoker_id, invoker_name),
        ),
        TransportNotification::TalkStatus {
            client_id,
            is_talking,
            is_whisper,
            ..
        } => render_message(
            "notifytalkstatus",
            &[
                ("clid", client_id.to_string()),
                ("status", if *is_talking { "1".to_string() } else { "0".to_string() }),
                ("isreceivedwhisper", if *is_whisper { "1".to_string() } else { "0".to_string() }),
            ],
        ),
        TransportNotification::TextMessage {
            target,
            invoker_id,
            invoker_name,
            invoker_uid,
        } => render_message(
            "notifytextmessage",
            &[
                ("targetmode", target.target_mode.to_string()),
                ("msg", target.message.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
            ],
        ),
    }
}
pub(crate) fn render_message(name: &str, fields: &[(&str, String)]) -> String {
    let values = fields
        .iter()
        .map(|(key, value)| format!("{}={}", key, encode_query_value(value)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", name, values)
}
pub(crate) fn render_message_owned(name: &str, fields: Vec<(String, String)>) -> String {
    let values = fields
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, encode_query_value(&value)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", name, values)
}
pub(crate) fn client_update_fields(
    before: &OnlineClientSnapshot,
    after: &OnlineClientSnapshot,
) -> Vec<(String, String)> {
    let mut fields = vec![(String::from("clid"), after.id.to_string())];

    if before.nickname != after.nickname {
        fields.push((String::from("client_nickname"), after.nickname.clone()));
    }
    if before.away != after.away {
        fields.push((
            String::from("client_away"),
            if after.away {
                String::from("1")
            } else {
                String::from("0")
            },
        ));
    }
    if before.away_message != after.away_message {
        fields.push((
            String::from("client_away_message"),
            after.away_message.clone(),
        ));
    }
    if before.input_muted != after.input_muted {
        fields.push((
            String::from("client_input_muted"),
            if after.input_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        ));
    }
    if before.output_muted != after.output_muted {
        fields.push((
            String::from("client_output_muted"),
            if after.output_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        ));
    }

    fields
}
pub(crate) fn server_edited_fields(
    before: &ServerSnapshot,
    after: &ServerSnapshot,
    invoker_id: u64,
    invoker_name: &str,
) -> Vec<(String, String)> {
    let mut fields = vec![(String::from("virtualserver_id"), after.id.to_string())];

    if before.name != after.name {
        fields.push((String::from("virtualserver_name"), after.name.clone()));
    }
    if before.welcome_message != after.welcome_message {
        fields.push((
            String::from("virtualserver_welcomemessage"),
            after.welcome_message.clone(),
        ));
    }
    if before.host_message != after.host_message {
        fields.push((
            String::from("virtualserver_hostmessage"),
            after.host_message.clone(),
        ));
    }
    if before.host_message_mode != after.host_message_mode {
        fields.push((
            String::from("virtualserver_hostmessage_mode"),
            after.host_message_mode.to_string(),
        ));
    }
    if before.ask_for_privilegekey != after.ask_for_privilegekey {
        fields.push((
            String::from("virtualserver_ask_for_privilegekey"),
            after.ask_for_privilegekey.to_string(),
        ));
    }
    if before.max_clients != after.max_clients {
        fields.push((
            String::from("virtualserver_maxclients"),
            after.max_clients.to_string(),
        ));
    }
    if before.antiflood_points_tick_reduce != after.antiflood_points_tick_reduce {
        fields.push((
            String::from("virtualserver_antiflood_points_tick_reduce"),
            after.antiflood_points_tick_reduce.to_string(),
        ));
    }
    if before.antiflood_points_needed_command_block != after.antiflood_points_needed_command_block {
        fields.push((
            String::from("virtualserver_antiflood_points_needed_command_block"),
            after.antiflood_points_needed_command_block.to_string(),
        ));
    }
    if before.antiflood_points_needed_ip_block != after.antiflood_points_needed_ip_block {
        fields.push((
            String::from("virtualserver_antiflood_points_needed_ip_block"),
            after.antiflood_points_needed_ip_block.to_string(),
        ));
    }
    if before.antiflood_ban_time != after.antiflood_ban_time {
        fields.push((
            String::from("virtualserver_antiflood_ban_time"),
            after.antiflood_ban_time.to_string(),
        ));
    }

    fields.push((String::from("invokerid"), invoker_id.to_string()));
    fields.push((String::from("invokername"), invoker_name.to_string()));
    fields.push((String::from("invokeruid"), format!("query-{}", invoker_id)));
    fields
}
