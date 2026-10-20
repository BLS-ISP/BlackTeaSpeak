pub use super::wire_formatting::*;
use crate::teaspeak_transport::UdpSession;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;

pub async fn handle_ts6_command(
    payload_str: &str,
    processed_decrypted: &[u8],
    session: &mut UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
    runtime: &Arc<Mutex<crate::runtime::BaselineRuntime>>,
    server_id: u32,
    packet_id: u16,
) -> bool {
    // 1. Handle clientek (TS6 Ed25519 + Curve25519 ECDH shared secret derivation)
    if payload_str.contains("clientek") {
        let mut ek_base64 = None;
        let ek_key = b"ek=";
        if let Some(ek_idx) = processed_decrypted
            .windows(ek_key.len())
            .position(|window| window == ek_key)
        {
            let value_start = ek_idx + ek_key.len();
            let mut value_end = value_start;
            while value_end < processed_decrypted.len() {
                let c = processed_decrypted[value_end];
                if c <= 0x20 || c > 0x7E {
                    break;
                }
                value_end += 1;
            }
            if let Ok(ek_str) = std::str::from_utf8(&processed_decrypted[value_start..value_end]) {
                ek_base64 = Some(ek_str.replace("\\/", "/"));
            }
        }

        if let Some(ek_b64) = ek_base64 {
            use base64::Engine as _;
            if let Ok(client_ek_bytes) =
                base64::engine::general_purpose::STANDARD.decode(ek_b64.trim())
            {
                if let (Some(ephemeral_sec_bytes), Some(entry2_hash_bytes)) =
                    (session.ephemeral_sec, session.entry2_hash)
                {
                    if let Some((_, identity_sec, _)) = crate::desktop_crypto::load_protocol_key() {
                        use curve25519_dalek::scalar::Scalar;

                        let parent_private = Scalar::from_bytes_mod_order(
                            identity_sec.try_into().unwrap_or([0u8; 32]),
                        );
                        let ephemeral_private = Scalar::from_bytes_mod_order(ephemeral_sec_bytes);

                        let mut clamped_hash_bytes = [0u8; 32];
                        clamped_hash_bytes.copy_from_slice(&entry2_hash_bytes[0..32]);
                        clamped_hash_bytes[0] &= 0xF8;
                        clamped_hash_bytes[31] &= 0x3F;
                        clamped_hash_bytes[31] |= 0x40;

                        let clamped_hash = Scalar::from_bytes_mod_order(clamped_hash_bytes);
                        let derived_private = (ephemeral_private * clamped_hash) + parent_private;
                        let derived_private_bytes = derived_private.to_bytes();

                        if let Some(shared_secret_64) = crate::desktop_crypto::get_shared_secret2(
                            &client_ek_bytes,
                            &derived_private_bytes,
                        ) {
                            println!("TS6 clientek: successfully computed 64-byte shared secret!");
                            session.shared_secret.copy_from_slice(&shared_secret_64);

                            let new_iv_struct = crate::desktop_crypto::derive_iv_struct(
                                &session.shared_secret,
                                &session.seed_client,
                                &session.seed_server,
                            );

                            let current_mac = {
                                use sha1::{Digest, Sha1};
                                let hash = Sha1::digest(&new_iv_struct);
                                let mut mac = [0u8; 8];
                                mac.copy_from_slice(&hash[0..8]);
                                mac
                            };

                            let ack_pid = session.server_ack_packet_id;
                            session.server_ack_packet_id = ack_pid.wrapping_add(1);

                            let mut ack_header = [0u8; 3];
                            ack_header[0..2].copy_from_slice(&ack_pid.to_be_bytes());
                            ack_header[2] = 0xA6; // Unencrypted | NewProtocol | Ack

                            let ack_payload = packet_id.to_be_bytes();

                            let mut final_ack = Vec::with_capacity(13);
                            final_ack.extend_from_slice(&current_mac);
                            final_ack.extend_from_slice(&ack_header);
                            final_ack.extend_from_slice(&ack_payload);

                            let _ = socket.send_to(&final_ack, addr).await;
                            println!(
                                "Sent TS6 clientek ACK RAW ({} bytes) to {}",
                                final_ack.len(),
                                addr
                            );
                        }
                    }
                }
            }
        }
        return true;
    }

    // 2. Handle TeamSpeak 6 clientinit (advertises STREAM_P2P + FILETIME capabilities)
    if payload_str.contains("clientinit") {
        let mut client_nickname = "TeamSpeak6User".to_string();

        let nickname_key = b"client_nickname=";
        if let Some(nick_idx) = processed_decrypted
            .windows(nickname_key.len())
            .position(|window| window == nickname_key)
        {
            let value_start = nick_idx + nickname_key.len();
            let mut value_end = value_start;
            while value_end < processed_decrypted.len() {
                let c = processed_decrypted[value_end];
                if c <= 0x20 || c > 0x7E {
                    break;
                }
                value_end += 1;
            }
            if let Ok(nick) = std::str::from_utf8(&processed_decrypted[value_start..value_end]) {
                let parsed = ts6_unescape(nick);
                if !parsed.is_empty() {
                    client_nickname = parsed;
                }
            }
        }

        println!("Received TS6 clientinit! Client nickname: {}", client_nickname);
        session.client_nickname = client_nickname.clone();

        let unique_identifier = session.client_uid.clone();
        let clid = session.client_id;

        let mut mock_request = crate::query::CommandRequest {
            command: "clientinit".to_string(),
            positional_args: vec![],
            named_args: std::collections::BTreeMap::new(),
            option_groups: vec![],
            flags: std::collections::BTreeSet::new(),
        };
        mock_request
            .named_args
            .insert("client_nickname".to_string(), client_nickname.clone());

        let init_input_muted =
            parse_ts6_argument_u32(payload_str, "client_input_muted").unwrap_or(0) != 0;
        let init_output_muted =
            parse_ts6_argument_u32(payload_str, "client_output_muted").unwrap_or(0) != 0;
        let init_outputonly_muted =
            parse_ts6_argument_u32(payload_str, "client_outputonly_muted").unwrap_or(0);
        let init_input_hardware =
            parse_ts6_argument_u32(payload_str, "client_input_hardware").unwrap_or(1);
        let init_output_hardware =
            parse_ts6_argument_u32(payload_str, "client_output_hardware").unwrap_or(1);
        let init_away = parse_ts6_argument_u32(payload_str, "client_away").unwrap_or(0) != 0;
        let init_away_message =
            parse_ts6_argument(payload_str, "client_away_message").unwrap_or_default();
        let init_myts_id =
            parse_ts6_argument(payload_str, "client_myteamspeak_id").unwrap_or_default();
        let init_integrations =
            parse_ts6_argument(payload_str, "client_integrations").unwrap_or_default();
        let init_badges = parse_ts6_argument(payload_str, "client_badges").unwrap_or_default();
        let init_version = parse_ts6_argument(payload_str, "client_version")
            .unwrap_or_else(|| String::from("6.0.0"));
        let init_platform = parse_ts6_argument(payload_str, "client_platform")
            .unwrap_or_else(|| String::from("Windows"));

        // Ensure any non-TS6 active broadcasts (TeaSpeak, BTEA Desktop/Web, MusicBot) are synced
        // into `TS6_STREAM_REGISTRY` before building peer enter rows and initial stream notifications.
        let _ = super::stream_manager::sync_non_ts6_broadcasts_to_ts6(runtime, server_id);

        let (
            bootstrap,
            peer_enter_cmds,
            dbid,
            self_sg_str,
            self_cg_id,
            self_talk_power,
            notifyservergrouplist,
            notifychannelgrouplist,
            notifyclientneededpermissions,
        ) = {
            let mut rt = runtime.lock().unwrap();
            let dbid = rt
                .store
                .clients
                .values()
                .find(|c| !unique_identifier.is_empty() && c.unique_identifier == unique_identifier)
                .map(|c| c.database_id)
                .unwrap_or(clid + 1000);
            let mut mock_session = crate::runtime::QuerySessionState {
                client_id: clid,
                connection_ip: addr.ip().to_string(),
                authenticated_login: None,
                selected_virtual_server_id: Some(server_id),
                current_channel_id: None,
                virtual_mode: false,
                notification_subscriptions: vec![],
                actor_client_database_id_override: Some(dbid),
                client_nickname: client_nickname.clone(),
                client_away: init_away,
                client_away_message: init_away_message.clone(),
                client_input_muted: init_input_muted,
                client_output_muted: init_output_muted,
                is_desktop_client: false,
                whisper_targets: None,
                ignored_clients: Vec::new(),
                subscribed_channels: std::collections::BTreeSet::new(),
                points: 0,
                blocked_until_millis: 0,
                last_points_decay_millis: 0,
            };

            let bootstrap = crate::transport::handlers::perform_client_bootstrap(
                &mut rt,
                &mut mock_session,
                &mock_request,
                &unique_identifier,
                true,
            )
            .unwrap();

            if let Some(self_online) = rt.store.online_clients.get_mut(&clid) {
                self_online.input_muted = init_input_muted;
                self_online.output_muted = init_output_muted;
                self_online.away = init_away;
                self_online.away_message = init_away_message.clone();
                self_online.version = init_version;
                self_online.platform = init_platform;
                self_online.extra_properties.insert(
                    String::from("client_outputonly_muted"),
                    init_outputonly_muted.to_string(),
                );
                self_online.extra_properties.insert(
                    String::from("client_input_hardware"),
                    init_input_hardware.to_string(),
                );
                self_online.extra_properties.insert(
                    String::from("client_output_hardware"),
                    init_output_hardware.to_string(),
                );
                self_online
                    .extra_properties
                    .insert(String::from("client_is_streaming"), String::from("0"));
                if !init_badges.is_empty() {
                    self_online
                        .extra_properties
                        .insert(String::from("client_badges"), init_badges.clone());
                }
            }

            let mut peers = Vec::new();
            for online in rt.store.online_clients.values() {
                if online.server_id == server_id && online.id != clid && online.id < 65535 {
                    let peer_sg = if online.server_groups.is_empty() {
                        String::from("8")
                    } else {
                        online
                            .server_groups
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    let peer_cg = rt.effective_channel_group_id_for_client(
                        server_id,
                        online.channel_id,
                        online.database_id,
                    );
                    let peer_tp =
                        rt.client_talk_power(server_id, online.channel_id, online.database_id);
                    let peer_outonly = online
                        .extra_properties
                        .get("client_outputonly_muted")
                        .map(String::as_str)
                        .unwrap_or("0");
                    let peer_in_hw = online
                        .extra_properties
                        .get("client_input_hardware")
                        .map(String::as_str)
                        .unwrap_or("1");
                    let peer_out_hw = online
                        .extra_properties
                        .get("client_output_hardware")
                        .map(String::as_str)
                        .unwrap_or("1");
                    let peer_rec = online
                        .extra_properties
                        .get("client_is_recording")
                        .map(String::as_str)
                        .unwrap_or("0");
                    let peer_talker = online
                        .extra_properties
                        .get("client_is_talker")
                        .map(String::as_str)
                        .unwrap_or("0");
                    let peer_cc = online
                        .extra_properties
                        .get("client_is_channel_commander")
                        .map(String::as_str)
                        .unwrap_or("0");
                    let peer_desc = online
                        .extra_properties
                        .get("client_description")
                        .map(String::as_str)
                        .unwrap_or("");
                    let peer_streaming = online
                        .extra_properties
                        .get("client_is_streaming")
                        .map(String::as_str)
                        .unwrap_or_else(|| {
                            if super::stream_manager::is_client_streaming_ts6(server_id, online.id) {
                                "1"
                            } else {
                                "0"
                            }
                        });
                    peers.push(format!(
                        "notifycliententerview cfid=0 ctid={} reasonid=0 clid={} client_unique_identifier={} client_nickname={} client_input_muted={} client_output_muted={} client_outputonly_muted={} client_input_hardware={} client_output_hardware={} client_meta_data= client_is_recording={} client_database_id={} client_channel_group_id={} client_servergroups={} client_totalconnections=1 client_away={} client_away_message={} client_type={} client_flag_avatar= client_talk_power={} client_talk_request=0 client_talk_request_msg= client_description={} client_is_talker={} client_is_priority_speaker=0 client_unread_messages=0 client_nickname_phonetic= client_needed_serverquery_view_power=0 client_icon_id=0 client_is_channel_commander={} client_country=DE client_channel_group_inherited_channel_id={} client_badges= client_myteamspeak_id= client_integrations= client_is_streaming={}",
                        online.channel_id,
                        online.id,
                        ts6_escape(&online.unique_identifier),
                        ts6_escape(&online.nickname),
                        if online.input_muted { 1 } else { 0 },
                        if online.output_muted { 1 } else { 0 },
                        ts6_escape(peer_outonly),
                        ts6_escape(peer_in_hw),
                        ts6_escape(peer_out_hw),
                        ts6_escape(peer_rec),
                        online.database_id,
                        peer_cg,
                        peer_sg,
                        if online.away { 1 } else { 0 },
                        ts6_escape(&online.away_message),
                        online.client_type,
                        peer_tp,
                        ts6_escape(peer_desc),
                        ts6_escape(peer_talker),
                        ts6_escape(peer_cc),
                        online.channel_id,
                        peer_streaming,
                    ));
                }
            }

            let self_sg_str = rt
                .store
                .online_clients
                .get(&clid)
                .map(|c| {
                    if c.server_groups.is_empty() {
                        String::from("8")
                    } else {
                        c.server_groups
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    }
                })
                .unwrap_or_else(|| String::from("8"));
            let self_cg_id = rt.effective_channel_group_id_for_client(
                server_id,
                bootstrap.default_cid as u32,
                dbid,
            );
            let self_talk_power =
                rt.client_talk_power(server_id, bootstrap.default_cid as u32, dbid);

            let sg_rows = rt.web_server_group_rows();
            let cg_rows = rt.web_channel_group_rows();
            let needed_rows = rt
                .web_client_needed_permission_rows(server_id, bootstrap.default_cid as u32, dbid)
                .unwrap_or_default();

            let notify_sg = format!("notifyservergrouplist {}", render_ts6_rows_only(&sg_rows));
            let notify_cg = format!("notifychannelgrouplist {}", render_ts6_rows_only(&cg_rows));
            let notify_needed = if needed_rows.is_empty() {
                String::from("notifyclientneededpermissions permid=1 permvalue=0")
            } else {
                format!(
                    "notifyclientneededpermissions {}",
                    render_ts6_rows_only(&needed_rows)
                )
            };

            let enter_self_notif = crate::transport::TransportNotification::ClientEnterView {
                presence: crate::transport::SessionPresence {
                    client_id: clid,
                    login_name: client_nickname.clone(),
                    unique_identifier: unique_identifier.clone(),
                    client_type: 0,
                    server_id,
                    channel_id: bootstrap.default_cid as u32,
                },
                from_channel_id: None,
                reason_id: 0,
            };
            rt.broadcast_event(server_id, &enter_self_notif);
            (
                bootstrap,
                peers,
                dbid,
                self_sg_str,
                self_cg_id,
                self_talk_power,
                notify_sg,
                notify_cg,
                notify_needed,
            )
        };

        let server_name = bootstrap.server_name;
        let welcome_msg = bootstrap.welcome_message;
        let default_cid = bootstrap.default_cid;

        let mut channel_parts = Vec::new();
        for channel in bootstrap.channels {
            let name_esc = ts6_escape(&channel.name);
            let topic_esc = ts6_escape(&channel.topic);
            let desc_esc = ts6_escape(&channel.description);
            let is_perm = if channel.kind.is_permanent() { 1 } else { 0 };
            let is_semi = if channel.kind.is_semi_permanent() {
                1
            } else {
                0
            };
            let is_default = if default_cid == (channel.id as u64) {
                1
            } else {
                0
            };
            channel_parts.push(format!(
                "cid={} cpid={} channel_name={} channel_topic={} channel_description={} channel_password={} channel_codec={} channel_codec_quality={} channel_maxclients={} channel_maxfamilyclients={} channel_order={} channel_flag_permanent={} channel_flag_semi_permanent={} channel_flag_default={} channel_flag_password={} channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power={} channel_forced_silence=0 channel_name_phonetic= channel_icon_id=0 channel_flag_private=0",
                channel.id,
                channel.parent_id,
                name_esc,
                topic_esc,
                desc_esc,
                ts6_escape(&channel.password),
                channel.codec,
                channel.codec_quality,
                channel.maxclients,
                channel.maxfamilyclients,
                channel.order,
                is_perm,
                is_semi,
                is_default,
                if channel.flag_password || !channel.password.is_empty() { 1 } else { 0 },
                channel.needed_talk_power()
            ));
        }

        let channellist = if channel_parts.is_empty() {
            "channellist cid=1 cpid=0 channel_name=Default\\sChannel channel_topic= channel_description= channel_password= channel_codec=4 channel_codec_quality=6 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=1 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=1 channel_flag_maxfamilyclients_inherited=0 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic= channel_icon_id=0 channel_flag_private=0".to_string()
        } else {
            format!("channellist {}", channel_parts.join("|"))
        };

        let server_name_escaped = ts6_escape(&server_name);
        let welcome_msg_escaped = ts6_escape(&welcome_msg);
        let client_nickname_escaped = ts6_escape(&client_nickname);
        let client_uid_escaped = ts6_escape(&session.client_uid);
        let server_uid_escaped = ts6_escape(&session.server_uid);
        let cap_ext_escaped = ts6_escape(TS6_CAPABILITY_EXTENSIONS);

        // TeamSpeak 6 `initserver` with `virtualserver_capability_extensions=FILETIME,STREAM_P2P`
        let initserver = format!(
            "initserver virtualserver_name={} virtualserver_welcomemessage={} virtualserver_maxclients=32 virtualserver_password= virtualserver_clientsonline=1 virtualserver_channelsonline=1 virtualserver_created=1494921612 virtualserver_uptime=33245 virtualserver_hostmessage= virtualserver_hostmessage_mode=0 virtualserver_filebase=files\\/virtualserver_1 virtualserver_default_server_group=8 virtualserver_default_channel_group=8 virtualserver_flag_password=0 virtualserver_default_channel_admin_group=5 virtualserver_max_download_total_bandwidth=-1 virtualserver_max_upload_total_bandwidth=-1 virtualserver_hostbanner_url= virtualserver_hostbanner_gfx_url= virtualserver_hostbanner_gfx_interval=0 virtualserver_complain_autoban_count=5 virtualserver_complain_autoban_time=1200 virtualserver_complain_remove_time=3600 virtualserver_min_clients_in_channel_before_forced_silence=100 virtualserver_priority_speaker_dimm_modificator=-18.0000 virtualserver_id=1 virtualserver_antiflood_points_tick_reduce=5 virtualserver_antiflood_points_needed_command_block=150 virtualserver_antiflood_points_needed_ip_block=250 virtualserver_client_connections=1 virtualserver_query_client_connections=0 virtualserver_hostbutton_tooltip= virtualserver_hostbutton_url= virtualserver_hostbutton_gfx_url= virtualserver_queryclientsonline=0 virtualserver_download_quota=-1 virtualserver_upload_quota=-1 virtualserver_month_bytes_downloaded=0 virtualserver_month_bytes_uploaded=0 virtualserver_total_bytes_downloaded=0 virtualserver_total_bytes_uploaded=0 virtualserver_port=9987 virtualserver_autostart=1 virtualserver_machine_id= virtualserver_needed_identity_security_level=8 virtualserver_log_client=0 virtualserver_log_query=0 virtualserver_log_channel=0 virtualserver_log_permissions=1 virtualserver_log_server=0 virtualserver_log_filetransfer=0 virtualserver_min_client_version=1481105459 virtualserver_name_phonetic= virtualserver_icon_id=0 virtualserver_reserved_slots=0 virtualserver_total_packetloss_speech=0.0000 virtualserver_total_packetloss_keepalive=0.0000 virtualserver_total_packetloss_control=0.0000 virtualserver_total_packetloss_total=0.0000 virtualserver_total_ping=0.0000 virtualserver_ip=0.0000 virtualserver_weblist_identifier= virtualserver_ask_for_privilegekey=0 virtualserver_hostbanner_mode=0 virtualserver_channel_temp_delete_delay_default=0 virtualserver_min_android_version=1429007622 virtualserver_min_ios_version=1429007622 virtualserver_nickname= virtualserver_unique_identifier={} virtualserver_platform=Windows virtualserver_version={} virtualserver_status=online virtualserver_codec_encryption_mode=0 virtualserver_capability_extensions={} client_talk_power={} client_needed_serverquery_view_power=0 client_myteamspeak_id={} client_integrations={} lt=0 pv=6 acn={} aclid={}",
            server_name_escaped,
            welcome_msg_escaped,
            server_uid_escaped,
            TS6_SERVER_VERSION,
            cap_ext_escaped,
            self_talk_power,
            ts6_escape(&init_myts_id),
            ts6_escape(&init_integrations),
            client_nickname_escaped,
            clid
        );

        let notifycliententerview = format!(
            "notifycliententerview cfid=0 ctid={} reasonid=2 clid={} client_unique_identifier={} client_nickname={} client_input_muted={} client_output_muted={} client_outputonly_muted={} client_input_hardware={} client_output_hardware={} client_meta_data= client_is_recording=0 client_database_id={} client_channel_group_id={} client_servergroups={} client_totalconnections=1 client_away={} client_away_message={} client_type=0 client_flag_avatar= client_talk_power={} client_talk_request=0 client_talk_request_msg= client_description= client_is_talker=0 client_is_priority_speaker=0 client_unread_messages=0 client_nickname_phonetic= client_needed_serverquery_view_power=0 client_icon_id=0 client_is_channel_commander=0 client_country=DE client_channel_group_inherited_channel_id={} client_badges={} client_myteamspeak_id={} client_integrations={} client_is_streaming=0",
            default_cid,
            clid,
            client_uid_escaped,
            client_nickname_escaped,
            if init_input_muted { 1 } else { 0 },
            if init_output_muted { 1 } else { 0 },
            init_outputonly_muted,
            init_input_hardware,
            init_output_hardware,
            dbid,
            self_cg_id,
            self_sg_str,
            if init_away { 1 } else { 0 },
            ts6_escape(&init_away_message),
            self_talk_power,
            default_cid,
            ts6_escape(&init_badges),
            ts6_escape(&init_myts_id),
            ts6_escape(&init_integrations),
        );
        let channellistfinished = "channellistfinished".to_string();
        let notifychannelsubscribed = format!("notifychannelsubscribed cid={}", default_cid);

        let mut commands_to_send = vec![
            initserver,
            channellist,
            notifychannelsubscribed,
            notifycliententerview,
        ];
        commands_to_send.extend(peer_enter_cmds);
        commands_to_send.extend(vec![
            channellistfinished,
            notifyservergrouplist,
            notifychannelgrouplist,
            notifyclientneededpermissions,
        ]);

        // Also announce any active TS6 streams on this server to the newly connected TS6 client
        for active_stream in super::stream_manager::active_ts6_streams_for_server(server_id) {
            commands_to_send.push(active_stream.render_notifystreamstarted(None));
            for &attendee_clid in &active_stream.attendees {
                commands_to_send.push(format!(
                    "notifystreamclientjoined clid={} id={}",
                    attendee_clid,
                    ts6_escape(&active_stream.id)
                ));
            }
        }

        for (cmd_idx, cmd) in commands_to_send.iter().enumerate() {
            send_ts6_command(cmd, session, addr, socket, iv_struct).await;
            println!(
                "Sent TS6 {} to {}",
                cmd.split(' ').next().unwrap_or(""),
                addr
            );

            if cmd_idx == 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
        }
        return true;
    }

    // 3. Handle TeamSpeak 6 Video & Desktop/Screen-Sharing commands
    if super::stream_manager::handle_ts6_stream_command(
        payload_str,
        session,
        addr,
        socket,
        iv_struct,
        runtime,
        server_id,
    )
    .await
    {
        return true;
    }

    // 4. Handle interactive TS6 clientmove
    if payload_str.starts_with("clientmove") {
        let clid = parse_ts6_argument_u64(payload_str, "clid")
            .filter(|&id| id != 0)
            .unwrap_or(session.client_id);
        let cid = parse_ts6_argument_u32(payload_str, "cid").unwrap_or(1);
        let rc = get_ts6_return_code(payload_str);

        let (from_cid, perm_err, ok) = {
            let mut rt = runtime.lock().unwrap();
            let old_cid = rt
                .store
                .online_clients
                .get(&clid)
                .map(|c| c.channel_id)
                .unwrap_or(1);
            let mut qsession = build_ts6_query_session(session, addr, server_id, &rt);
            if let Ok(mut req) = crate::query::parse_request_line(payload_str) {
                strip_ts6_return_code(&mut req);
                req.named_args.insert(String::from("clid"), clid.to_string());
                req.named_args.insert(String::from("cid"), cid.to_string());
                let resp = rt.handle_clientmove(&req, &mut qsession);
                if resp.error_id != 0 {
                    (old_cid, Some(resp), false)
                } else {
                    (old_cid, None, true)
                }
            } else {
                (old_cid, None, false)
            }
        };
        if let Some(err_resp) = perm_err {
            if rc.is_some() {
                let ack_cmd = render_ts6_error_ack(&err_resp, rc.as_deref());
                send_ts6_command(&ack_cmd, session, addr, socket, iv_struct).await;
            }
            return true;
        }

        if ok {
            let is_other = clid != session.client_id;
            let move_reason_id = if is_other { 1 } else { 0 };
            let notify_cmd = if is_other {
                format!(
                    "notifyclientmoved cfid={} ctid={} reasonid=1 invokerid={} invokername={} invokeruid={} clid={}",
                    from_cid,
                    cid,
                    session.client_id,
                    ts6_escape(&session.client_nickname),
                    ts6_escape(&session.client_uid),
                    clid
                )
            } else {
                format!(
                    "notifyclientmoved cfid={} ctid={} reasonid=0 clid={}",
                    from_cid, cid, clid
                )
            };
            send_ts6_command(&notify_cmd, session, addr, socket, iv_struct).await;

            if let Some(rc_val) = &rc {
                let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
                send_ts6_command(&ack_cmd, session, addr, socket, iv_struct).await;
            }

            let rt = runtime.lock().unwrap();
            let (moved_nick, moved_uid, moved_type) = rt
                .store
                .online_clients
                .get(&clid)
                .map(|c| {
                    (
                        c.nickname.clone(),
                        c.unique_identifier.clone(),
                        c.client_type,
                    )
                })
                .unwrap_or_else(|| {
                    (
                        session.client_nickname.clone(),
                        session.client_uid.clone(),
                        0,
                    )
                });
            let presence = crate::transport::SessionPresence {
                client_id: clid,
                login_name: moved_nick,
                unique_identifier: moved_uid,
                client_type: moved_type,
                server_id,
                channel_id: cid,
            };
            let notif = crate::transport::TransportNotification::ClientMoved {
                presence,
                from_channel_id: from_cid,
                reason_id: move_reason_id,
                reason_message: String::new(),
                invoker_id: session.client_id,
                invoker_name: session.client_nickname.clone(),
                invoker_uid: session.client_uid.clone(),
            };
            rt.broadcast_event(server_id, &notif);
        } else if let Some(rc_val) = &rc {
            let ack_cmd = format!(
                "error id=768 msg=client\\snot\\sfound return_code={}",
                rc_val
            );
            send_ts6_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }
        return true;
    }

    // 5. Handle TS6 sendtextmessage
    if payload_str.starts_with("sendtextmessage") {
        let clid = session.client_id;
        let targetmode = parse_ts6_argument_u32(payload_str, "targetmode").unwrap_or(2);
        let msg = parse_ts6_argument(payload_str, "msg").unwrap_or_default();

        let msg_esc = ts6_escape(&msg);
        let nick_esc = ts6_escape(&session.client_nickname);
        let uid_esc = ts6_escape(&session.client_uid);
        let notify_cmd = format!(
            "notifytextmessage targetmode={} msg={} invokerid={} invokername={} invokeruid={}",
            targetmode, msg_esc, clid, nick_esc, uid_esc
        );
        send_ts6_command(&notify_cmd, session, addr, socket, iv_struct).await;

        if let Some(rc_val) = get_ts6_return_code(payload_str) {
            let ack_cmd = format!("error id=0 msg=ok return_code={}", rc_val);
            send_ts6_command(&ack_cmd, session, addr, socket, iv_struct).await;
        }

        let mut rt = runtime.lock().unwrap();
        let current_cid = rt.store.online_clients.get(&clid).map(|c| c.channel_id);
        let text_target = crate::runtime::TextMessageTarget {
            target_mode: targetmode,
            server_id,
            channel_id: if targetmode == 2 { current_cid } else { None },
            target_client_id: if targetmode == 1 {
                parse_ts6_argument_u64(payload_str, "target")
            } else {
                None
            },
            message: msg.clone(),
        };
        rt.record_text_message(
            &text_target,
            clid + 1000,
            session.client_uid.clone(),
            session.client_nickname.clone(),
        );
        let target_cid_opt = text_target.target_client_id;
        let notif = crate::transport::TransportNotification::TextMessage {
            target: text_target,
            invoker_id: clid,
            invoker_name: session.client_nickname.clone(),
            invoker_uid: session.client_uid.clone(),
        };
        rt.broadcast_event(server_id, &notif);
        rt.try_handle_musicbot_chat_command(server_id, clid, targetmode, target_cid_opt, &msg);
        return true;
    }

    super::channel_and_runtime_dispatch::handle_ts6_channel_and_runtime_command(
        payload_str,
        session,
        addr,
        socket,
        iv_struct,
        runtime,
        server_id,
    )
    .await
}
