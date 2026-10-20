use crate::query::*;
use crate::runtime::*;
use super::*;
pub(crate) fn matches_channel_tree_subscription(
    subscription_channel_id: Option<u32>,
    channel: &ChannelSnapshot,
    previous_parent_id: Option<u32>,
) -> bool {
    subscription_channel_id.is_some_and(|channel_id| {
        channel_id == 0
            || channel_id == channel.id
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
        } => {
            let mut fields = vec![
                (String::from("cfid"), from_channel_id.to_string()),
                (String::from("ctid"), presence.channel_id.to_string()),
                (String::from("reasonid"), reason_id.to_string()),
            ];
            if *reason_id != 0 {
                if !reason_message.is_empty() || *reason_id == 4 {
                    fields.push((String::from("reasonmsg"), reason_message.clone()));
                }
                if *invoker_id > 0 || !invoker_name.is_empty() {
                    fields.push((String::from("invokerid"), invoker_id.to_string()));
                    fields.push((String::from("invokername"), invoker_name.clone()));
                    fields.push((String::from("invokeruid"), invoker_uid.clone()));
                }
            }
            fields.push((String::from("clid"), presence.client_id.to_string()));
            render_message_owned("notifyclientmoved", fields)
        }
        TransportNotification::ClientLeftView {
            presence,
            to_channel_id,
            reason_id,
            reason_message,
            invoker_id,
            invoker_name,
            invoker_uid,
            ban_time,
        } => {
            let mut fields = vec![
                (String::from("cfid"), presence.channel_id.to_string()),
                (String::from("ctid"), to_channel_id.unwrap_or(0).to_string()),
                (String::from("reasonid"), reason_id.to_string()),
            ];
            if *invoker_id > 0
                || !invoker_name.is_empty()
                || matches!(*reason_id, 1 | 4 | 5 | 6 | 11)
            {
                fields.push((String::from("invokerid"), invoker_id.to_string()));
                fields.push((String::from("invokername"), invoker_name.clone()));
                fields.push((String::from("invokeruid"), invoker_uid.clone()));
            }
            fields.push((String::from("reasonmsg"), reason_message.clone()));
            if let Some(bt) = ban_time {
                fields.push((String::from("bantime"), bt.to_string()));
            } else if *reason_id == 6 {
                fields.push((String::from("bantime"), String::from("0")));
            }
            fields.push((String::from("clid"), presence.client_id.to_string()));
            render_message_owned("notifyclientleftview", fields)
        }
        TransportNotification::ChannelEdited {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychanneledited",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("pid", channel.parent_id.to_string()),
                ("channel_order", channel.order.to_string()),
                ("channel_name", channel.name.clone()),
                ("channel_topic", channel.topic.clone()),
                ("channel_description", channel.description.clone()),
                ("channel_codec", channel.codec.to_string()),
                ("channel_codec_quality", channel.codec_quality.to_string()),
                ("channel_maxclients", channel.maxclients.to_string()),
                (
                    "channel_maxfamilyclients",
                    channel.maxfamilyclients.to_string(),
                ),
                (
                    "channel_flag_permanent",
                    if channel.kind.is_permanent() {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_semi_permanent",
                    if channel.kind.is_semi_permanent() {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_default",
                    if channel.flag_default {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_password",
                    if channel.flag_password {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_maxclients_unlimited",
                    if channel.maxclients < 0 {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_maxfamilyclients_unlimited",
                    if channel.maxfamilyclients < 0 {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_needed_talk_power",
                    channel.needed_talk_power.to_string(),
                ),
                (
                    "i_channel_needed_join_power",
                    channel.needed_join_power.to_string(),
                ),
                (
                    "i_channel_needed_subscribe_power",
                    channel.needed_subscribe_power.to_string(),
                ),
                (
                    "i_channel_needed_description_view_power",
                    channel.needed_description_view_power.to_string(),
                ),
                (
                    "i_channel_needed_modify_power",
                    channel.needed_modify_power.to_string(),
                ),
                (
                    "i_channel_needed_delete_power",
                    channel.needed_delete_power.to_string(),
                ),
                ("channel_icon_id", channel.icon_id.to_string()),
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
                ("pid", channel.parent_id.to_string()),
                ("channel_order", channel.order.to_string()),
                ("channel_name", channel.name.clone()),
                ("channel_topic", channel.topic.clone()),
                ("channel_description", channel.description.clone()),
                ("channel_codec", channel.codec.to_string()),
                ("channel_codec_quality", channel.codec_quality.to_string()),
                ("channel_maxclients", channel.maxclients.to_string()),
                (
                    "channel_maxfamilyclients",
                    channel.maxfamilyclients.to_string(),
                ),
                (
                    "channel_flag_permanent",
                    if channel.kind.is_permanent() {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_semi_permanent",
                    if channel.kind.is_semi_permanent() {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_default",
                    if channel.flag_default {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_password",
                    if channel.flag_password {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_maxclients_unlimited",
                    if channel.maxclients < 0 {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_flag_maxfamilyclients_unlimited",
                    if channel.maxfamilyclients < 0 {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                ),
                (
                    "channel_needed_talk_power",
                    channel.needed_talk_power.to_string(),
                ),
                (
                    "i_channel_needed_join_power",
                    channel.needed_join_power.to_string(),
                ),
                (
                    "i_channel_needed_subscribe_power",
                    channel.needed_subscribe_power.to_string(),
                ),
                (
                    "i_channel_needed_description_view_power",
                    channel.needed_description_view_power.to_string(),
                ),
                (
                    "i_channel_needed_modify_power",
                    channel.needed_modify_power.to_string(),
                ),
                (
                    "i_channel_needed_delete_power",
                    channel.needed_delete_power.to_string(),
                ),
                ("channel_icon_id", channel.icon_id.to_string()),
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
                ("pid", channel.parent_id.to_string()),
                ("order", channel.order.to_string()),
                ("channel_order", channel.order.to_string()),
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
        TransportNotification::Ts6StreamNotify { command, .. } => command.clone(),
        TransportNotification::VideoBroadcastChanged { channel_id, .. } => {
            render_message("notifybroadcastvideo", &[("cid", channel_id.to_string())])
        }
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
    if before.server_groups != after.server_groups {
        fields.push((
            String::from("client_servergroups"),
            after
                .server_groups
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ));
    }
    if before.version != after.version {
        fields.push((String::from("client_version"), after.version.clone()));
    }
    if before.platform != after.platform {
        fields.push((String::from("client_platform"), after.platform.clone()));
    }
    if before.country != after.country {
        fields.push((String::from("client_country"), after.country.clone()));
    }
    for key in [
        "client_outputonly_muted",
        "client_description",
        "client_is_talker",
        "client_talk_power",
        "client_talk_request",
        "client_talk_request_msg",
        "client_is_channel_commander",
        "client_is_priority_speaker",
        "client_is_recording",
        "client_input_hardware",
        "client_output_hardware",
        "client_input_deactivated",
        "client_nickname_phonetic",
        "client_meta_data",
        "client_flag_avatar",
        "client_badges",
        "client_is_streaming",
        "client_flag_video_broadcasting",
        "client_flag_screen_broadcasting",
        "player_state",
        "player_volume",
        "client_playlist_id",
        "client_disabled",
        "client_flag_notify_song_change",
        "client_bot_type",
        "client_uptime_mode",
    ] {
        let before_val = before.extra_properties.get(key);
        let after_val = after.extra_properties.get(key);
        if before_val != after_val
            && let Some(val) = after_val
        {
            fields.push((String::from(key), val.clone()));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::SessionPresence;

    fn sample_presence(clid: u64, cid: u32) -> SessionPresence {
        SessionPresence {
            client_id: clid,
            login_name: String::from("TestUser"),
            unique_identifier: String::from("uid-test"),
            client_type: 0,
            server_id: 1,
            channel_id: cid,
        }
    }

    #[test]
    fn self_move_omits_invoker_fields_while_admin_move_includes_them() {
        let self_move = TransportNotification::ClientMoved {
            presence: sample_presence(5, 2),
            from_channel_id: 1,
            reason_id: 0,
            reason_message: String::new(),
            invoker_id: 5,
            invoker_name: String::from("TestUser"),
            invoker_uid: String::from("uid-test"),
        };
        assert_eq!(
            render_notification(&self_move),
            "notifyclientmoved cfid=1 ctid=2 reasonid=0 clid=5"
        );

        let admin_move = TransportNotification::ClientMoved {
            presence: sample_presence(5, 3),
            from_channel_id: 2,
            reason_id: 1,
            reason_message: String::new(),
            invoker_id: 1,
            invoker_name: String::from("Admin"),
            invoker_uid: String::from("uid-admin"),
        };
        assert_eq!(
            render_notification(&admin_move),
            "notifyclientmoved cfid=2 ctid=3 reasonid=1 invokerid=1 invokername=Admin invokeruid=uid-admin clid=5"
        );
    }

    #[test]
    fn left_view_encodings_match_ts3_and_teaspeak_expectations() {
        let voluntary = TransportNotification::ClientLeftView {
            presence: sample_presence(7, 1),
            to_channel_id: None,
            reason_id: 8,
            reason_message: String::from("leaving"),
            invoker_id: 0,
            invoker_name: String::new(),
            invoker_uid: String::new(),
            ban_time: None,
        };
        assert_eq!(
            render_notification(&voluntary),
            "notifyclientleftview cfid=1 ctid=0 reasonid=8 reasonmsg=leaving clid=7"
        );

        let kicked = TransportNotification::ClientLeftView {
            presence: sample_presence(7, 1),
            to_channel_id: None,
            reason_id: 5,
            reason_message: String::from("Bye"),
            invoker_id: 2,
            invoker_name: String::from("Mod"),
            invoker_uid: String::from("uid-mod"),
            ban_time: None,
        };
        assert_eq!(
            render_notification(&kicked),
            "notifyclientleftview cfid=1 ctid=0 reasonid=5 invokerid=2 invokername=Mod invokeruid=uid-mod reasonmsg=Bye clid=7"
        );

        let banned = TransportNotification::ClientLeftView {
            presence: sample_presence(7, 1),
            to_channel_id: None,
            reason_id: 6,
            reason_message: String::from("Rule violation"),
            invoker_id: 2,
            invoker_name: String::from("Mod"),
            invoker_uid: String::from("uid-mod"),
            ban_time: Some(3600),
        };
        assert_eq!(
            render_notification(&banned),
            "notifyclientleftview cfid=1 ctid=0 reasonid=6 invokerid=2 invokername=Mod invokeruid=uid-mod reasonmsg=Rule\\sviolation bantime=3600 clid=7"
        );
    }
}

