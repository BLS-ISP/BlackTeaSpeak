use super::wire_formatting::*;
use crate::runtime::{Channel, ChannelKind};
use crate::teaspeak_transport::UdpSession;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;

pub(crate) async fn handle_channel_and_runtime_command(
    payload_str: &str,
    session: &mut UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
    runtime: &Arc<Mutex<crate::runtime::BaselineRuntime>>,
    server_id: u32,
) -> bool {
    // Handle clientdisconnect cleanly
    if payload_str.starts_with("clientdisconnect") {
        let clid = session.client_id;
        let rc = get_return_code(payload_str);
        let reason_id = parse_argument_u32(payload_str, "reasonid").unwrap_or(8);
        let reason_msg = parse_argument(payload_str, "reasonmsg").unwrap_or_default();
        let mut cfid = 1u32;
        {
            let mut rt = runtime.lock().unwrap();
            if let Some(existing) = rt.store.online_clients.get(&clid) {
                cfid = existing.channel_id;
            }
            rt.remove_session_client(clid, reason_id, reason_msg.clone());
        }
        let left_cmd = format!(
            "notifyclientleftview cfid={} ctid=0 reasonid={} reasonmsg={} clid={}",
            cfid,
            reason_id,
            ts3_escape(&reason_msg),
            clid
        );
        send_command(&left_cmd, session, addr, socket, iv_struct).await;
        if let Some(rc_val) = rc {
            let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
        } else {
            send_command("error id=0 msg=ok", session, addr, socket, iv_struct).await;
        }
        session
            .is_active
            .store(false, std::sync::atomic::Ordering::SeqCst);
        return true;
    }

    if payload_str.starts_with("channelcreate") {
        let rc = get_return_code(payload_str);
        let parent_id = parse_argument_u32(payload_str, "cpid").unwrap_or(0);
        let channel_name = parse_argument(payload_str, "channel_name")
            .unwrap_or_else(|| "New Channel".to_string());
        let channel_topic = parse_argument(payload_str, "channel_topic").unwrap_or_default();
        let channel_description =
            parse_argument(payload_str, "channel_description").unwrap_or_default();
        let flag_perm = parse_argument_u32(payload_str, "channel_flag_permanent").unwrap_or(0) != 0;
        let flag_semi =
            parse_argument_u32(payload_str, "channel_flag_semi_permanent").unwrap_or(0) != 0;
        let channel_kind = ChannelKind::from_flags(flag_perm, flag_semi);
        let codec = parse_argument_u32(payload_str, "channel_codec").unwrap_or(4);
        let codec_quality = parse_argument_u32(payload_str, "channel_codec_quality").unwrap_or(6);
        let maxclients = parse_argument(payload_str, "channel_maxclients")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(-1);
        let maxfamilyclients = parse_argument(payload_str, "channel_maxfamilyclients")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(-1);

        let perm_err = {
            let rt = runtime.lock().unwrap();
            let qsession = build_ts3_query_session(session, addr, server_id, &rt);
            match rt.query_actor_effective_permissions(&qsession) {
                Ok((_actor, actor_permissions)) => {
                    let create_perm = match channel_kind {
                        ChannelKind::Permanent => "b_channel_create_permanent",
                        ChannelKind::SemiPermanent => "b_channel_create_semi_permanent",
                        ChannelKind::Temporary => "b_channel_create_temporary",
                    };
                    if let Err(p) = crate::runtime::permissions::check_required_permission(
                        &actor_permissions,
                        &[create_perm],
                        create_perm,
                    ) {
                        Some(rt.insufficient_permission_response(p))
                    } else if parent_id != 0
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_create_child"],
                            "b_channel_create_child",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if !channel_topic.is_empty()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_create_with_topic"],
                            "b_channel_create_with_topic",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if !channel_description.is_empty()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_create_with_description"],
                            "b_channel_create_with_description",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if !parse_argument(payload_str, "channel_password")
                        .unwrap_or_default()
                        .is_empty()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_create_with_password"],
                            "b_channel_create_with_password",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument_u32(payload_str, "channel_flag_default").unwrap_or(0)
                        != 0
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_create_with_default"],
                            "b_channel_create_with_default",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument(payload_str, "channel_needed_talk_power")
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(0)
                        != 0
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_create_with_needed_talk_power"],
                            "b_channel_create_with_needed_talk_power",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else {
                        None
                    }
                }
                Err(resp) => Some(resp),
            }
        };
        if let Some(err_resp) = perm_err {
            let ack_cmd = render_ts3_error_ack(&err_resp, rc.as_deref());
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            return true;
        }

        let mut created_channel = None;

        {
            let mut rt = runtime.lock().unwrap();
            if let Some(channels) = rt.store.channels.get_mut(&server_id) {
                let channel_id = channels
                    .iter()
                    .map(|c| c.id)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);

                let channel_password =
                    parse_argument(payload_str, "channel_password").unwrap_or_default();
                let channel_order = parse_argument_u32(payload_str, "channel_order").unwrap_or(0);
                let flag_default =
                    parse_argument_u32(payload_str, "channel_flag_default").unwrap_or(0) != 0;
                let needed_talk_power = parse_argument(payload_str, "channel_needed_talk_power")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let mut channel_perms = BTreeMap::new();
                if needed_talk_power != 0 {
                    channel_perms.insert(
                        String::from("i_channel_needed_talk_power"),
                        crate::runtime::PermissionAssignment {
                            value: needed_talk_power,
                            skipped: false,
                            negated: false,
                        },
                    );
                }
                if flag_default {
                    for existing in channels.iter_mut() {
                        existing.flag_default = false;
                    }
                }

                let new_channel = Channel {
                    id: channel_id,
                    parent_id,
                    order: channel_order,
                    kind: channel_kind,
                    name: channel_name.clone(),
                    topic: channel_topic.clone(),
                    description: channel_description.clone(),
                    password: channel_password.clone(),
                    codec,
                    codec_quality,
                    maxclients,
                    maxfamilyclients,
                    flag_default,
                    flag_password: !channel_password.is_empty(),
                    permissions: channel_perms,
                };

                channels.push(new_channel.clone());
                let _ = rt.db.save_channel(server_id, &new_channel);
                created_channel = Some(new_channel);
            }
        }

        if let Some(channel) = created_channel {
            let needed_talk = channel
                .permissions
                .get("i_channel_needed_talk_power")
                .map(|p| p.value)
                .unwrap_or(0);
            let notify_cmd = format!(
                "notifychannelcreated cid={} cpid={} channel_name={} channel_topic={} channel_description={} channel_password= channel_codec={} channel_codec_quality={} channel_maxclients={} channel_maxfamilyclients={} channel_order={} channel_flag_permanent={} channel_flag_semi_permanent={} channel_flag_default={} channel_flag_password={} channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power={} channel_forced_silence=0 channel_name_phonetic= channel_icon_id=0 channel_flag_private=0 invokerid={} invokername={} invokeruid={}",
                channel.id,
                channel.parent_id,
                ts3_escape(&channel.name),
                ts3_escape(&channel.topic),
                ts3_escape(&channel.description),
                channel.codec,
                channel.codec_quality,
                channel.maxclients,
                channel.maxfamilyclients,
                channel.order,
                if channel.kind == ChannelKind::Permanent { 1 } else { 0 },
                if channel.kind == ChannelKind::SemiPermanent { 1 } else { 0 },
                if channel.flag_default { 1 } else { 0 },
                if channel.flag_password { 1 } else { 0 },
                needed_talk,
                session.client_id,
                ts3_escape(&session.client_nickname),
                ts3_escape(&session.client_uid)
            );
            send_command(&notify_cmd, session, addr, socket, iv_struct).await;

            if let Some(rc_val) = &rc {
                let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            }

            let rt = runtime.lock().unwrap();
            let snapshot = crate::runtime::ChannelSnapshot::from_channel(&channel, 0);
            let notif = crate::transport::TransportNotification::ChannelCreated {
                server_id,
                channel: snapshot,
                invoker_id: session.client_id,
                invoker_name: session.client_nickname.clone(),
            };
            rt.broadcast_event(server_id, &notif);
        } else if let Some(rc_val) = &rc {
            let ack_cmd = format!(
                "error id=768 msg=could\\snot\\screate\\schannel return_code={}",
                rc_val
            );
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    if payload_str.starts_with("channeldelete") {
        let rc = get_return_code(payload_str);
        let clid = session.client_id;
        let cid = parse_argument_u32(payload_str, "cid").unwrap_or(0);

        let perm_err = {
            let rt = runtime.lock().unwrap();
            let qsession = build_ts3_query_session(session, addr, server_id, &rt);
            match rt.query_actor_effective_permissions(&qsession) {
                Ok((_actor, actor_permissions)) => {
                    let target_perms = rt
                        .store
                        .channels
                        .get(&server_id)
                        .and_then(|chs| chs.iter().find(|c| c.id == cid))
                        .map(|c| c.permissions.clone())
                        .unwrap_or_default();
                    if let Err(p) = crate::runtime::permissions::check_required_permission(
                        &actor_permissions,
                        &["b_channel_delete_permanent"],
                        "b_channel_delete_permanent",
                    ) {
                        Some(rt.insufficient_permission_response(p))
                    } else if let Err(p) =
                        crate::runtime::permissions::check_channel_delete_power_allowed(
                            &actor_permissions,
                            &target_perms,
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else {
                        None
                    }
                }
                Err(resp) => Some(resp),
            }
        };
        if let Some(err_resp) = perm_err {
            let ack_cmd = render_ts3_error_ack(&err_resp, rc.as_deref());
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            return true;
        }

        let mut deleted_channel = None;
        let default_cid = {
            let mut rt = runtime.lock().unwrap();
            let def = rt.default_channel_id_for_server(server_id).unwrap_or(1);
            if let Some(channels) = rt.store.channels.get_mut(&server_id) {
                if let Some(idx) = channels.iter().position(|c| c.id == cid) {
                    let channel = channels.remove(idx);
                    let _ = rt.db.delete_channel(server_id, cid);
                    deleted_channel = Some(channel);
                }
            }
            def
        };

        if let Some(channel) = deleted_channel {
            {
                let mut rt = runtime.lock().unwrap();
                if let Some(client) = rt.store.online_clients.get_mut(&clid) {
                    if client.channel_id == cid {
                        client.channel_id = default_cid;

                        let notify_move = format!(
                            "notifyclientmoved clid={} ctid={} cfid={} reasonid=0",
                            clid, default_cid, cid
                        );
                        send_command(&notify_move, session, addr, socket, iv_struct).await;

                        let presence = crate::transport::SessionPresence {
                            client_id: clid,
                            login_name: session.client_nickname.clone(),
                            unique_identifier: session.client_uid.clone(),
                            client_type: 0,
                            server_id,
                            channel_id: default_cid,
                        };
                        let notif_move = crate::transport::TransportNotification::ClientMoved {
                            presence,
                            from_channel_id: cid,
                            reason_id: 0,
                            reason_message: String::new(),
                            invoker_id: clid,
                            invoker_name: session.client_nickname.clone(),
                            invoker_uid: session.client_uid.clone(),
                        };
                        rt.broadcast_event(server_id, &notif_move);
                    }
                }
            }

            let notify_cmd = format!(
                "notifychanneldeleted cid={} invokerid={} invokername={} invokeruid={}",
                cid,
                clid,
                ts3_escape(&session.client_nickname),
                ts3_escape(&session.client_uid)
            );
            send_command(&notify_cmd, session, addr, socket, iv_struct).await;

            if let Some(rc_val) = &rc {
                let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            }

            let rt = runtime.lock().unwrap();
            let snapshot = crate::runtime::ChannelSnapshot::from_channel(&channel, 0);
            let notif = crate::transport::TransportNotification::ChannelDeleted {
                server_id,
                channel: snapshot,
                invoker_id: clid,
                invoker_name: session.client_nickname.clone(),
            };
            rt.broadcast_event(server_id, &notif);
        } else if let Some(rc_val) = &rc {
            let ack_cmd = format!(
                "error id=768 msg=channel\\snot\\sfound return_code={}",
                rc_val
            );
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    if payload_str.starts_with("channeledit") {
        let rc = get_return_code(payload_str);
        let cid = parse_argument_u32(payload_str, "cid").unwrap_or(0);

        let perm_err = {
            let rt = runtime.lock().unwrap();
            let qsession = build_ts3_query_session(session, addr, server_id, &rt);
            match rt.query_actor_effective_permissions(&qsession) {
                Ok((_actor, actor_permissions)) => {
                    let target_perms = rt
                        .store
                        .channels
                        .get(&server_id)
                        .and_then(|chs| chs.iter().find(|c| c.id == cid))
                        .map(|c| c.permissions.clone())
                        .unwrap_or_default();
                    if let Err(p) =
                        crate::runtime::permissions::check_channel_modify_power_allowed(
                            &actor_permissions,
                            &target_perms,
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument(payload_str, "channel_name").is_some()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_modify_name"],
                            "b_channel_modify_name",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument(payload_str, "channel_topic").is_some()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_modify_topic"],
                            "b_channel_modify_topic",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument(payload_str, "channel_description").is_some()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_modify_description"],
                            "b_channel_modify_description",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument(payload_str, "channel_password").is_some()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_modify_password"],
                            "b_channel_modify_password",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else if parse_argument(payload_str, "channel_needed_talk_power").is_some()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_modify_needed_talk_power"],
                            "b_channel_modify_needed_talk_power",
                        )
                    {
                        Some(rt.insufficient_permission_response(p))
                    } else {
                        None
                    }
                }
                Err(resp) => Some(resp),
            }
        };
        if let Some(err_resp) = perm_err {
            let ack_cmd = render_ts3_error_ack(&err_resp, rc.as_deref());
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            return true;
        }

        let mut updated_channel = None;

        {
            let mut rt = runtime.lock().unwrap();
            if let Some(channels) = rt.store.channels.get_mut(&server_id) {
                let make_default =
                    parse_argument_u32(payload_str, "channel_flag_default").unwrap_or(0) != 0;
                if make_default {
                    for existing in channels.iter_mut() {
                        existing.flag_default = existing.id == cid;
                    }
                }
                if let Some(channel) = channels.iter_mut().find(|c| c.id == cid) {
                    if let Some(name) = parse_argument(payload_str, "channel_name") {
                        if !name.is_empty() {
                            channel.name = name;
                        }
                    }
                    if let Some(topic) = parse_argument(payload_str, "channel_topic") {
                        channel.topic = topic;
                    }
                    if let Some(desc) = parse_argument(payload_str, "channel_description") {
                        channel.description = desc;
                    }
                    if let Some(password) = parse_argument(payload_str, "channel_password") {
                        channel.flag_password = !password.is_empty();
                        channel.password = password;
                    }
                    if let Some(order) = parse_argument_u32(payload_str, "channel_order") {
                        channel.order = order;
                    }
                    if let Some(needed_talk) =
                        parse_argument(payload_str, "channel_needed_talk_power")
                            .and_then(|v| v.parse::<i64>().ok())
                    {
                        channel.permissions.insert(
                            String::from("i_channel_needed_talk_power"),
                            crate::runtime::PermissionAssignment {
                                value: needed_talk,
                                skipped: false,
                                negated: false,
                            },
                        );
                    }
                    if let Some(flag_perm) = parse_argument_u32(payload_str, "channel_flag_permanent")
                    {
                        let flag_semi =
                            parse_argument_u32(payload_str, "channel_flag_semi_permanent")
                                .unwrap_or(0)
                                != 0;
                        channel.kind = ChannelKind::from_flags(flag_perm != 0, flag_semi);
                    } else if let Some(flag_semi) =
                        parse_argument_u32(payload_str, "channel_flag_semi_permanent")
                    {
                        let flag_perm = channel.kind == ChannelKind::Permanent;
                        channel.kind = ChannelKind::from_flags(flag_perm, flag_semi != 0);
                    }
                    if let Some(codec) = parse_argument_u32(payload_str, "channel_codec") {
                        channel.codec = codec;
                    }
                    if let Some(codec_quality) =
                        parse_argument_u32(payload_str, "channel_codec_quality")
                    {
                        channel.codec_quality = codec_quality;
                    }
                    if let Some(maxclients) = parse_argument(payload_str, "channel_maxclients")
                        .and_then(|v| v.parse::<i32>().ok())
                    {
                        channel.maxclients = maxclients;
                    }
                    if let Some(maxfamilyclients) =
                        parse_argument(payload_str, "channel_maxfamilyclients")
                            .and_then(|v| v.parse::<i32>().ok())
                    {
                        channel.maxfamilyclients = maxfamilyclients;
                    }

                    updated_channel = Some(channel.clone());
                }
            }
            if let Some(ref ch) = updated_channel {
                let _ = rt.db.save_channel(server_id, ch);
            }
        }

        if let Some(channel) = updated_channel {
            let needed_talk = channel
                .permissions
                .get("i_channel_needed_talk_power")
                .map(|p| p.value)
                .unwrap_or(0);
            let notify_cmd = format!(
                "notifychanneledited cid={} reasonid=10 invokerid={} invokername={} invokeruid={} channel_name={} channel_topic={} channel_description={} channel_codec={} channel_codec_quality={} channel_maxclients={} channel_maxfamilyclients={} channel_order={} channel_flag_permanent={} channel_flag_semi_permanent={} channel_flag_default={} channel_flag_password={} channel_needed_talk_power={}",
                cid,
                session.client_id,
                ts3_escape(&session.client_nickname),
                ts3_escape(&session.client_uid),
                ts3_escape(&channel.name),
                ts3_escape(&channel.topic),
                ts3_escape(&channel.description),
                channel.codec,
                channel.codec_quality,
                channel.maxclients,
                channel.maxfamilyclients,
                channel.order,
                if channel.kind == ChannelKind::Permanent { 1 } else { 0 },
                if channel.kind == ChannelKind::SemiPermanent { 1 } else { 0 },
                if channel.flag_default { 1 } else { 0 },
                if channel.flag_password { 1 } else { 0 },
                needed_talk
            );
            send_command(&notify_cmd, session, addr, socket, iv_struct).await;

            if let Some(rc_val) = &rc {
                let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            }

            let rt = runtime.lock().unwrap();
            let snapshot = crate::runtime::ChannelSnapshot::from_channel(&channel, 0);
            let notif = crate::transport::TransportNotification::ChannelEdited {
                server_id,
                channel: snapshot,
                description_changed: true,
                invoker_id: session.client_id,
                invoker_name: session.client_nickname.clone(),
            };
            rt.broadcast_event(server_id, &notif);
        } else if let Some(rc_val) = &rc {
            let ack_cmd = format!(
                "error id=768 msg=channel\\snot\\sfound return_code={}",
                rc_val
            );
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    if payload_str.starts_with("clientupdate") {
        let clid = session.client_id;
        let mut nick_updated = false;
        let mut nickname_val = String::new();

        let before_snap = {
            let rt = runtime.lock().unwrap();
            rt.online_client_snapshot(server_id, clid)
        };

        {
            let mut rt = runtime.lock().unwrap();
            if let Some(client) = rt.store.online_clients.get_mut(&clid) {
                if let Some(nick) = parse_argument(payload_str, "client_nickname") {
                    if !nick.is_empty() {
                        client.nickname = nick.clone();
                        nickname_val = nick;
                        nick_updated = true;
                    }
                }
                if let Some(away) = parse_argument_u32(payload_str, "client_away") {
                    client.away = away != 0;
                }
                if let Some(away_msg) = parse_argument(payload_str, "client_away_message") {
                    client.away_message = away_msg;
                }
                if let Some(muted) = parse_argument_u32(payload_str, "client_input_muted") {
                    client.input_muted = muted != 0;
                }
                if let Some(muted) = parse_argument_u32(payload_str, "client_output_muted") {
                    client.output_muted = muted != 0;
                }
                for key in [
                    "client_outputonly_muted",
                    "client_talk_request",
                    "client_talk_request_msg",
                    "client_is_channel_commander",
                    "client_is_recording",
                    "client_description",
                    "client_input_hardware",
                    "client_output_hardware",
                    "client_input_deactivated",
                    "client_nickname_phonetic",
                    "client_meta_data",
                    "client_flag_avatar",
                    "client_badges",
                ] {
                    if let Some(val) = parse_argument(payload_str, key) {
                        client.extra_properties.insert(key.to_string(), val.clone());
                        if key == "client_talk_request" && val == "0" && parse_argument(payload_str, "client_talk_request_msg").is_none() {
                            client.extra_properties.insert(String::from("client_talk_request_msg"), String::new());
                        }
                    }
                }
            }
        }

        if nick_updated {
            session.client_nickname = nickname_val;
        }

        let after_snap = {
            let rt = runtime.lock().unwrap();
            rt.online_client_snapshot(server_id, clid)
        };

        // In Official TS3 (pv=6), ONLY send error id=0 msg=ok if return_code was requested!
        if let Some(rc_val) = get_return_code(payload_str) {
            let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }

        if let (Some(before), Some(after)) = (before_snap, after_snap)
            && crate::transport::server::client_update_fields(&before, &after).len() > 1
        {
            let rt = runtime.lock().unwrap();
            let notif = crate::transport::TransportNotification::ClientUpdated {
                server_id,
                before,
                after,
            };
            rt.broadcast_event(server_id, &notif);
        }
        return true;
    }

    if payload_str.starts_with("channelgetdescription") {
        let cid = parse_argument_u32(payload_str, "cid").unwrap_or(1);
        let desc = {
            let rt = runtime.lock().unwrap();
            rt.channel_by_id(server_id, cid)
                .map(|c| c.description.clone())
                .unwrap_or_default()
        };
        let notify_cmd = format!(
            "notifychanneledited cid={} reasonid=9 channel_description={}",
            cid,
            ts3_escape(&desc)
        );
        send_command(&notify_cmd, session, addr, socket, iv_struct).await;
        if let Some(rc_val) = get_return_code(payload_str) {
            let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 3. Handle Official TS3 variable & subscription commands
    let is_ts3_var_or_sub_cmd = payload_str.starts_with("servergetvariables")
        || payload_str.starts_with("connectioninfoautoupdate")
        || payload_str.starts_with("clientgetvariables")
        || payload_str.starts_with("channelsubscribeball");

    if is_ts3_var_or_sub_cmd {
        // In Official TS3, ONLY send error id=0 msg=ok if return_code (or rt-) was requested!
        if let Some(rc_val) = get_return_code(payload_str) {
            let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;

            if payload_str.starts_with("clientgetvariables") {
                let target_clid = parse_argument_u64(payload_str, "clid")
                    .filter(|&id| id != 0)
                    .unwrap_or(session.client_id);
                let notify_cmd = {
                    let rt = runtime.lock().unwrap();
                    if let Some(online) = rt.store.online_clients.get(&target_clid) {
                        let sg_str = if online.server_groups.is_empty() {
                            String::from("8")
                        } else {
                            online
                                .server_groups
                                .iter()
                                .map(u32::to_string)
                                .collect::<Vec<_>>()
                                .join(",")
                        };
                        let cg_id = rt.effective_channel_group_id_for_client(
                            server_id,
                            online.channel_id,
                            online.database_id,
                        );
                        let talk_power = rt.client_talk_power(
                            server_id,
                            online.channel_id,
                            online.database_id,
                        );
                        let outonly = online
                            .extra_properties
                            .get("client_outputonly_muted")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let in_hw = online
                            .extra_properties
                            .get("client_input_hardware")
                            .map(String::as_str)
                            .unwrap_or("1");
                        let out_hw = online
                            .extra_properties
                            .get("client_output_hardware")
                            .map(String::as_str)
                            .unwrap_or("1");
                        let rec = online
                            .extra_properties
                            .get("client_is_recording")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let cc = online
                            .extra_properties
                            .get("client_is_channel_commander")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let talker = online
                            .extra_properties
                            .get("client_is_talker")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let desc = online
                            .extra_properties
                            .get("client_description")
                            .map(String::as_str)
                            .unwrap_or("");
                        format!(
                            "notifyclientupdated clid={} client_unique_identifier={} client_nickname={} client_database_id={} client_channel_group_id={} client_servergroups={} client_country={} client_talk_power={} client_input_muted={} client_output_muted={} client_outputonly_muted={} client_input_hardware={} client_output_hardware={} client_is_recording={} client_is_channel_commander={} client_is_talker={} client_is_priority_speaker=0 client_away={} client_away_message={} client_description={} client_type={}",
                            target_clid,
                            ts3_escape(&online.unique_identifier),
                            ts3_escape(&online.nickname),
                            online.database_id,
                            cg_id,
                            sg_str,
                            ts3_escape(&online.country),
                            talk_power,
                            if online.input_muted { 1 } else { 0 },
                            if online.output_muted { 1 } else { 0 },
                            ts3_escape(outonly),
                            ts3_escape(in_hw),
                            ts3_escape(out_hw),
                            ts3_escape(rec),
                            ts3_escape(cc),
                            ts3_escape(talker),
                            if online.away { 1 } else { 0 },
                            ts3_escape(&online.away_message),
                            ts3_escape(desc),
                            online.client_type
                        )
                    } else {
                        let nick = if session.client_nickname.is_empty() {
                            "BlackTeaUser"
                        } else {
                            &session.client_nickname
                        };
                        format!(
                            "notifyclientupdated clid={} client_unique_identifier={} client_nickname={} client_database_id={} client_channel_group_id=8 client_servergroups=8 client_country=DE client_talk_power=0 client_input_muted=0 client_output_muted=0 client_outputonly_muted=0 client_input_hardware=1 client_output_hardware=1 client_is_recording=0 client_is_channel_commander=0 client_is_priority_speaker=0 client_away=0 client_type=0",
                            target_clid,
                            ts3_escape(&session.client_uid),
                            ts3_escape(nick),
                            target_clid + 1000
                        )
                    }
                };
                send_command(&notify_cmd, session, addr, socket, iv_struct).await;
            } else if payload_str.starts_with("servergetvariables") {
                let server_uid_escaped = ts3_escape(&session.server_uid);
                let notify_cmd = format!(
                    "notifyserverupdated virtualserver_name=BlackTeaSpeak virtualserver_welcomemessage=Welcome! virtualserver_maxclients=32 virtualserver_clientsonline=1 virtualserver_channelsonline=1 virtualserver_created=1494921612 virtualserver_uptime=33245 virtualserver_unique_identifier={} virtualserver_platform=Windows virtualserver_version=3.5.6 virtualserver_status=online",
                    server_uid_escaped
                );
                send_command(&notify_cmd, session, addr, socket, iv_struct).await;
            }
        }
        return true;
    }

    // 4. Route any other TS3 commands that carry a return_code through BaselineRuntime
    if let Some(rc_val) = get_return_code(payload_str) {
        if let Ok(mut req) = crate::query::parse_request_line(payload_str) {
            strip_ts3_return_code(&mut req);
            let cmd_name = req.command.clone();

            let (resp, notifs, needed_perms_notify, permission_list_override) = {
                let mut rt = runtime.lock().unwrap();
                let qsession_before = build_ts3_query_session(session, addr, server_id, &rt);
                let mut qsession_after = qsession_before.clone();
                let (resp, notifs) = crate::transport::execute_request_with_notifications(
                    &mut rt,
                    &req,
                    &qsession_before,
                    &mut qsession_after,
                );

                let perm_list_override = if cmd_name == "permissionlist" && resp.error_id == 0 {
                    let permission_rows = rt.web_permission_rows();
                    let mut parts = Vec::new();
                    for row in &permission_rows {
                        if let (Some(permid), Some(permname), Some(permdesc)) =
                            (row.get("permid"), row.get("permname"), row.get("permdesc"))
                        {
                            parts.push(format!(
                                "permid={} permname={} permdesc={}",
                                permid,
                                ts3_escape(permname),
                                ts3_escape(permdesc)
                            ));
                        }
                    }
                    for row in &permission_rows {
                        if let Some(group_id_end) = row.get("group_id_end") {
                            parts.push(format!("group_id_end={}", group_id_end));
                        }
                    }
                    Some(format!("notifypermissionlist {}", parts.join("|")))
                } else {
                    None
                };

                let refresh_needed = resp.error_id == 0
                    && matches!(
                        cmd_name.as_str(),
                        "tokenuse"
                            | "privilegekeyuse"
                            | "permreset"
                            | "servergroupaddclient"
                            | "servergroupdelclient"
                            | "setclientchannelgroup"
                            | "servergroupaddperm"
                            | "servergroupdelperm"
                            | "channelgroupaddperm"
                            | "channelgroupdelperm"
                            | "clientaddperm"
                            | "clientdelperm"
                            | "channeladdperm"
                            | "channeldelperm"
                            | "channelclientaddperm"
                            | "channelclientdelperm"
                    );
                let needed_notify = if refresh_needed {
                    let current_cid = qsession_after.current_channel_id.unwrap_or(1);
                    let dbid = qsession_after
                        .actor_client_database_id_override
                        .unwrap_or(session.client_id + 1000);
                    let needed_rows = rt
                        .web_client_needed_permission_rows(server_id, current_cid, dbid)
                        .unwrap_or_default();
                    if needed_rows.is_empty() {
                        Some(String::from("notifyclientneededpermissions permid=1 permvalue=0"))
                    } else {
                        Some(format!(
                            "notifyclientneededpermissions {}",
                            render_ts3_rows_only(&needed_rows)
                        ))
                    }
                } else {
                    None
                };

                (resp, notifs, needed_notify, perm_list_override)
            };

            if resp.error_id == 259 || resp.error_id == 256 {
                let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                send_command(&ack_cmd, session, addr, socket, iv_struct).await;
                return true;
            }

            if let Some(notify_perm_cmd) = permission_list_override {
                send_command(&notify_perm_cmd, session, addr, socket, iv_struct).await;
            } else if !resp.rows.is_empty() {
                let notify_prefix = ts3_notify_command_name(&cmd_name);
                let rows_str = render_ts3_rows_only(&resp.rows);
                let notify_cmd = format!("{} {}", notify_prefix, rows_str);
                send_command(&notify_cmd, session, addr, socket, iv_struct).await;
            }

            for notif in &notifs {
                if matches!(
                    notif,
                    crate::transport::TransportNotification::ClientUpdated { .. }
                        | crate::transport::TransportNotification::ClientPoke { .. }
                        | crate::transport::TransportNotification::VideoBroadcastChanged { .. }
                ) {
                    continue;
                }
                let notif_str = crate::transport::render_notification(notif);
                send_command(&notif_str, session, addr, socket, iv_struct).await;
            }

            if let Some(needed_cmd) = needed_perms_notify {
                send_command(&needed_cmd, session, addr, socket, iv_struct).await;
            }

            let ack_cmd = render_ts3_error_ack(&resp, Some(&rc_val));
            send_command(&ack_cmd, session, addr, socket, iv_struct).await;
            return true;
        }
    }

    false
}
