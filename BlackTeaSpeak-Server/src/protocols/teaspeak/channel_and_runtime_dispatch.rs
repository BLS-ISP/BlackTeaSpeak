use super::wire_formatting::*;
use crate::runtime::{Channel, ChannelKind};
use crate::teaspeak_transport::UdpSession;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;

pub(crate) async fn handle_teaspeak_channel_and_runtime_command(
    payload_str: &str,
    session: &mut UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
    runtime: &Arc<Mutex<crate::runtime::BaselineRuntime>>,
    server_id: u32,
) -> bool {
    if payload_str.starts_with("clientdisconnect") {
        let clid = session.client_id;
        let rc = get_teaspeak_return_code(payload_str);
        let reason_id = parse_teaspeak_argument_u32(payload_str, "reasonid").unwrap_or(8);
        let reason_msg = parse_teaspeak_argument(payload_str, "reasonmsg").unwrap_or_default();
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
            teaspeak_escape(&reason_msg),
            clid
        );
        send_teaspeak_command(&left_cmd, session, addr, socket, iv_struct).await;
        if let Some(rc_val) = rc {
            let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
        } else {
            send_teaspeak_command("error id=0 msg=ok", session, addr, socket, iv_struct).await;
        }
        session
            .is_active
            .store(false, std::sync::atomic::Ordering::SeqCst);
        return true;
    }

    if payload_str.starts_with("channelcreate") {
        let rc = get_teaspeak_return_code(payload_str);
        let parent_id = parse_teaspeak_argument_u32(payload_str, "cpid").unwrap_or(0);
        let channel_name = parse_teaspeak_argument(payload_str, "channel_name")
            .unwrap_or_else(|| "New Channel".to_string());
        let channel_topic =
            parse_teaspeak_argument(payload_str, "channel_topic").unwrap_or_default();
        let channel_description =
            parse_teaspeak_argument(payload_str, "channel_description").unwrap_or_default();
        let flag_perm =
            parse_teaspeak_argument_u32(payload_str, "channel_flag_permanent").unwrap_or(0) != 0;
        let flag_semi = parse_teaspeak_argument_u32(payload_str, "channel_flag_semi_permanent")
            .unwrap_or(0)
            != 0;
        let channel_kind = ChannelKind::from_flags(flag_perm, flag_semi);
        let codec = parse_teaspeak_argument_u32(payload_str, "channel_codec").unwrap_or(4);
        let codec_quality =
            parse_teaspeak_argument_u32(payload_str, "channel_codec_quality").unwrap_or(6);
        let maxclients = parse_teaspeak_argument(payload_str, "channel_maxclients")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(-1);
        let maxfamilyclients = parse_teaspeak_argument(payload_str, "channel_maxfamilyclients")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(-1);

        let perm_err = {
            let rt = runtime.lock().unwrap();
            let qsession = build_teaspeak_query_session(session, addr, server_id, &rt);
            match rt.query_actor_effective_permissions(&qsession) {
                Ok((_actor, actor_permissions)) => {
                    let create_perm = match channel_kind {
                        ChannelKind::Temporary => "b_channel_create_temporary",
                        ChannelKind::SemiPermanent => "b_channel_create_semi_permanent",
                        ChannelKind::Permanent => "b_channel_create_permanent",
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
                    } else {
                        None
                    }
                }
                Err(resp) => Some(resp),
            }
        };
        if let Some(err_resp) = perm_err {
            let ack_cmd = render_teaspeak_error_ack(&err_resp, rc.as_deref());
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
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
                    parse_teaspeak_argument(payload_str, "channel_password").unwrap_or_default();
                let channel_order =
                    parse_teaspeak_argument_u32(payload_str, "channel_order").unwrap_or(0);
                let flag_default =
                    parse_teaspeak_argument_u32(payload_str, "channel_flag_default").unwrap_or(0)
                        != 0;
                let needed_talk_power =
                    parse_teaspeak_argument(payload_str, "channel_needed_talk_power")
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
            let ack_cmd = if let Some(rc_val) = &rc {
                format!(
                    "error id=0 msg=ok cid={} return_code={}",
                    channel.id, rc_val
                )
            } else {
                format!("error id=0 msg=ok cid={}", channel.id)
            };
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;

            let needed_talk = channel
                .permissions
                .get("i_channel_needed_talk_power")
                .map(|p| p.value)
                .unwrap_or(0);
            let notify_cmd = format!(
                "notifychannelcreated cid={} cpid={} channel_name={} channel_topic={} channel_description={} channel_password= channel_codec={} channel_codec_quality={} channel_maxclients={} channel_maxfamilyclients={} channel_order={} channel_flag_permanent={} channel_flag_semi_permanent={} channel_flag_default={} channel_flag_password={} channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power={} channel_forced_silence=0 channel_name_phonetic= channel_icon_id=0 channel_flag_private=0",
                channel.id,
                channel.parent_id,
                teaspeak_escape(&channel.name),
                teaspeak_escape(&channel.topic),
                teaspeak_escape(&channel.description),
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
            send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;

            let rt = runtime.lock().unwrap();
            let snapshot = crate::runtime::ChannelSnapshot::from_channel(&channel, 0);
            let notif = crate::transport::TransportNotification::ChannelCreated {
                server_id,
                channel: snapshot,
                invoker_id: session.client_id,
                invoker_name: session.client_nickname.clone(),
            };
            rt.broadcast_event(server_id, &notif);
        } else {
            let ack_cmd = if let Some(rc_val) = &rc {
                format!(
                    "error id=768 msg=could\\snot\\screate\\schannel return_code={}",
                    rc_val
                )
            } else {
                "error id=768 msg=could\\snot\\screate\\schannel".to_string()
            };
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    if payload_str.starts_with("channeldelete") {
        let rc = get_teaspeak_return_code(payload_str);
        let clid = session.client_id;
        let cid = parse_teaspeak_argument_u32(payload_str, "cid").unwrap_or(0);

        let perm_err = {
            let rt = runtime.lock().unwrap();
            let qsession = build_teaspeak_query_session(session, addr, server_id, &rt);
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
            let ack_cmd = render_teaspeak_error_ack(&err_resp, rc.as_deref());
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
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
            let mut extra_session_cmds = Vec::new();
            {
                let mut rt = runtime.lock().unwrap();
                let bot_clients_in_deleted: Vec<(u32, u64)> = rt
                    .store
                    .music_bots
                    .iter()
                    .filter_map(|(bot_id, bot)| {
                        let bclid = bot.linked_client_id?;
                        let bcid = rt.store.online_clients.get(&bclid).map(|c| c.channel_id)?;
                        (bot.server_id == server_id && bcid == cid).then_some((*bot_id, bclid))
                    })
                    .collect();

                if channel.kind == ChannelKind::Temporary {
                    for (bot_id, bot_clid) in &bot_clients_in_deleted {
                        if let Some(mut removed_bot) = rt.store.music_bots.remove(bot_id) {
                            crate::runtime::BaselineRuntime::stop_music_bot_playback(
                                &mut removed_bot,
                            );
                        }
                        extra_session_cmds.push(format!(
                            "notifyclientleftview cfid={} ctid=0 reasonid=5 reasonmsg=temporary\\schannel\\scleanup clid={}",
                            cid, bot_clid
                        ));
                        rt.remove_session_client(
                            *bot_clid,
                            5,
                            String::from("temporary channel cleanup"),
                        );
                    }
                    if !bot_clients_in_deleted.is_empty() {
                        rt.persist_state_if_configured();
                    }
                } else {
                    for (bot_id, bot_clid) in &bot_clients_in_deleted {
                        if let Some(bclient) = rt.store.online_clients.get_mut(bot_clid) {
                            bclient.channel_id = default_cid;
                        }
                        rt.sync_music_bot_client_state(*bot_id);
                        extra_session_cmds.push(format!(
                            "notifyclientmoved clid={} ctid={} cfid={} reasonid=0",
                            bot_clid, default_cid, cid
                        ));
                        if let Some(snap) = rt.online_client_snapshot(server_id, *bot_clid) {
                            let notif_move = crate::transport::TransportNotification::ClientMoved {
                                presence: crate::transport::SessionPresence {
                                    client_id: snap.id,
                                    login_name: snap.nickname,
                                    unique_identifier: snap.unique_identifier,
                                    client_type: snap.client_type_exact,
                                    server_id,
                                    channel_id: default_cid,
                                },
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
                    if !bot_clients_in_deleted.is_empty() {
                        rt.persist_state_if_configured();
                    }
                }

                if let Some(client) = rt.store.online_clients.get_mut(&clid) {
                    if client.channel_id == cid {
                        client.channel_id = default_cid;

                        extra_session_cmds.push(format!(
                            "notifyclientmoved clid={} ctid={} cfid={} reasonid=0",
                            clid, default_cid, cid
                        ));

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
            for cmd in &extra_session_cmds {
                send_teaspeak_command(cmd, session, addr, socket, iv_struct).await;
            }

            let ack_cmd = if let Some(rc_val) = &rc {
                format!("error id=0 msg=ok return_code={}", rc_val)
            } else {
                "error id=0 msg=ok".to_string()
            };
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;

            let notify_cmd = format!("notifychanneldeleted cid={}", cid);
            send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;

            let rt = runtime.lock().unwrap();
            let snapshot = crate::runtime::ChannelSnapshot::from_channel(&channel, 0);
            let notif = crate::transport::TransportNotification::ChannelDeleted {
                server_id,
                channel: snapshot,
                invoker_id: clid,
                invoker_name: session.client_nickname.clone(),
            };
            rt.broadcast_event(server_id, &notif);
        } else {
            let ack_cmd = if let Some(rc_val) = &rc {
                format!(
                    "error id=768 msg=channel\\snot\\sfound return_code={}",
                    rc_val
                )
            } else {
                "error id=768 msg=channel\\snot\\sfound".to_string()
            };
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    if payload_str.starts_with("channeledit") {
        let rc = get_teaspeak_return_code(payload_str);
        let cid = parse_teaspeak_argument_u32(payload_str, "cid").unwrap_or(0);

        let perm_err = {
            let rt = runtime.lock().unwrap();
            let qsession = build_teaspeak_query_session(session, addr, server_id, &rt);
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
                    } else if parse_teaspeak_argument(payload_str, "channel_name").is_some()
                        && let Err(p) = crate::runtime::permissions::check_required_permission(
                            &actor_permissions,
                            &["b_channel_modify_name"],
                            "b_channel_modify_name",
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
            let ack_cmd = render_teaspeak_error_ack(&err_resp, rc.as_deref());
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
            return true;
        }

        let mut updated_channel = None;

        {
            let mut rt = runtime.lock().unwrap();
            if let Some(channels) = rt.store.channels.get_mut(&server_id) {
                let make_default =
                    parse_teaspeak_argument_u32(payload_str, "channel_flag_default").unwrap_or(0)
                        != 0;
                if make_default {
                    for existing in channels.iter_mut() {
                        existing.flag_default = existing.id == cid;
                    }
                }
                if let Some(channel) = channels.iter_mut().find(|c| c.id == cid) {
                    if let Some(name) = parse_teaspeak_argument(payload_str, "channel_name") {
                        if !name.is_empty() {
                            channel.name = name;
                        }
                    }
                    if let Some(topic) = parse_teaspeak_argument(payload_str, "channel_topic") {
                        channel.topic = topic;
                    }
                    if let Some(desc) = parse_teaspeak_argument(payload_str, "channel_description")
                    {
                        channel.description = desc;
                    }
                    if let Some(password) = parse_teaspeak_argument(payload_str, "channel_password")
                    {
                        channel.flag_password = !password.is_empty();
                        channel.password = password;
                    }
                    if let Some(order) = parse_teaspeak_argument_u32(payload_str, "channel_order") {
                        channel.order = order;
                    }
                    if let Some(needed_talk) =
                        parse_teaspeak_argument(payload_str, "channel_needed_talk_power")
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
                    if let Some(flag_perm) =
                        parse_teaspeak_argument_u32(payload_str, "channel_flag_permanent")
                    {
                        let flag_semi = parse_teaspeak_argument_u32(
                            payload_str,
                            "channel_flag_semi_permanent",
                        )
                        .unwrap_or(0)
                            != 0;
                        channel.kind = ChannelKind::from_flags(flag_perm != 0, flag_semi);
                    } else if let Some(flag_semi) =
                        parse_teaspeak_argument_u32(payload_str, "channel_flag_semi_permanent")
                    {
                        let flag_perm = channel.kind == ChannelKind::Permanent;
                        channel.kind = ChannelKind::from_flags(flag_perm, flag_semi != 0);
                    }
                    if let Some(codec) = parse_teaspeak_argument_u32(payload_str, "channel_codec") {
                        channel.codec = codec;
                    }
                    if let Some(codec_quality) =
                        parse_teaspeak_argument_u32(payload_str, "channel_codec_quality")
                    {
                        channel.codec_quality = codec_quality;
                    }
                    if let Some(maxclients) =
                        parse_teaspeak_argument(payload_str, "channel_maxclients")
                            .and_then(|v| v.parse::<i32>().ok())
                    {
                        channel.maxclients = maxclients;
                    }
                    if let Some(maxfamilyclients) =
                        parse_teaspeak_argument(payload_str, "channel_maxfamilyclients")
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
            let rc = get_teaspeak_return_code(payload_str);
            let ack_cmd = if let Some(rc_val) = &rc {
                format!("error id=0 msg=ok return_code={}", rc_val)
            } else {
                "error id=0 msg=ok".to_string()
            };
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;

            let needed_talk = channel
                .permissions
                .get("i_channel_needed_talk_power")
                .map(|p| p.value)
                .unwrap_or(0);
            let notify_cmd = format!(
                "notifychanneledited cid={} channel_name={} channel_topic={} channel_description={} channel_codec={} channel_codec_quality={} channel_maxclients={} channel_maxfamilyclients={} channel_order={} channel_flag_permanent={} channel_flag_semi_permanent={} channel_flag_default={} channel_flag_password={} channel_needed_talk_power={}",
                cid,
                teaspeak_escape(&channel.name),
                teaspeak_escape(&channel.topic),
                teaspeak_escape(&channel.description),
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
            send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;

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
        } else {
            let rc = get_teaspeak_return_code(payload_str);
            let ack_cmd = if let Some(rc_val) = &rc {
                format!(
                    "error id=768 msg=channel\\snot\\sfound return_code={}",
                    rc_val
                )
            } else {
                "error id=768 msg=channel\\snot\\sfound".to_string()
            };
            send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
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
                if let Some(nick) = parse_teaspeak_argument(payload_str, "client_nickname") {
                    if !nick.is_empty() {
                        client.nickname = nick.clone();
                        nickname_val = nick;
                        nick_updated = true;
                    }
                }
                if let Some(away) = parse_teaspeak_argument_u32(payload_str, "client_away") {
                    client.away = away != 0;
                }
                if let Some(away_msg) = parse_teaspeak_argument(payload_str, "client_away_message")
                {
                    client.away_message = away_msg;
                }
                if let Some(muted) = parse_teaspeak_argument_u32(payload_str, "client_input_muted")
                {
                    client.input_muted = muted != 0;
                }
                if let Some(muted) = parse_teaspeak_argument_u32(payload_str, "client_output_muted")
                {
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
                    if let Some(val) = parse_teaspeak_argument(payload_str, key) {
                        client.extra_properties.insert(key.to_string(), val.clone());
                        if key == "client_talk_request" && val == "0" && parse_teaspeak_argument(payload_str, "client_talk_request_msg").is_none() {
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

        let rc = get_teaspeak_return_code(payload_str);
        let ack_cmd = if let Some(rc_val) = &rc {
            format!("error id=0 msg=ok return_code={}", rc_val)
        } else {
            "error id=0 msg=ok".to_string()
        };
        send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;

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
        let cid = parse_teaspeak_argument_u32(payload_str, "cid").unwrap_or(1);
        let desc = {
            let rt = runtime.lock().unwrap();
            rt.channel_by_id(server_id, cid)
                .map(|c| c.description.clone())
                .unwrap_or_default()
        };
        let rc = get_teaspeak_return_code(payload_str);
        let notify_cmd = format!(
            "notifychanneledited cid={} reasonid=9 channel_description={}",
            cid,
            teaspeak_escape(&desc)
        );
        send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;
        let ack_cmd = if let Some(rc_val) = &rc {
            format!("error id=0 msg=ok return_code={}", rc_val)
        } else {
            "error id=0 msg=ok".to_string()
        };
        send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
        return true;
    }

    let is_var_or_sub_cmd = payload_str.starts_with("servergetvariables")
        || payload_str.starts_with("connectioninfoautoupdate")
        || payload_str.starts_with("clientgetvariables")
        || payload_str.starts_with("channelsubscribeall")
        || payload_str.starts_with("channelsubscribeball");

    if is_var_or_sub_cmd {
        let rc = get_teaspeak_return_code(payload_str);

        if payload_str.starts_with("clientgetvariables") {
            let target_clid = parse_teaspeak_argument_u64(payload_str, "clid")
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
                    let is_music_bot = online
                        .extra_properties
                        .get("client_type_exact")
                        .map(String::as_str)
                        == Some("4");
                    let bot_tail = if is_music_bot {
                        let p_state = online
                            .extra_properties
                            .get("player_state")
                            .map(String::as_str)
                            .unwrap_or("4");
                        let p_vol = online
                            .extra_properties
                            .get("player_volume")
                            .map(String::as_str)
                            .unwrap_or("1");
                        let p_plid = online
                            .extra_properties
                            .get("client_playlist_id")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let c_dis = online
                            .extra_properties
                            .get("client_disabled")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let c_notify = online
                            .extra_properties
                            .get("client_flag_notify_song_change")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let c_btype = online
                            .extra_properties
                            .get("client_bot_type")
                            .map(String::as_str)
                            .unwrap_or("0");
                        let c_uptime = online
                            .extra_properties
                            .get("client_uptime_mode")
                            .map(String::as_str)
                            .unwrap_or("0");
                        format!(
                            " client_type_exact=4 player_state={} player_volume={} client_playlist_id={} client_disabled={} client_flag_notify_song_change={} client_bot_type={} client_uptime_mode={} client_owner={}",
                            teaspeak_escape(p_state),
                            teaspeak_escape(p_vol),
                            teaspeak_escape(p_plid),
                            teaspeak_escape(c_dis),
                            teaspeak_escape(c_notify),
                            teaspeak_escape(c_btype),
                            teaspeak_escape(c_uptime),
                            online.database_id,
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "notifyclientupdated clid={} client_unique_identifier={} client_nickname={} client_database_id={} client_channel_group_id={} client_servergroups={} client_talk_power={} client_input_muted={} client_output_muted={} client_outputonly_muted={} client_input_hardware={} client_output_hardware={} client_is_recording={} client_is_channel_commander={} client_is_talker={} client_away={} client_away_message={} client_description={} client_login_name= client_created=1494921612 client_lastconnected=1494921612 client_totalconnections=1 client_month_bytes_uploaded=0 client_month_bytes_downloaded=0 client_total_bytes_uploaded=0 client_total_bytes_downloaded=0 client_icon_id=0 client_country={} client_version={} client_platform={}{}",
                        target_clid,
                        teaspeak_escape(&online.unique_identifier),
                        teaspeak_escape(&online.nickname),
                        online.database_id,
                        cg_id,
                        sg_str,
                        talk_power,
                        if online.input_muted { 1 } else { 0 },
                        if online.output_muted { 1 } else { 0 },
                        teaspeak_escape(outonly),
                        teaspeak_escape(in_hw),
                        teaspeak_escape(out_hw),
                        teaspeak_escape(rec),
                        teaspeak_escape(cc),
                        teaspeak_escape(talker),
                        if online.away { 1 } else { 0 },
                        teaspeak_escape(&online.away_message),
                        teaspeak_escape(desc),
                        teaspeak_escape(&online.country),
                        teaspeak_escape(&online.version),
                        teaspeak_escape(&online.platform),
                        bot_tail,
                    )
                } else {
                    format!(
                        "notifyclientupdated clid={} client_login_name= client_created=1494921612 client_lastconnected=1494921612 client_totalconnections=1 client_month_bytes_uploaded=0 client_month_bytes_downloaded=0 client_total_bytes_uploaded=0 client_total_bytes_downloaded=0 client_icon_id=0 client_country=DE client_version=3.6.2 client_platform=Windows",
                        target_clid
                    )
                }
            };
            send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;
        } else if payload_str.starts_with("servergetvariables") {
            let server_uid_escaped = teaspeak_escape(&session.server_uid);
            let notify_cmd = format!(
                "notifyserverupdated virtualserver_name=BlackTeaSpeak virtualserver_welcomemessage=Welcome! virtualserver_maxclients=32 virtualserver_clientsonline=1 virtualserver_channelsonline=1 virtualserver_created=1494921612 virtualserver_uptime=33245 virtualserver_unique_identifier={} virtualserver_platform=Windows virtualserver_version=3.13.7\\s[Build:\\s1655727713] virtualserver_status=online",
                server_uid_escaped
            );
            send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;
        } else if payload_str.starts_with("channelsubscribeall")
            || payload_str.starts_with("channelsubscribeball")
        {
            let (sub_cmd, active_bcast_cmds) = {
                let rt = runtime.lock().unwrap();
                let mut parts = Vec::new();
                let mut bcast_cmds = Vec::new();
                if let Some(channels) = rt.store.channels.get(&server_id) {
                    for ch in channels {
                        parts.push(format!("cid={} es=0", ch.id));
                        let chan_bcasts: Vec<String> = rt
                            .store
                            .active_broadcasts
                            .values()
                            .filter(|b| {
                                b.server_id == server_id
                                    && b.channel_id == ch.id
                                    && (b.broadcast_type == 0 || b.broadcast_type == 1)
                            })
                            .map(|b| {
                                format!(
                                    "cid={} bid={} sclid={} scl={} clid={} bt={} ssrc={}",
                                    b.channel_id,
                                    b.broadcaster_client_id,
                                    b.broadcaster_client_id,
                                    b.broadcaster_client_id,
                                    b.broadcaster_client_id,
                                    b.broadcast_type,
                                    b.ssrc
                                )
                            })
                            .collect();
                        if !chan_bcasts.is_empty() {
                            bcast_cmds.push(format!(
                                "notifybroadcastvideo {}",
                                chan_bcasts.join("|")
                            ));
                        }
                    }
                }
                let sub = if parts.is_empty() {
                    "notifychannelsubscribed cid=1 es=0".to_string()
                } else {
                    format!("notifychannelsubscribed {}", parts.join("|"))
                };
                (sub, bcast_cmds)
            };
            send_teaspeak_command(&sub_cmd, session, addr, socket, iv_struct).await;
            for bcmd in active_bcast_cmds {
                send_teaspeak_command(&bcmd, session, addr, socket, iv_struct).await;
            }
        }

        let ack_cmd = if let Some(rc_val) = &rc {
            format!("error id=0 msg=ok return_code={}", rc_val)
        } else {
            "error id=0 msg=ok".to_string()
        };
        send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
        return true;
    }

    if let Ok(mut req) = crate::query::parse_request_line(payload_str) {
        let rc = get_teaspeak_return_code(payload_str);
        strip_teaspeak_return_code(&mut req);
        let cmd_name = req.command.clone();
        if cmd_name == "rtcsessiondescribe" {
            // Handled asynchronously in webrtc_bridge.rs so notifyrtcsessiondescription precedes error id=0 return_code=...
            return true;
        }

        let (resp, notifs, needed_perms_notify, permission_list_override, music_notify_cmds) = {
            let mut rt = runtime.lock().unwrap();
            let qsession_before = build_teaspeak_query_session(session, addr, server_id, &rt);
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
                            teaspeak_escape(permname),
                            teaspeak_escape(permdesc)
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
                Some(format!(
                    "notifyclientneededpermissions {}",
                    render_teaspeak_rows_only(&needed_rows)
                ))
            } else {
                None
            };

            let mut m_cmds = Vec::new();
            if resp.error_id == 0 && cmd_name == "playlistsongadd" {
                for row in &resp.rows {
                    if row.get("song_loaded").map(String::as_str) == Some("1")
                        && let (Some(plid), Some(sid)) = (row.get("playlist_id"), row.get("song_id"))
                    {
                        let meta = row.get("song_metadata").map(String::as_str).unwrap_or("");
                        m_cmds.push(format!(
                            "notifyplaylistsongloaded playlist_id={} song_id={} success=1 song_metadata={}",
                            teaspeak_escape(plid),
                            teaspeak_escape(sid),
                            teaspeak_escape(meta),
                        ));
                    }
                }
            }

            (resp, notifs, needed_notify, perm_list_override, m_cmds)
        };

        if resp.error_id == 259 || resp.error_id == 256 {
            if let Some(rc_val) = &rc {
                let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
                return true;
            }
            return false;
        }

        let skip_auto_row_notify = matches!(
            cmd_name.as_str(),
            "rtcsessiondescribe"
                | "broadcastvideo"
                | "broadcastvideojoin"
                | "broadcastvideoleave"
                | "broadcastvideoconfigure"
                | "broadcastaudio"
                | "broadcastaudiojoin"
                | "broadcastaudioleave"
                | "broadcastleave"
                | "musicbotsetsubscription"
                | "musicbotplayeraction"
        );

        if let Some(notify_perm_cmd) = permission_list_override {
            send_teaspeak_command(&notify_perm_cmd, session, addr, socket, iv_struct).await;
        } else if !resp.rows.is_empty() && !skip_auto_row_notify {
            let notify_prefix = teaspeak_notify_command_name(&cmd_name);
            let rows_str = render_teaspeak_rows_only(&resp.rows);
            let notify_cmd = format!("{} {}", notify_prefix, rows_str);
            send_teaspeak_command(&notify_cmd, session, addr, socket, iv_struct).await;
        }

        for m_cmd in &music_notify_cmds {
            send_teaspeak_command(m_cmd, session, addr, socket, iv_struct).await;
        }

        for notif in &notifs {
            if matches!(
                notif,
                crate::transport::TransportNotification::VideoBroadcastChanged { .. }
                    | crate::transport::TransportNotification::ClientUpdated { .. }
                    | crate::transport::TransportNotification::ClientPoke { .. }
                    | crate::transport::TransportNotification::ClientEnterView { .. }
                    | crate::transport::TransportNotification::ClientLeftView { .. }
            ) {
                continue;
            }
            let notif_str = crate::transport::render_notification(notif);
            send_teaspeak_command(&notif_str, session, addr, socket, iv_struct).await;
        }

        if let Some(needed_cmd) = needed_perms_notify {
            send_teaspeak_command(&needed_cmd, session, addr, socket, iv_struct).await;
        }

        let ack_cmd = render_teaspeak_error_ack(&resp, rc.as_deref());
        send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct).await;
        return true;
    }

    false
}
