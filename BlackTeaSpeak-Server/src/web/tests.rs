mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::{
        PresenceBroadcast, SharedPendingFrames, SharedBlackTeaWebSessions, BlackTeaWebFrameBroadcast,
        BlackTeaWebDisconnectKind, BlackTeaWebNotificationBridge, BlackTeaWebPresence,
        BlackTeaWebRtcNotificationBridge, BlackTeaWebSessionHandler,
        broadcast_frames_for_presence_change, broadcast_permission_refreshes,
        broadcast_queued_frames,
        default_self_client_state, derive_direct_frame, derive_peer_frames,
        derive_query_notifications_from_presence, drain_pending_frames,
        generate_localhost_tls_assets, load_tls_config_from_files, permission_refresh_scope,
        presence_enter_view_row, presence_left_view_row, presence_move_row,
        presence_update_row,
        register_or_update_session, session_presence_from_blackteaweb_presence,
        blackteaweb_disconnect_kind, blackteaweb_disconnect_reason,
    };
    use crate::rtc::BlackTeaWebRtcManager;
    use crate::runtime::{
        BaselineRuntime, QuerySessionState, TextMessageTarget,
        create_baseline_runtime_with_state_path, stable_web_client_database_id,
        stable_web_client_unique_identifier,
    };
    use crate::transport::{SessionPresence, TransportNotification};
    use rtc::interceptor::Registry;
    use rtc::peer_connection::configuration::RTCConfigurationBuilder;
    use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
    use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9, MediaEngine};
    use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
    use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
    use webrtc::runtime::{block_on, default_runtime};

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .to_path_buf()
    }

    fn parse_frame(payload: &str) -> Value {
        serde_json::from_str(payload).expect("frame should be valid JSON")
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "BlackTeaSpeak-Server-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    fn create_test_runtime(label: &str) -> BaselineRuntime {
        let state_path = unique_temp_dir(label).join("runtime-state.json");
        create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should load")
    }

    fn register_test_session(
        sessions: &SharedBlackTeaWebSessions,
        handler: &BlackTeaWebSessionHandler,
        runtime: &BaselineRuntime,
    ) -> SharedPendingFrames {
        let pending_frames = Arc::new(Mutex::new(Vec::new()));
        register_or_update_session(
            sessions,
            handler
                .presence()
                .expect("logged in handler should expose presence"),
            handler
                .self_client_database_id()
                .expect("logged in handler should expose a database id"),
            handler.visible_channel_ids(runtime),
            Arc::clone(&pending_frames),
        )
        .expect("session should register");
        pending_frames
    }

    fn drain_test_frames(pending_frames: &SharedPendingFrames) -> Vec<String> {
        drain_pending_frames(pending_frames).expect("pending frames should drain")
    }

    fn attach_test_realtime_support(
        handler: &mut BlackTeaWebSessionHandler,
        sessions: SharedBlackTeaWebSessions,
    ) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rtc_manager = Arc::new(
            BlackTeaWebRtcManager::new(Arc::new(BlackTeaWebRtcNotificationBridge {
                sessions: Arc::clone(&sessions),
            }))
            .expect("rtc manager should initialize"),
        );
        handler.set_sessions(sessions);
        handler.set_rtc_manager(rtc_manager);
    }

    #[derive(Clone)]
    struct TestRtcOfferHandler;

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for TestRtcOfferHandler {}

    fn register_teawweb_rtc_test_codecs(media_engine: &mut MediaEngine) {
        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    rtp_codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_OPUS.to_string(),
                        clock_rate: 48_000,
                        channels: 2,
                        sdp_fmtp_line: String::from(
                            "minptime=1;maxptime=20;useinbandfec=1;usedtx=1;stereo=0;sprop-stereo=0",
                        ),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 111,
                    ..Default::default()
                },
                RtpCodecKind::Audio,
            )
            .expect("BlackTeaWeb opus codec should register");
        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    rtp_codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_OPUS.to_string(),
                        clock_rate: 48_000,
                        channels: 2,
                        sdp_fmtp_line: String::from(
                            "minptime=1;maxptime=20;useinbandfec=1;usedtx=1;stereo=1;sprop-stereo=1",
                        ),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 112,
                    ..Default::default()
                },
                RtpCodecKind::Audio,
            )
            .expect("BlackTeaWeb stereo opus codec should register");
        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    rtp_codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_VP8.to_string(),
                        clock_rate: 90_000,
                        channels: 0,
                        sdp_fmtp_line: String::new(),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 120,
                    ..Default::default()
                },
                RtpCodecKind::Video,
            )
            .expect("BlackTeaWeb VP8 codec should register");
        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    rtp_codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_VP9.to_string(),
                        clock_rate: 90_000,
                        channels: 0,
                        sdp_fmtp_line: String::from("profile-id=0"),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 98,
                    ..Default::default()
                },
                RtpCodecKind::Video,
            )
            .expect("BlackTeaWeb VP9 codec should register");
        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    rtp_codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_H264.to_string(),
                        clock_rate: 90_000,
                        channels: 0,
                        sdp_fmtp_line: String::from(
                            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f",
                        ),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 126,
                    ..Default::default()
                },
                RtpCodecKind::Video,
            )
            .expect("BlackTeaWeb H264 codec should register");
    }

    fn generate_rtc_test_offer() -> String {
        let _ = rustls::crypto::ring::default_provider().install_default();
        block_on(async {
            let runtime = default_runtime().expect("webrtc runtime should exist in tests");
            let mut media_engine = MediaEngine::default();
            register_teawweb_rtc_test_codecs(&mut media_engine);
            let registry = register_default_interceptors(Registry::new(), &mut media_engine)
                .expect("default interceptors should register");
            let peer = PeerConnectionBuilder::new()
                .with_configuration(RTCConfigurationBuilder::new().build())
                .with_media_engine(media_engine)
                .with_interceptor_registry(registry)
                .with_handler(Arc::new(TestRtcOfferHandler))
                .with_runtime(runtime)
                .with_udp_addrs(vec![String::from("0.0.0.0:0")])
                .build()
                .await
                .expect("test offer peer should build");
            let _ = peer
                .add_transceiver_from_kind(RtpCodecKind::Audio, None)
                .await
                .expect("audio transceiver should add");
            let _ = peer
                .add_transceiver_from_kind(RtpCodecKind::Video, None)
                .await
                .expect("video transceiver should add");
            let offer = peer
                .create_offer(None)
                .await
                .expect("test offer should create");
            peer.set_local_description(offer)
                .await
                .expect("test offer local description should set");
            peer.local_description()
                .await
                .expect("test offer local description should exist")
                .sdp
        })
    }

    fn extract_response_field(response: &str, key: &str) -> Option<String> {
        response.lines().find_map(|line| {
            line.split_whitespace().find_map(|part| {
                part.split_once('=')
                    .and_then(|(name, value)| (name == key).then(|| value.to_string()))
            })
        })
    }

    #[test]
    fn explicit_close_stays_left_server() {
        assert_eq!(
            blackteaweb_disconnect_kind(true, false),
            BlackTeaWebDisconnectKind::LeftServer
        );
        assert_eq!(blackteaweb_disconnect_reason(BlackTeaWebDisconnectKind::LeftServer), (8, "left server"));
    }

    #[test]
    fn missing_close_frame_is_classified_as_timeout() {
        assert_eq!(
            blackteaweb_disconnect_kind(false, false),
            BlackTeaWebDisconnectKind::TimedOut
        );
        assert_eq!(blackteaweb_disconnect_reason(BlackTeaWebDisconnectKind::TimedOut), (3, ""));
    }

    #[test]
    fn ping_timeout_forces_timeout_disconnect_kind() {
        assert_eq!(
            blackteaweb_disconnect_kind(true, true),
            BlackTeaWebDisconnectKind::TimedOut
        );
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

    fn add_server_group_to_client(
        runtime: &mut BaselineRuntime,
        query_client_id: u64,
        group_id: u32,
        client_database_id: u64,
    ) {
        let mut admin = login_query_serveradmin(runtime, query_client_id);
        assert!(
            runtime
                .execute(
                    &format!("servergroupaddclient sgid={} cldbid={}", group_id, client_database_id),
                    &mut admin,
                )
                .contains("error id=0 msg=ok")
        );
    }

    fn query_presence(runtime: &BaselineRuntime, session: &QuerySessionState) -> SessionPresence {
        let nickname = session
            .selected_virtual_server_id
            .and_then(|server_id| runtime.online_client_snapshot(server_id, session.client_id))
            .map(|snapshot| snapshot.nickname)
            .unwrap_or_else(|| session.effective_nickname());
        SessionPresence {
            client_id: session.client_id,
            login_name: nickname,
            unique_identifier: runtime.query_session_unique_identifier(session),
            client_type: 1,
            server_id: session
                .selected_virtual_server_id
                .expect("query session should have a selected server"),
            channel_id: session.current_channel_id.unwrap_or(1),
        }
    }

    fn command_name(payload: &str) -> String {
        parse_frame(payload)["command"]
            .as_str()
            .expect("frame should expose command")
            .to_string()
    }

    fn assert_text_frame(
        payload: &str,
        target_mode: &str,
        message: &str,
        invoker_name: &str,
        invoker_uid: &str,
    ) {
        let frame = parse_frame(payload);
        assert_eq!(frame["command"], "notifytextmessage");
        assert_eq!(frame["data"][0]["targetmode"], target_mode);
        assert_eq!(frame["data"][0]["msg"], message);
        assert_eq!(frame["data"][0]["invokername"], invoker_name);
        assert_eq!(frame["data"][0]["invokeruid"], invoker_uid);
    }

    fn assert_text_notification(
        notification: &TransportNotification,
        target_mode: u32,
        channel_id: Option<u32>,
        target_client_id: Option<u64>,
        message: &str,
        invoker_id: u64,
        invoker_name: &str,
        invoker_uid: &str,
    ) {
        match notification {
            TransportNotification::TextMessage {
                target,
                invoker_id: actual_invoker_id,
                invoker_name: actual_invoker_name,
                invoker_uid: actual_invoker_uid,
            } => {
                assert_eq!(target.target_mode, target_mode);
                assert_eq!(target.server_id, 1);
                assert_eq!(target.channel_id, channel_id);
                assert_eq!(target.target_client_id, target_client_id);
                assert_eq!(target.message, message);
                assert_eq!(*actual_invoker_id, invoker_id);
                assert_eq!(actual_invoker_name, invoker_name);
                assert_eq!(actual_invoker_uid, invoker_uid);
            }
            _ => panic!("expected text notification"),
        }
    }

    fn parse_u64_field(frame: &Value, key: &str) -> u64 {
        frame["data"][0][key]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .expect("field should be a numeric string")
    }

    fn login_with_identity(
        handler: &mut BlackTeaWebSessionHandler,
        runtime: &mut BaselineRuntime,
        public_key: &str,
        nickname: &str,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        handler
            .handle_text_frame(r#"{"type":"enable-raw-commands"}"#, runtime)
            .expect("enable-raw-commands should succeed");

        let begin = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"handshakebegin","data":[{{"return_code":"1","intention":0,"authentication_method":1,"publicKey":"{}"}}]}}"#,
                    public_key,
                ),
                runtime,
            )
            .expect("handshakebegin should succeed");
        let proof = handler
            .handle_text_frame(
                r#"{"type":"command","command":"handshakeindentityproof","data":[{"return_code":"2","proof":"signed-proof"}]}"#,
                runtime,
            )
            .expect("proof should succeed");
        let init = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientinit","data":[{{"return_code":"3","client_nickname":"{}","client_server_password":"","client_default_channel":"/"}}]}}"#,
                    nickname,
                ),
                runtime,
            )
            .expect("clientinit should succeed");

        (begin, proof, init)
    }

    fn login(
        handler: &mut BlackTeaWebSessionHandler,
        runtime: &mut BaselineRuntime,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        login_with_identity(handler, runtime, "compat-public-key", "Tea Web")
    }

    #[test]
    fn clientinit_default_token_is_applied_before_connected_bootstrap() {
        let mut handler = BlackTeaWebSessionHandler::new(81);
        let mut runtime = create_test_runtime("blackteaweb-clientinit-default-token");
        let mut admin = login_query_serveradmin(&mut runtime, 2081);

        let instance_info = runtime.execute("instanceinfo", &mut admin);
        let server_admin_group_id = extract_response_field(
            &instance_info,
            "serverinstance_template_serveradmin_group",
        )
        .expect("instanceinfo should expose server admin group")
        .parse::<u32>()
        .expect("server admin group id should parse");
        let created_key = runtime.execute(
            &format!(
                r"privilegekeyadd token_description=Connect\sGrant token_max_uses=1 action_type=2 action_id1={}",
                server_admin_group_id
            ),
            &mut admin,
        );
        let token = extract_response_field(&created_key, "token")
            .expect("privilegekeyadd should expose token");

        handler
            .handle_text_frame(r#"{"type":"enable-raw-commands"}"#, &mut runtime)
            .expect("enable-raw-commands should succeed");
        handler
            .handle_text_frame(
                r#"{"type":"command","command":"handshakebegin","data":[{"return_code":"1","intention":0,"authentication_method":1,"publicKey":"compat-public-key-connect-token"}]}"#,
                &mut runtime,
            )
            .expect("handshakebegin should succeed");
        handler
            .handle_text_frame(
                r#"{"type":"command","command":"handshakeindentityproof","data":[{"return_code":"2","proof":"signed-proof"}]}"#,
                &mut runtime,
            )
            .expect("identity proof should succeed");

        let init = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientinit","data":[{{"return_code":"3","client_nickname":"Tea Web Token","client_server_password":"","client_default_channel":"/","client_default_token":"{}"}}]}}"#,
                    token,
                ),
                &mut runtime,
            )
            .expect("clientinit should succeed with default token");

        let init_result = parse_frame(init.last().expect("clientinit result should exist"));
        assert_eq!(init_result["command"], "error");
        assert_eq!(init_result["data"][0]["id"], "0");

        let self_client_database_id = handler
            .self_client_database_id()
            .expect("logged in handler should expose a database id");
        let groups = runtime.execute(
            &format!("servergroupsbyclientid cldbid={}", self_client_database_id),
            &mut admin,
        );
        assert!(groups.contains(&format!("sgid={server_admin_group_id}")));

        let tokens_after_use = runtime.execute("tokenlist", &mut admin);
        assert!(!tokens_after_use.contains(&token));
    }

    fn promote_web_permission_actor(
        runtime: &mut BaselineRuntime,
        client_database_id: u64,
        client_id: u64,
    ) -> u32 {
        let mut admin = login_query_serveradmin(runtime, client_id);
        let created = runtime.execute(
            "servergroupadd name=BlackTeaWeb\\sPerm\\sAdmin type=1",
            &mut admin,
        );
        let group_id = extract_response_field(&created, "sgid")
            .expect("servergroupadd should expose sgid")
            .parse::<u32>()
            .expect("sgid should parse");

        for command in [
            format!(
                "servergroupaddperm sgid={} permsid=b_permission_modify_power_ignore permvalue=1 permnegated=0 permskip=0",
                group_id
            ),
            format!(
                "servergroupaddperm sgid={} permsid=i_group_modify_power permvalue=100 permnegated=0 permskip=0",
                group_id
            ),
            format!(
                "servergroupaddperm sgid={} permsid=i_permission_modify_power permvalue=100 permnegated=0 permskip=0",
                group_id
            ),
            format!(
                "servergroupaddperm sgid={} permsid=b_virtualserver_servergroup_create permvalue=1 permnegated=0 permskip=0",
                group_id
            ),
            format!(
                "servergroupaddperm sgid={} permsid=b_virtualserver_servergroup_delete permvalue=1 permnegated=0 permskip=0",
                group_id
            ),
            format!(
                "servergroupaddperm sgid={} permsid=b_virtualserver_channelgroup_create permvalue=1 permnegated=0 permskip=0",
                group_id
            ),
            format!(
                "servergroupaddperm sgid={} permsid=b_virtualserver_channelgroup_delete permvalue=1 permnegated=0 permskip=0",
                group_id
            ),
        ] {
            assert!(
                runtime
                    .execute(&command, &mut admin)
                    .contains("error id=0 msg=ok")
            );
        }

        let actor_server_id = admin
            .selected_virtual_server_id
            .expect("query admin should have a selected server");
        let actor_channel_id = admin.current_channel_id.unwrap_or(1);
        let actor_client_database_id = runtime
            .online_client_snapshot(actor_server_id, admin.client_id)
            .expect("query admin should be visible as an online client")
            .database_id;

        assert!(
            runtime
                .web_add_server_group_client(
                    actor_server_id,
                    actor_channel_id,
                    actor_client_database_id,
                    group_id,
                    client_database_id,
                )
                .is_ok()
        );

        group_id
    }

    #[test]
    fn generated_localhost_tls_assets_can_be_loaded_by_rustls() {
        let temp_dir = unique_temp_dir("blackteaweb-tls");
        fs::create_dir_all(&temp_dir).expect("temp dir should be creatable");

        let certificate_path = temp_dir.join("localhost-cert.pem");
        let private_key_path = temp_dir.join("localhost-key.pem");

        generate_localhost_tls_assets(&certificate_path, &private_key_path)
            .expect("localhost TLS assets should be generated");
        load_tls_config_from_files(&certificate_path, &private_key_path)
            .expect("generated TLS assets should load into rustls");

        fs::remove_dir_all(&temp_dir).expect("temp dir should be removable");
    }

    #[test]
    fn team_speak_login_path_emits_initserver() {
        let mut handler = BlackTeaWebSessionHandler::new(7);
        let mut runtime = create_test_runtime("blackteaweb-login-path");
        let (begin, proof, init) = login(&mut handler, &mut runtime);

        assert_eq!(command_name(&begin[0]), "error");
        assert_eq!(command_name(&begin[1]), "handshakeidentityproof");
        assert_eq!(command_name(&proof[0]), "error");
        assert_eq!(command_name(&init[0]), "initserver");
        assert_eq!(command_name(&init[1]), "notifyservergrouplist");
        assert_eq!(command_name(&init[2]), "notifychannelgrouplist");
        assert_eq!(command_name(&init[3]), "notifyclientneededpermissions");
        assert_eq!(command_name(&init[4]), "channellist");
        assert_eq!(command_name(&init[5]), "notifycliententerview");
        assert_eq!(command_name(&init[6]), "channellistfinished");
        assert_eq!(command_name(&init[7]), "error");

        let initserver = parse_frame(&init[0]);
        assert_eq!(initserver["data"][0]["acn"], "Tea Web");
        assert_eq!(initserver["data"][0]["aclid"], "20007");
        assert!(
            initserver["data"][0]["virtualserver_name"]
                .as_str()
                .expect("server name should exist")
                .contains("BlackTeaSpeak Compat")
        );

        let server_groups = parse_frame(&init[1]);
        assert!(server_groups["data"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["sgid"] == "8" && row["name"] == "Normal")
        }));

        let channel_groups = parse_frame(&init[2]);
        assert!(channel_groups["data"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["cgid"] == "10" && row["name"] == "Guest")
        }));

        let needed_permissions = parse_frame(&init[3]);
        assert!(needed_permissions["data"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["permid"] == "178" && row["permvalue"] == "100")
                && rows
                    .iter()
                    .any(|row| row["permid"] == "181" && row["permvalue"] == "100")
        }));

        let channels = parse_frame(&init[4]);
        assert!(
            channels["data"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
        );
        assert!(channels["data"][0].get("cpid").is_some());

        let clients = parse_frame(&init[5]);
        assert!(
            clients["data"]
                .as_array()
                .is_some_and(|rows| rows.len() >= 2)
        );
        assert_eq!(clients["data"][0]["client_type_exact"], "3");
        assert_eq!(clients["data"][0]["client_input_muted"], "0");
    }

    #[test]
    fn connected_session_answers_permission_feature_and_state_queries() {
        let mut handler = BlackTeaWebSessionHandler::new(9);
        let mut runtime = create_test_runtime("blackteaweb-permission-state");
        let _ = login(&mut handler, &mut runtime);

        let permissions = handler
            .handle_text_frame(
                r#"{"type":"command","command":"permissionlist","data":[{"return_code":"4"}]}"#,
                &mut runtime,
            )
            .expect("permissionlist should succeed");
        assert_eq!(command_name(&permissions[0]), "notifypermissionlist");
        assert_eq!(command_name(&permissions[1]), "error");
        assert!(
            parse_frame(&permissions[0])["data"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
        );
        let permission_frame = parse_frame(&permissions[0]);
        let permission_rows = permission_frame["data"]
            .as_array()
            .expect("notifypermissionlist should return rows");
        assert!(permission_rows.iter().any(|row| {
            row.get("permname").and_then(|value| value.as_str())
                == Some("b_serverinstance_help_view")
        }));
        let permission_group_markers = permission_rows
            .iter()
            .filter_map(|row| {
                row.get("group_id_end")
                    .and_then(|value| value.as_str())
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .collect::<Vec<_>>();
        assert!(
            !permission_group_markers.is_empty(),
            "notifypermissionlist should include BlackTeaWeb permission-group separators"
        );
        assert_eq!(
            &permission_group_markers[..26],
            &[
                0, 7, 13, 18, 21, 21, 34, 48, 82, 82, 89, 113, 133, 140, 157, 157, 173, 175, 199,
                201, 201, 275, 303, 323, 342, 360,
            ],
            "notifypermissionlist should mirror BlackTeaWeb's hierarchical base permission groups"
        );

        let features = handler
            .handle_text_frame(
                r#"{"type":"command","command":"listfeaturesupport","data":[{"return_code":"5"}]}"#,
                &mut runtime,
            )
            .expect("listfeaturesupport should succeed");
        assert_eq!(command_name(&features[0]), "notifyfeaturesupport");
        assert_eq!(command_name(&features[1]), "error");
        assert!(
            parse_frame(&features[0])["data"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
        );
        assert!(
            parse_frame(&features[0])["data"].as_array().is_some_and(|rows| rows.iter().any(
                |row| row["name"] == "whisper-echo" && row["support"] == "1" && row["version"] == "1"
            ))
        );
        assert!(
            parse_frame(&features[0])["data"].as_array().is_some_and(|rows| rows.iter().any(
                |row| row["name"] == "video" && row["support"] == "1" && row["version"] == "1"
            ))
        );

        let client_name_from_dbid = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientgetnamefromdbid","data":[{"return_code":"5cnd","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("clientgetnamefromdbid should succeed");
        assert_eq!(
            command_name(&client_name_from_dbid[0]),
            "notifyclientgetnamefromdbid"
        );
        assert_eq!(
            parse_frame(&client_name_from_dbid[0])["data"][0]["cldbid"],
            "40"
        );
        assert_eq!(
            parse_frame(&client_name_from_dbid[0])["data"][0]["cluid"],
            "compat-seed-user-40"
        );
        assert_eq!(
            parse_frame(&client_name_from_dbid[0])["data"][0]["clname"],
            "ScP"
        );
        assert_eq!(command_name(&client_name_from_dbid[1]), "error");

        let client_name_from_uid = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientgetnamefromuid","data":[{"return_code":"5cnu","cluid":"compat-seed-user-40"}]}"#,
                &mut runtime,
            )
            .expect("clientgetnamefromuid should succeed");
        assert_eq!(
            command_name(&client_name_from_uid[0]),
            "notifyclientnamefromuid"
        );
        assert_eq!(
            parse_frame(&client_name_from_uid[0])["data"][0]["cldbid"],
            "40"
        );
        assert_eq!(
            parse_frame(&client_name_from_uid[0])["data"][0]["cluid"],
            "compat-seed-user-40"
        );
        assert_eq!(
            parse_frame(&client_name_from_uid[0])["data"][0]["clname"],
            "ScP"
        );
        assert_eq!(command_name(&client_name_from_uid[1]), "error");

        let client_dbid_from_uid = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientgetdbidfromuid","data":[{"return_code":"5cdu","cluid":"compat-seed-user-40"}]}"#,
                &mut runtime,
            )
            .expect("clientgetdbidfromuid should succeed");
        assert_eq!(
            command_name(&client_dbid_from_uid[0]),
            "notifyclientdbidfromuid"
        );
        assert_eq!(
            parse_frame(&client_dbid_from_uid[0])["data"][0]["cldbid"],
            "40"
        );
        assert_eq!(
            parse_frame(&client_dbid_from_uid[0])["data"][0]["cluid"],
            "compat-seed-user-40"
        );
        assert_eq!(command_name(&client_dbid_from_uid[1]), "error");

        let missing_client_dbid_from_uid = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientgetdbidfromuid","data":[{"return_code":"5cdu-empty","cluid":"missing-uid"}]}"#,
                &mut runtime,
            )
            .expect("clientgetdbidfromuid should answer empty results");
        assert_eq!(command_name(&missing_client_dbid_from_uid[0]), "error");
        assert_eq!(
            parse_frame(&missing_client_dbid_from_uid[0])["data"][0]["id"],
            super::ERROR_DATABASE_EMPTY_RESULT.to_string()
        );

        let whoami = handler
            .handle_text_frame(
                r#"{"type":"command","command":"whoami","data":[{"return_code":"6"}]}"#,
                &mut runtime,
            )
            .expect("whoami should succeed");
        let whoami_frame = parse_frame(&whoami[0]);
        assert_eq!(whoami_frame["type"], "command-raw");
        assert!(
            whoami_frame["payload"]
                .as_str()
                .expect("whoami payload should exist")
                .contains("virtualserver_id=1")
        );
        assert_eq!(command_name(&whoami[1]), "error");

        let server_variables = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"7"}]}"#,
                &mut runtime,
            )
            .expect("servergetvariables should succeed");
        assert_eq!(command_name(&server_variables[0]), "notifyserverupdated");
        assert_eq!(command_name(&server_variables[1]), "error");
        let server_variables_payload = parse_frame(&server_variables[0]);
        assert_eq!(
            server_variables_payload["data"][0]["virtualserver_antiflood_points_tick_reduce"],
            "10"
        );
        assert_eq!(
            server_variables_payload["data"][0]["virtualserver_antiflood_points_needed_command_block"],
            "150"
        );
        assert_eq!(
            server_variables_payload["data"][0]["virtualserver_antiflood_points_needed_ip_block"],
            "250"
        );
        assert_eq!(
            server_variables_payload["data"][0]["virtualserver_antiflood_ban_time"],
            "300"
        );

        let connection_info = handler
            .handle_text_frame(
                r#"{"type":"command","command":"serverrequestconnectioninfo","data":[{"return_code":"8"}]}"#,
                &mut runtime,
            )
            .expect("serverrequestconnectioninfo should succeed");
        assert_eq!(
            command_name(&connection_info[0]),
            "notifyserverconnectioninfo"
        );
        assert_eq!(command_name(&connection_info[1]), "error");
        let info_payload = parse_frame(&connection_info[0]);
        assert!(info_payload["data"][0].get("connection_ping").is_some());

        let server_groups = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergrouplist","data":[{"return_code":"8sg"}]}"#,
                &mut runtime,
            )
            .expect("servergrouplist should succeed");
        assert_eq!(command_name(&server_groups[0]), "notifyservergrouplist");
        assert_eq!(parse_frame(&server_groups[0])["data"][0]["sgid"], "6");
        assert_eq!(command_name(&server_groups[1]), "error");

        let channel_groups = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channelgrouplist","data":[{"return_code":"8cg"}]}"#,
                &mut runtime,
            )
            .expect("channelgrouplist should succeed");
        assert_eq!(command_name(&channel_groups[0]), "notifychannelgrouplist");
        assert_eq!(parse_frame(&channel_groups[0])["data"][0]["cgid"], "8");
        assert_eq!(command_name(&channel_groups[1]), "error");

        let server_group_permissions = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergrouppermlist","data":[{"return_code":"8sgp","sgid":"6"}]}"#,
                &mut runtime,
            )
            .expect("servergrouppermlist should succeed");
        assert_eq!(
            command_name(&server_group_permissions[0]),
            "notifyservergrouppermlist"
        );
        assert_eq!(
            parse_frame(&server_group_permissions[0])["data"][0]["sgid"],
            "6"
        );
        assert_eq!(
            parse_frame(&server_group_permissions[0])["data"][0]["permid"],
            "1"
        );
        assert_eq!(command_name(&server_group_permissions[1]), "error");

        let channel_group_permissions = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channelgrouppermlist","data":[{"return_code":"8cgp","cgid":"8"}]}"#,
                &mut runtime,
            )
            .expect("channelgrouppermlist should succeed");
        assert_eq!(
            command_name(&channel_group_permissions[0]),
            "notifychannelgrouppermlist"
        );
        assert_eq!(
            parse_frame(&channel_group_permissions[0])["data"][0]["cgid"],
            "8"
        );
        assert_eq!(command_name(&channel_group_permissions[1]), "error");

        let server_group_clients = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupclientlist","data":[{"return_code":"8sgc","sgid":"6"}]}"#,
                &mut runtime,
            )
            .expect("servergroupclientlist should succeed");
        assert_eq!(
            command_name(&server_group_clients[0]),
            "notifyservergroupclientlist"
        );
        assert_eq!(
            parse_frame(&server_group_clients[0])["data"][0]["sgid"],
            "6"
        );
        assert_eq!(command_name(&server_group_clients[1]), "error");

        let server_groups_by_client = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupsbyclientid","data":[{"return_code":"8sgbc","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("servergroupsbyclientid should succeed");
        assert_eq!(
            command_name(&server_groups_by_client[0]),
            "notifyservergroupsbyclientid"
        );
        let server_groups_by_client_payload = parse_frame(&server_groups_by_client[0]);
        assert!(
            server_groups_by_client_payload["data"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["cldbid"] == "40"
                    && row["sgid"] == "8"
                    && row["name"] == "Normal"))
        );
        assert_eq!(command_name(&server_groups_by_client[1]), "error");

        let add_server_group_client = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupaddclient","data":[{"return_code":"8sgac","sgid":"7","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("servergroupaddclient should succeed");
        assert_eq!(command_name(&add_server_group_client[0]), "error");
        assert_eq!(
            parse_frame(&add_server_group_client[0])["data"][0]["id"],
            "0"
        );

        let server_groups_by_client_after_add = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupsbyclientid","data":[{"return_code":"8sgbc-add","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("servergroupsbyclientid should include added group");
        assert!(
            parse_frame(&server_groups_by_client_after_add[0])["data"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["sgid"] == "7"))
        );

        let del_server_group_client = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupdelclient","data":[{"return_code":"8sgdc","sgid":"7","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("servergroupdelclient should succeed");
        assert_eq!(command_name(&del_server_group_client[0]), "error");
        assert_eq!(
            parse_frame(&del_server_group_client[0])["data"][0]["id"],
            "0"
        );

        let server_groups_by_client_after_del = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupsbyclientid","data":[{"return_code":"8sgbc-del","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("servergroupsbyclientid should reflect removed group");
        assert!(
            parse_frame(&server_groups_by_client_after_del[0])["data"]
                .as_array()
                .is_some_and(|rows| !rows.iter().any(|row| row["sgid"] == "7"))
        );

        let self_client_database_id = handler
            .self_client_state
            .get("client_database_id")
            .cloned()
            .expect("web client should expose a database id");

        let channel_permissions = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channelpermlist","data":[{"return_code":"8cp","cid":"1"}]}"#,
                &mut runtime,
            )
            .expect("channelpermlist should succeed");
        assert_eq!(
            command_name(&channel_permissions[0]),
            "notifychannelpermlist"
        );
        assert_eq!(parse_frame(&channel_permissions[0])["data"][0]["cid"], "1");
        assert_eq!(command_name(&channel_permissions[1]), "error");

        let client_permissions = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientpermlist","data":[{{"return_code":"8clp","cldbid":"{}"}}]}}"#,
                    self_client_database_id
                ),
                &mut runtime,
            )
            .expect("clientpermlist should succeed");
        assert_eq!(command_name(&client_permissions[0]), "notifyclientpermlist");
        assert_eq!(
            parse_frame(&client_permissions[0])["data"][0]["cldbid"],
            self_client_database_id
        );
        assert_eq!(command_name(&client_permissions[1]), "error");

        let channel_client_permissions = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"channelclientpermlist","data":[{{"return_code":"8ccp","cid":"1","cldbid":"{}"}}]}}"#,
                    self_client_database_id
                ),
                &mut runtime,
            )
            .expect("channelclientpermlist should succeed");
        assert_eq!(
            command_name(&channel_client_permissions[0]),
            "notifychannelclientpermlist"
        );
        let channel_client_permissions_payload = parse_frame(&channel_client_permissions[0]);
        assert_eq!(channel_client_permissions_payload["data"][0]["cid"], "1");
        assert_eq!(
            channel_client_permissions_payload["data"][0]["cldbid"],
            self_client_database_id
        );
        assert_eq!(command_name(&channel_client_permissions[1]), "error");

        let missing_download = handler
            .handle_text_frame(
                r#"{"type":"command","command":"ftinitdownload","data":[{"return_code":"8a","path":"","name":"/avatar_test","cid":"0","clientftfid":"1","seekpos":"0","proto":"1"}]}"#,
                &mut runtime,
            )
            .expect("ftinitdownload should answer with a file error");
        assert_eq!(command_name(&missing_download[0]), "error");
        let download_payload = parse_frame(&missing_download[0]);
        assert_eq!(download_payload["data"][0]["id"], "2051");
        assert_eq!(download_payload["data"][0]["msg"], "file not found");

        let channels = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channellist","data":[{"return_code":"9"}]}"#,
                &mut runtime,
            )
            .expect("channellist should succeed");
        assert_eq!(command_name(&channels[0]), "channellist");
        assert_eq!(command_name(&channels[1]), "channellistfinished");
        assert_eq!(command_name(&channels[2]), "error");

        let clientupdate = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientupdate","data":[{"return_code":"10","client_nickname":"Tea Web Renamed","client_away":"1","client_away_message":"busy","client_input_hardware":"1","client_output_muted":"1","client_flag_avatar":"avatar-hash"}]}"#,
                &mut runtime,
            )
            .expect("clientupdate should succeed");
        assert_eq!(command_name(&clientupdate[0]), "notifyclientupdated");
        let update_payload = parse_frame(&clientupdate[0]);
        assert_eq!(update_payload["data"][0]["clid"], "20009");
        assert_eq!(
            update_payload["data"][0]["client_nickname"],
            "Tea Web Renamed"
        );
        assert_eq!(update_payload["data"][0]["client_away"], "1");
        assert_eq!(update_payload["data"][0]["client_away_message"], "busy");
        assert_eq!(update_payload["data"][0]["client_input_hardware"], "1");
        assert_eq!(update_payload["data"][0]["client_output_muted"], "1");
        assert_eq!(
            update_payload["data"][0]["client_flag_avatar"],
            "avatar-hash"
        );
        assert_eq!(command_name(&clientupdate[1]), "error");
        assert_eq!(parse_frame(&clientupdate[1])["data"][0]["id"], "0");

        let client_variables = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientgetvariables","data":[{"return_code":"10a","clid":"20009"}]}"#,
                &mut runtime,
            )
            .expect("clientgetvariables should succeed");
        assert_eq!(command_name(&client_variables[0]), "notifyclientupdated");
        let variables_payload = parse_frame(&client_variables[0]);
        assert_eq!(variables_payload["data"][0]["clid"], "20009");
        assert_eq!(
            variables_payload["data"][0]["client_nickname"],
            "Tea Web Renamed"
        );
        assert_eq!(variables_payload["data"][0]["client_input_hardware"], "1");
        assert_eq!(variables_payload["data"][0]["client_output_muted"], "1");
        assert_eq!(variables_payload["data"][0]["client_type_exact"], "3");
        assert_eq!(variables_payload["data"][0]["client_totalconnections"], "1");
        assert!(
            variables_payload["data"][0]
                .get("client_channel_group_id")
                .is_some()
        );
        assert!(variables_payload["data"][0].get("client_created").is_some());
        assert!(
            variables_payload["data"][0]
                .get("client_description")
                .is_some()
        );
        assert_eq!(command_name(&client_variables[1]), "error");
        assert_eq!(parse_frame(&client_variables[1])["data"][0]["id"], "0");

        let connection_info = handler
            .handle_text_frame(
                r#"{"type":"command","command":"getconnectioninfo","data":[{"return_code":"10b","clid":"20009"}]}"#,
                &mut runtime,
            )
            .expect("getconnectioninfo should succeed");
        assert_eq!(command_name(&connection_info[0]), "notifyconnectioninfo");
        let client_connection_payload = parse_frame(&connection_info[0]);
        assert_eq!(client_connection_payload["data"][0]["clid"], "20009");
        assert_eq!(
            client_connection_payload["data"][0]["connection_idle_time"],
            "0"
        );
        assert_eq!(client_connection_payload["data"][0]["connection_ping"], "1");
        assert_eq!(command_name(&connection_info[1]), "error");
        assert_eq!(parse_frame(&connection_info[1])["data"][0]["id"], "0");

        let whoami_after_update = handler
            .handle_text_frame(
                r#"{"type":"command","command":"whoami","data":[{"return_code":"10c"}]}"#,
                &mut runtime,
            )
            .expect("whoami after update should succeed");
        assert!(
            parse_frame(&whoami_after_update[0])["payload"]
                .as_str()
                .expect("whoami payload should exist")
                .contains(r"client_nickname=Tea\sWeb\sRenamed")
        );

        let away_clear = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientupdate","data":[{"return_code":"10c","client_away":"0"}]}"#,
                &mut runtime,
            )
            .expect("away clear should succeed");
        let away_clear_payload = parse_frame(&away_clear[0]);
        assert_eq!(away_clear_payload["data"][0]["client_away"], "0");
        assert_eq!(away_clear_payload["data"][0]["client_away_message"], "");

        let subscribe_all = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channelsubscribeall","data":[{"return_code":"11"}]}"#,
                &mut runtime,
            )
            .expect("channelsubscribeall should succeed");
        assert_eq!(command_name(&subscribe_all[0]), "notifychannelsubscribed");
        assert_eq!(parse_frame(&subscribe_all[0])["data"][0]["cid"], "1");
        assert!(subscribe_all.iter().all(|frame| {
            let name = command_name(frame);
            name != "notifychannelhide" && name != "notifychannelshow"
        }));
        assert_eq!(command_name(&subscribe_all[1]), "error");
        assert_eq!(parse_frame(&subscribe_all[1])["data"][0]["id"], "0");

        let unsubscribe_all = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channelunsubscribeall","data":[{"return_code":"11a"}]}"#,
                &mut runtime,
            )
            .expect("channelunsubscribeall should succeed");
        assert_eq!(command_name(&unsubscribe_all[0]), "notifychannelunsubscribed");
        assert_eq!(parse_frame(&unsubscribe_all[0])["data"][0]["cid"], "1");
        assert!(unsubscribe_all.iter().all(|frame| {
            let name = command_name(frame);
            name != "notifychannelhide" && name != "notifychannelshow"
        }));
        assert_eq!(command_name(&unsubscribe_all[1]), "error");
        assert_eq!(parse_frame(&unsubscribe_all[1])["data"][0]["id"], "0");

        let before_move = handler.presence().expect("session should expose presence");
        let move_client = handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientmove","data":[{"return_code":"12","clid":"20009","cid":"2"}]}"#,
                &mut runtime,
            )
            .expect("clientmove should succeed");
        assert_eq!(command_name(&move_client[0]), "error");
        assert_eq!(parse_frame(&move_client[0])["data"][0]["id"], "0");

        let after_move = handler
            .presence()
            .expect("session should expose updated presence");
        let move_frame = derive_direct_frame(&Some(before_move.clone()), &Some(after_move.clone()))
            .expect("move frame should encode")
            .expect("move should emit a direct frame");
        let move_payload = parse_frame(&move_frame);
        assert_eq!(move_payload["command"], "notifyclientmoved");
        assert_eq!(move_payload["data"][0]["clid"], "20009");
        assert_eq!(move_payload["data"][0]["cfid"], "1");
        assert_eq!(move_payload["data"][0]["ctid"], "2");
        assert_eq!(move_payload["data"][0]["reasonid"], "0");

        let whoami_after_move = handler
            .handle_text_frame(
                r#"{"type":"command","command":"whoami","data":[{"return_code":"13"}]}"#,
                &mut runtime,
            )
            .expect("whoami after move should succeed");
        assert!(
            parse_frame(&whoami_after_move[0])["payload"]
                .as_str()
                .expect("whoami payload should exist")
                .contains("client_channel_id=2")
        );

        let channel_description = handler
            .handle_text_frame(
                r#"{"type":"command","command":"channelgetdescription","data":[{"return_code":"14","cid":"1"}]}"#,
                &mut runtime,
            )
            .expect("channelgetdescription should succeed");
        assert_eq!(command_name(&channel_description[0]), "notifychanneldescriptionchanged");
        assert_eq!(command_name(&channel_description[1]), "notifychanneledited");
        assert_eq!(command_name(&channel_description[2]), "error");
        let channel_description_changed_payload = parse_frame(&channel_description[0]);
        assert_eq!(channel_description_changed_payload["data"][0]["cid"], "1");
        let channel_description_payload = parse_frame(&channel_description[1]);
        assert_eq!(channel_description_payload["data"][0]["cid"], "1");
        assert_eq!(
            channel_description_payload["data"][0]["channel_description"],
            ""
        );

        let conversation_fetch = handler
            .handle_text_frame(
                r#"{"type":"command","command":"conversationfetch","data":[{"return_code":"15","cid":"1","cpw":""},{"cid":"2","cpw":""}]}"#,
                &mut runtime,
            )
            .expect("conversationfetch should succeed");
        assert_eq!(
            command_name(&conversation_fetch[0]),
            "notifyconversationindex"
        );
        assert_eq!(command_name(&conversation_fetch[1]), "error");
        let conversation_fetch_payload = parse_frame(&conversation_fetch[0]);
        assert_eq!(conversation_fetch_payload["data"][0]["cid"], "1");
        assert_eq!(conversation_fetch_payload["data"][0]["timestamp"], "0");
        assert_eq!(conversation_fetch_payload["data"][1]["cid"], "2");

        let conversation_history = handler
            .handle_text_frame(
                r#"{"type":"command","command":"conversationhistory","data":[{"return_code":"16","cid":"1","message_count":"50"}]}"#,
                &mut runtime,
            )
            .expect("conversationhistory should succeed");
        assert_eq!(conversation_history.len(), 1);
        assert_eq!(command_name(&conversation_history[0]), "error");
        assert_eq!(parse_frame(&conversation_history[0])["data"][0]["id"], "0");
    }

    #[test]
    fn clientmute_and_clientunmute_are_accepted_as_local_compat_commands() {
        let mut handler = BlackTeaWebSessionHandler::new(81);
        let mut runtime = create_test_runtime("blackteaweb-clientmute-compat");
        let (_, _, init) = login(&mut handler, &mut runtime);

        let init_payload = parse_frame(&init[0]);
        let self_client_id = init_payload["data"][0]["aclid"]
            .as_str()
            .expect("self client id should exist");
        let visible_clients = parse_frame(&init[5]);
        let target_client_id = visible_clients["data"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["clid"] != self_client_id))
            .and_then(|row| row["clid"].as_str())
            .expect("another visible client should exist");

        let before = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientgetvariables","data":[{{"return_code":"cm-before","clid":"{target_client_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("clientgetvariables before mute should succeed");
        let before_payload = parse_frame(&before[0]);

        let muted = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientmute","data":[{{"return_code":"cm-mute","clid":"{target_client_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("clientmute should succeed");
        assert_eq!(muted.len(), 1);
        assert_eq!(command_name(&muted[0]), "error");
        assert_eq!(parse_frame(&muted[0])["data"][0]["id"], "0");

        let unmuted = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientunmute","data":[{{"return_code":"cm-unmute","clid":"{target_client_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("clientunmute should succeed");
        assert_eq!(unmuted.len(), 1);
        assert_eq!(command_name(&unmuted[0]), "error");
        assert_eq!(parse_frame(&unmuted[0])["data"][0]["id"], "0");

        assert!(handler.drain_pending_broadcasts().is_empty());

        let after = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientgetvariables","data":[{{"return_code":"cm-after","clid":"{target_client_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("clientgetvariables after mute should succeed");
        let after_payload = parse_frame(&after[0]);

        assert_eq!(
            before_payload["data"][0]["client_input_muted"],
            after_payload["data"][0]["client_input_muted"]
        );
        assert_eq!(
            before_payload["data"][0]["client_output_muted"],
            after_payload["data"][0]["client_output_muted"]
        );
    }

    #[test]
    fn musicbot_remote_volume_updates_player_volume_and_playerinfo() {
        let mut handler = BlackTeaWebSessionHandler::new(82);
        let mut runtime = create_test_runtime("blackteaweb-musicbot-volume");
        let (_, _, init) = login(&mut handler, &mut runtime);

        let visible_clients = parse_frame(&init[5]);
        let music_bot_row = visible_clients["data"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["client_type_exact"] == "4"))
            .expect("seeded music bot should be visible to BlackTeaWeb");
        let music_bot_client_id = music_bot_row["clid"]
            .as_str()
            .expect("music bot client id should exist");
        let music_bot_database_id = music_bot_row["client_database_id"]
            .as_str()
            .expect("music bot database id should exist");

        let response = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientedit","data":[{{"return_code":"mb-volume","clid":"{music_bot_client_id}","player_volume":"0.65"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("music bot volume edit should succeed");
        assert_eq!(response.len(), 1);
        assert_eq!(command_name(&response[0]), "error");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");

        let broadcasts = handler.drain_pending_broadcasts();
        assert_eq!(broadcasts.len(), 1);
        match &broadcasts[0] {
            BlackTeaWebFrameBroadcast::Server {
                server_id,
                exclude_client_id,
                frame,
            } => {
                assert_eq!(*server_id, 1);
                assert_eq!(*exclude_client_id, None);
                let payload = parse_frame(frame);
                assert_eq!(payload["command"], "notifyclientupdated");
                assert_eq!(payload["data"][0]["clid"], music_bot_client_id);
                assert_eq!(payload["data"][0]["player_volume"], "0.65");
            }
            BlackTeaWebFrameBroadcast::Client { .. } => {
                panic!("music bot volume updates should broadcast on the server path")
            }
        }

        let variables = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientgetvariables","data":[{{"return_code":"mb-vars","clid":"{music_bot_client_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("music bot variables should be readable");
        assert_eq!(command_name(&variables[0]), "notifyclientupdated");
        let variables_payload = parse_frame(&variables[0]);
        assert_eq!(variables_payload["data"][0]["client_type_exact"], "4");
        assert_eq!(variables_payload["data"][0]["player_volume"], "0.65");
        assert_eq!(variables_payload["data"][0]["player_state"], "4");

        let subscription = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"musicbotsetsubscription","data":[{{"return_code":"mb-sub","bot_id":"{music_bot_database_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("music bot subscription should succeed");
        assert!(subscription.len() >= 1);
        let subscription_result = subscription
            .last()
            .expect("music bot subscription should end with an OK frame");
        assert_eq!(command_name(subscription_result), "error");
        assert_eq!(parse_frame(subscription_result)["data"][0]["id"], "0");

        let player_info = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"musicbotplayerinfo","data":[{{"return_code":"mb-info","bot_id":"{music_bot_database_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("music bot player info should succeed");
        assert_eq!(command_name(&player_info[0]), "notifymusicplayerinfo");
        let info_payload = parse_frame(&player_info[0]);
        assert_eq!(info_payload["data"][0]["bot_id"], music_bot_database_id);
        assert_eq!(info_payload["data"][0]["player_volume"], "0.65");
        assert_eq!(info_payload["data"][0]["player_state"], "4");
        assert_eq!(info_payload["data"][0]["song_id"], "0");
        assert_eq!(command_name(&player_info[1]), "error");
        assert_eq!(parse_frame(&player_info[1])["data"][0]["id"], "0");
    }

    #[test]
    fn musicbot_queue_and_playlist_support_webradio_and_current_song_switching() {
        let mut handler = BlackTeaWebSessionHandler::new(182);
        let mut runtime = create_test_runtime("blackteaweb-musicbot-queue-radio");
        let (_, _, init) = login(&mut handler, &mut runtime);

        let visible_clients = parse_frame(&init[5]);
        let music_bot_row = visible_clients["data"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["client_type_exact"] == "4"))
            .expect("seeded music bot should be visible to BlackTeaWeb");
        let music_bot_client_id = music_bot_row["clid"]
            .as_str()
            .expect("music bot client id should exist");
        let music_bot_database_id = music_bot_row["client_database_id"]
            .as_str()
            .expect("music bot database id should exist");

        let quick_radio = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"musicbotqueueadd","data":[{{"return_code":"mb-queue","bot_id":"{music_bot_database_id}","type":"yt","url":"https://streams.example.net/live"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("musicbotqueueadd should succeed");
        assert_eq!(quick_radio.len(), 1);
        assert_eq!(command_name(&quick_radio[0]), "error");
        assert_eq!(parse_frame(&quick_radio[0])["data"][0]["id"], "0");

        let radio_broadcasts = handler.drain_pending_broadcasts();
        let radio_playlist_add = radio_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyplaylistsongadd" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("musicbotqueueadd should broadcast playlist song add");
        let radio_client_update = radio_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyclientupdated" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("musicbotqueueadd should broadcast client update");
        let radio_song_change = radio_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifymusicplayersongchange" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("musicbotqueueadd should broadcast current song change");
        let radio_song_loaded = radio_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyplaylistsongloaded" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("musicbotqueueadd should broadcast loaded song metadata");

        let playlist_id = radio_playlist_add["data"][0]["playlist_id"]
            .as_str()
            .expect("playlist id should exist")
            .to_string();
        let first_song_id = radio_playlist_add["data"][0]["song_id"]
            .as_str()
            .expect("first song id should exist")
            .to_string();
        assert_eq!(radio_playlist_add["data"][0]["song_loaded"], "0");
        assert_eq!(radio_playlist_add["data"][0]["song_url_loader"], "ffmpeg");
        assert_eq!(
            radio_client_update["data"][0]["client_playlist_id"],
            playlist_id
        );
        assert_eq!(radio_client_update["data"][0]["clid"], music_bot_client_id);
        assert_eq!(radio_client_update["data"][0]["player_state"], "2");
        assert_eq!(radio_song_change["data"][0]["bot_id"], music_bot_database_id);
        assert_eq!(
            radio_song_change["data"][0]["song_title"],
            "Webradio streams.example.net"
        );
        assert_eq!(
            radio_song_change["data"][0]["song_url"],
            "https://streams.example.net/live"
        );
        assert_eq!(radio_song_loaded["data"][0]["playlist_id"], playlist_id);
        assert_eq!(radio_song_loaded["data"][0]["song_id"], first_song_id);
        assert_eq!(radio_song_loaded["data"][0]["success"], "1");
        assert!(radio_song_loaded["data"][0]["song_metadata"]
            .as_str()
            .expect("loaded metadata should exist")
            .contains("Webradio streams.example.net"));

        let client_variables = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientgetvariables","data":[{{"return_code":"mb-vars","clid":"{music_bot_client_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("music bot variables should succeed");
        let variables_payload = parse_frame(&client_variables[0]);
        assert_eq!(variables_payload["data"][0]["client_playlist_id"], playlist_id);
        assert_eq!(variables_payload["data"][0]["player_state"], "2");

        let second_song_add = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistsongadd","data":[{{"return_code":"mb-song-add","playlist_id":"{playlist_id}","previous":"{first_song_id}","url":"https://cdn.example.net/archive.mp3","type":"ffmpeg"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistsongadd should succeed");
        assert_eq!(second_song_add.len(), 1);
        assert_eq!(command_name(&second_song_add[0]), "error");
        assert_eq!(parse_frame(&second_song_add[0])["data"][0]["id"], "0");

        let second_add_broadcasts = handler.drain_pending_broadcasts();
        let second_playlist_add = second_add_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyplaylistsongadd" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("playlistsongadd should broadcast playlist update");
        let second_song_loaded = second_add_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyplaylistsongloaded" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("playlistsongadd should broadcast loaded metadata");
        let second_song_id = second_playlist_add["data"][0]["song_id"]
            .as_str()
            .expect("second song id should exist")
            .to_string();
        assert_eq!(second_playlist_add["data"][0]["song_loaded"], "0");
        assert_eq!(
            second_playlist_add["data"][0]["song_previous_song_id"],
            first_song_id
        );
        assert_eq!(
            second_playlist_add["data"][0]["song_url"],
            "https://cdn.example.net/archive.mp3"
        );
        assert_eq!(second_song_loaded["data"][0]["song_id"], second_song_id);
        assert_eq!(second_song_loaded["data"][0]["success"], "1");
        assert!(second_song_loaded["data"][0]["song_metadata"]
            .as_str()
            .expect("second loaded metadata should exist")
            .contains("archive"));

        let playlist_info = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistinfo","data":[{{"return_code":"mb-playlist-info","playlist_id":"{playlist_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistinfo should succeed");
        assert_eq!(command_name(&playlist_info[0]), "notifyplaylistinfo");
        let playlist_info_payload = parse_frame(&playlist_info[0]);
        assert_eq!(playlist_info_payload["data"][0]["playlist_id"], playlist_id);
        assert_eq!(
            playlist_info_payload["data"][0]["playlist_current_song_id"],
            first_song_id
        );

        let playlist_song_list = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistsonglist","data":[{{"return_code":"mb-song-list","playlist_id":"{playlist_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistsonglist should succeed");
        assert_eq!(command_name(&playlist_song_list[0]), "notifyplaylistsonglist");
        let list_payload = parse_frame(&playlist_song_list[0]);
        assert_eq!(
            list_payload["data"].as_array().map(|rows| rows.len()),
            Some(2)
        );
        assert_eq!(list_payload["data"][0]["song_id"], first_song_id);
        assert_eq!(list_payload["data"][1]["song_id"], second_song_id);
        assert_eq!(
            list_payload["data"][1]["song_previous_song_id"],
            first_song_id
        );

        let set_current = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistsongsetcurrent","data":[{{"return_code":"mb-set-current","playlist_id":"{playlist_id}","song_id":"{second_song_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistsongsetcurrent should succeed");
        assert_eq!(set_current.len(), 1);
        assert_eq!(command_name(&set_current[0]), "error");
        assert_eq!(parse_frame(&set_current[0])["data"][0]["id"], "0");

        let set_current_broadcasts = handler.drain_pending_broadcasts();
        let current_song_change = set_current_broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifymusicplayersongchange" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("playlistsongsetcurrent should broadcast song change");
        assert_eq!(current_song_change["data"][0]["song_id"], second_song_id);
        assert_eq!(
            current_song_change["data"][0]["song_url"],
            "https://cdn.example.net/archive.mp3"
        );

        let player_info = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"musicbotplayerinfo","data":[{{"return_code":"mb-player-info","bot_id":"{music_bot_database_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("musicbotplayerinfo should succeed");
        assert_eq!(command_name(&player_info[0]), "notifymusicplayerinfo");
        let player_info_payload = parse_frame(&player_info[0]);
        assert_eq!(player_info_payload["data"][0]["bot_id"], music_bot_database_id);
        assert_eq!(player_info_payload["data"][0]["song_id"], second_song_id);
        assert_eq!(
            player_info_payload["data"][0]["song_title"],
            "archive"
        );
        assert_eq!(player_info_payload["data"][0]["player_state"], "2");
        let replay_index = player_info_payload["data"][0]["player_replay_index"]
            .as_str()
            .and_then(|value| value.parse::<u32>().ok())
            .expect("player_replay_index should parse");
        let buffered_index = player_info_payload["data"][0]["player_buffered_index"]
            .as_str()
            .and_then(|value| value.parse::<u32>().ok())
            .expect("player_buffered_index should parse");
        assert!(buffered_index >= replay_index);
    }

    #[test]
    fn playlist_permission_commands_roundtrip_for_web_actor() {
        let mut handler = BlackTeaWebSessionHandler::new(183);
        let mut runtime = create_test_runtime("blackteaweb-playlist-permissions");
        let (_, _, init) = login(&mut handler, &mut runtime);

        let _visible_clients = parse_frame(&init[5]);
        let playlist_list = handler
            .handle_text_frame(
                r#"{"type":"command","command":"playlistlist","data":[{"return_code":"pp-playlists"}]}"#,
                &mut runtime,
            )
            .expect("playlistlist should succeed");
        assert_eq!(command_name(&playlist_list[0]), "notifyplaylistlist");
        let playlist_list_payload = parse_frame(&playlist_list[0]);
        let playlist_id = playlist_list_payload["data"][0]["playlist_id"]
            .as_str()
            .expect("playlist id should exist")
            .to_string();
        let self_client_database_id = handler
            .self_client_database_id()
            .expect("logged in BlackTeaWeb client should expose client dbid");

        let edit = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistedit","data":[{{"return_code":"pp-edit","playlist_id":"{playlist_id}","playlist_flag_finished":"1","playlist_replay_mode":"2","playlist_max_songs":"33"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistedit should succeed");
        let edit_result = parse_frame(edit.last().expect("playlistedit should return a result"));
        assert_eq!(edit_result["command"], "error");
        assert_eq!(edit_result["data"][0]["id"], "0");

        let playlist_add_perm = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistaddperm","data":[{{"return_code":"pp-add","playlist_id":"{playlist_id}","permsid":"i_playlist_permission_modify_power","permvalue":"42","permnegated":"0","permskip":"0"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistaddperm should succeed");
        let playlist_add_perm_result = parse_frame(
            playlist_add_perm
                .last()
                .expect("playlistaddperm should return a result"),
        );
        assert_eq!(playlist_add_perm_result["command"], "error");
        assert_eq!(playlist_add_perm_result["data"][0]["id"], "0");

        let playlist_perm_list = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistpermlist","data":[{{"return_code":"pp-list","playlist_id":"{playlist_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistpermlist should succeed");
        assert_eq!(command_name(&playlist_perm_list[0]), "notifyplaylistpermlist");
        assert_eq!(playlist_perm_list.len(), 2);
        let playlist_perm_payload = parse_frame(&playlist_perm_list[0]);
        assert_eq!(playlist_perm_payload["data"][0]["playlist_id"], playlist_id);
        assert_eq!(playlist_perm_payload["data"][0]["permvalue"], "42");

        let playlist_client_add_perm = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistclientaddperm","data":[{{"return_code":"pp-client-add","playlist_id":"{playlist_id}","cldbid":"{self_client_database_id}","permsid":"i_playlist_delete_power","permvalue":"21","permnegated":"0","permskip":"0"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistclientaddperm should succeed");
        let playlist_client_add_perm_result = parse_frame(
            playlist_client_add_perm
                .last()
                .expect("playlistclientaddperm should return a result"),
        );
        assert_eq!(playlist_client_add_perm_result["command"], "error");
        assert_eq!(playlist_client_add_perm_result["data"][0]["id"], "0");

        let playlist_client_perm_list = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistclientpermlist","data":[{{"return_code":"pp-client-list","playlist_id":"{playlist_id}","cldbid":"{self_client_database_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistclientpermlist should succeed");
        assert_eq!(
            command_name(&playlist_client_perm_list[0]),
            "notifyplaylistclientpermlist"
        );
        let playlist_client_perm_payload = parse_frame(&playlist_client_perm_list[0]);
        assert_eq!(
            playlist_client_perm_payload["data"][0]["playlist_id"],
            playlist_id
        );
        assert_eq!(
            playlist_client_perm_payload["data"][0]["cldbid"],
            self_client_database_id.to_string()
        );
        assert_eq!(playlist_client_perm_payload["data"][0]["permvalue"], "21");

        let playlist_client_list = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistclientlist","data":[{{"return_code":"pp-clients","playlist_id":"{playlist_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistclientlist should succeed");
        assert_eq!(command_name(&playlist_client_list[0]), "notifyplaylistclientlist");
        let playlist_client_payload = parse_frame(&playlist_client_list[0]);
        assert_eq!(playlist_client_payload["data"][0]["playlist_id"], playlist_id);
        assert_eq!(
            playlist_client_payload["data"][0]["cldbid"],
            self_client_database_id.to_string()
        );

        let playlist_info = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistinfo","data":[{{"return_code":"pp-info","playlist_id":"{playlist_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistinfo should succeed");
        let playlist_info_payload = parse_frame(&playlist_info[0]);
        assert_eq!(playlist_info_payload["data"][0]["playlist_flag_finished"], "1");
        assert_eq!(playlist_info_payload["data"][0]["playlist_replay_mode"], "2");
        assert_eq!(playlist_info_payload["data"][0]["playlist_max_songs"], "33");
    }

    #[test]
    fn channel_loader_song_add_broadcasts_loaded_metadata_for_blackteaweb() {
        let mut handler = BlackTeaWebSessionHandler::new(184);
        let mut runtime = create_test_runtime("blackteaweb-channel-loader-song");
        let (_, _, init) = login(&mut handler, &mut runtime);

        let _visible_clients = parse_frame(&init[5]);
        let playlist_list = handler
            .handle_text_frame(
                r#"{"type":"command","command":"playlistlist","data":[{"return_code":"pl-playlists"}]}"#,
                &mut runtime,
            )
            .expect("playlistlist should succeed");
        assert_eq!(command_name(&playlist_list[0]), "notifyplaylistlist");
        let playlist_list_payload = parse_frame(&playlist_list[0]);
        let playlist_id = playlist_list_payload["data"][0]["playlist_id"]
            .as_str()
            .expect("playlist id should exist")
            .to_string();

        let add_song = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistsongadd","data":[{{"return_code":"pl-channel-add","playlist_id":"{playlist_id}","url":"channel://1/smoke-upload.txt","type":"channel"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("channel playlistsongadd should succeed");
        assert_eq!(add_song.len(), 1);
        assert_eq!(command_name(&add_song[0]), "error");
        assert_eq!(parse_frame(&add_song[0])["data"][0]["id"], "0");

        let broadcasts = handler.drain_pending_broadcasts();
        let playlist_add = broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyplaylistsongadd" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("channel playlistsongadd should broadcast playlist add");
        let playlist_loaded = broadcasts
            .iter()
            .find_map(|broadcast| match broadcast {
                BlackTeaWebFrameBroadcast::Server { frame, .. }
                    if parse_frame(frame)["command"] == "notifyplaylistsongloaded" =>
                {
                    Some(parse_frame(frame))
                }
                _ => None,
            })
            .expect("channel playlistsongadd should broadcast loaded metadata");
        assert_eq!(playlist_add["data"][0]["playlist_id"], playlist_id);
        assert_eq!(playlist_add["data"][0]["song_url_loader"], "channel");
        assert_eq!(playlist_add["data"][0]["song_loaded"], "0");
        assert_eq!(playlist_loaded["data"][0]["playlist_id"], playlist_id);
        assert_eq!(playlist_loaded["data"][0]["success"], "1");
        assert!(playlist_loaded["data"][0]["song_metadata"]
            .as_str()
            .expect("channel loaded metadata should exist")
            .contains("smoke upload"));

        let playlist_song_list = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"playlistsonglist","data":[{{"return_code":"pl-channel-list","playlist_id":"{playlist_id}"}}]}}"#
                ),
                &mut runtime,
            )
            .expect("playlistsonglist should succeed after channel add");
        assert_eq!(command_name(&playlist_song_list[0]), "notifyplaylistsonglist");
        let playlist_song_list_payload = parse_frame(&playlist_song_list[0]);
        assert_eq!(playlist_song_list_payload["data"][0]["song_url_loader"], "channel");
        assert_eq!(playlist_song_list_payload["data"][0]["song_loaded"], "1");
        assert!(playlist_song_list_payload["data"][0]["song_metadata"]
            .as_str()
            .expect("persisted song metadata should exist")
            .contains("smoke upload"));
    }

    #[test]
    fn musicbotcreate_and_musicbotdelete_emit_presence_updates() {
        let mut runtime = create_test_runtime("blackteaweb-musicbot-lifecycle");

        let mut creator = BlackTeaWebSessionHandler::new(83);
        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));
        attach_test_realtime_support(&mut creator, Arc::clone(&sessions));

        let _ = login_with_identity(
            &mut creator,
            &mut runtime,
            "compat-musicbot-lifecycle-creator",
            "BotCreator",
        );
        let creator_pending = register_test_session(&sessions, &creator, &runtime);

        let create = creator
            .handle_text_frame(
                r#"{"type":"command","command":"musicbotcreate","data":[{"return_code":"mb-create","cid":"2"}]}"#,
                &mut runtime,
            )
            .expect("musicbotcreate should succeed");
        let create_result = create.last().expect("musicbotcreate should end with OK");
        assert_eq!(command_name(create_result), "error");
        assert_eq!(parse_frame(create_result)["data"][0]["id"], "0");

        let created_broadcasts = creator.drain_pending_broadcasts();
        assert_eq!(created_broadcasts.len(), 1);
        broadcast_queued_frames(&sessions, &created_broadcasts)
            .expect("musicbotcreate broadcasts should reach registered sessions");
        let created_frames = drain_test_frames(&creator_pending);
        assert_eq!(created_frames.len(), 1);
        let created = parse_frame(&created_frames[0]);
        assert_eq!(created["command"], "notifycliententerview");
        assert_eq!(created["data"][0]["ctid"], "2");
        assert_eq!(created["data"][0]["client_type_exact"], "4");
        let bot_dbid = created["data"][0]["client_database_id"]
            .as_str()
            .expect("music bot dbid should exist");
        let bot_clid = created["data"][0]["clid"]
            .as_str()
            .expect("music bot clid should exist")
            .to_string();

        let delete = creator
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"musicbotdelete","data":[{{"return_code":"mb-delete","botid":"{}"}}]}}"#,
                    bot_dbid,
                ),
                &mut runtime,
            )
            .expect("musicbotdelete should succeed");
        let delete_result = delete.last().expect("musicbotdelete should end with OK");
        assert_eq!(command_name(delete_result), "error");
        assert_eq!(parse_frame(delete_result)["data"][0]["id"], "0");

        let deleted_broadcasts = creator.drain_pending_broadcasts();
        assert!(!deleted_broadcasts.is_empty());
        broadcast_queued_frames(&sessions, &deleted_broadcasts)
            .expect("musicbotdelete broadcasts should reach registered sessions");
        let deleted_frames = drain_test_frames(&creator_pending);
        let deleted = deleted_frames
            .iter()
            .map(|frame| parse_frame(frame))
            .find(|payload| payload["command"] == "notifyclientleftview")
            .expect("musicbotdelete should include a notifyclientleftview frame");
        assert_eq!(deleted["command"], "notifyclientleftview");
        assert_eq!(deleted["data"][0]["clid"], bot_clid);
        assert_eq!(deleted["data"][0]["reasonmsg"], "music bot deleted");
        assert_eq!(deleted["data"][0]["ctid"], "0");
    }

    #[test]
    fn temporary_channel_cleanup_despawns_lone_musicbot_and_deletes_channel() {
        let mut runtime = create_test_runtime("blackteaweb-musicbot-temp-cleanup");
        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));

        let mut creator = BlackTeaWebSessionHandler::new(85);
        attach_test_realtime_support(&mut creator, Arc::clone(&sessions));

        let _ = login_with_identity(
            &mut creator,
            &mut runtime,
            "compat-musicbot-temp-creator",
            "TempCreator",
        );
        register_test_session(&sessions, &creator, &runtime);

        let mut admin = login_query_serveradmin(&mut runtime, 90085);
        let created_channel = runtime.execute("channelcreate channel_name=Bot\\sTemp channel_flag_temporary=1", &mut admin);
        let temporary_channel_id = extract_response_field(&created_channel, "cid")
            .expect("channelcreate should expose cid");

        creator
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientmove","data":[{{"return_code":"temp-move-in","cid":"{}"}}]}}"#,
                    temporary_channel_id,
                ),
                &mut runtime,
            )
            .expect("creator should move into temporary channel");
        assert!(creator.drain_pending_broadcasts().is_empty());
        let mut creator_pending = register_test_session(&sessions, &creator, &runtime);

        creator
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"musicbotcreate","data":[{{"return_code":"temp-bot-create","cid":"{}"}}]}}"#,
                    temporary_channel_id,
                ),
                &mut runtime,
            )
            .expect("musicbotcreate in temporary channel should succeed");
        let created_bot_broadcasts = creator.drain_pending_broadcasts();
        assert_eq!(created_bot_broadcasts.len(), 1);
        broadcast_queued_frames(&sessions, &created_bot_broadcasts)
            .expect("musicbotcreate temp broadcasts should reach registered sessions");
        let created_bot_frames = drain_test_frames(&creator_pending);
        assert_eq!(created_bot_frames.len(), 1);
        let created_bot = parse_frame(&created_bot_frames[0]);
        assert_eq!(created_bot["command"], "notifycliententerview");
        assert_eq!(created_bot["data"][0]["ctid"], temporary_channel_id);
        let bot_clid = created_bot["data"][0]["clid"]
            .as_str()
            .expect("music bot clid should exist")
            .to_string();

        creator
            .handle_text_frame(
                r#"{"type":"command","command":"clientmove","data":[{"return_code":"temp-move-out","cid":"1"}]}"#,
                &mut runtime,
            )
            .expect("creator should move out of temporary channel");
        creator_pending = register_test_session(&sessions, &creator, &runtime);

        let cleanup_broadcasts = creator.drain_pending_broadcasts();
        assert!(!cleanup_broadcasts.is_empty());
        broadcast_queued_frames(&sessions, &cleanup_broadcasts)
            .expect("temporary cleanup broadcasts should reach registered sessions");
        let cleanup_frames = drain_test_frames(&creator_pending);

        let bot_left = cleanup_frames
            .iter()
            .map(|frame| parse_frame(frame))
            .find(|payload| payload["command"] == "notifyclientleftview")
            .expect("cleanup should include a clientleftview frame");
        assert_eq!(bot_left["command"], "notifyclientleftview");
        assert_eq!(bot_left["data"][0]["clid"], bot_clid);
        assert_eq!(bot_left["data"][0]["reasonmsg"], "temporary channel cleanup");

        let channel_deleted = cleanup_frames
            .iter()
            .map(|frame| parse_frame(frame))
            .find(|payload| payload["command"] == "notifychanneldeleted")
            .expect("cleanup should include a channeldeleted frame");
        assert_eq!(channel_deleted["command"], "notifychanneldeleted");
        assert_eq!(channel_deleted["data"][0]["cid"], temporary_channel_id);
        assert!(runtime
            .snapshot_channel(1, temporary_channel_id.parse::<u32>().expect("cid should parse"))
            .is_none());
    }

    #[test]
    fn servergroup_assignment_denials_report_failed_permission() {
        let mut handler = BlackTeaWebSessionHandler::new(18);
        let mut runtime = create_test_runtime("blackteaweb-servergroup-permission-denial");
        let _ = login(&mut handler, &mut runtime);

        let mut query_session = login_query_serveradmin(&mut runtime, 90018);
        assert!(runtime
            .execute(
                "servergroupaddperm sgid=8 permsid=i_server_group_member_add_power permvalue=0 permnegated=0 permskip=0|permsid=i_group_member_add_power permvalue=0 permnegated=0 permskip=0",
                &mut query_session,
            )
            .contains("error id=0 msg=ok"));

        let denied = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupaddclient","data":[{"return_code":"8sgac-denied","sgid":"6","cldbid":"40"}]}"#,
                &mut runtime,
            )
            .expect("servergroupaddclient should return a permission error");

        assert_eq!(denied.len(), 1);
        assert_eq!(command_name(&denied[0]), "error");
        let denied_payload = parse_frame(&denied[0]);
        assert_eq!(denied_payload["data"][0]["id"], "2568");
        assert!(denied_payload["data"][0].get("failed_permid").is_some());
    }

    #[test]
    fn sendtextmessage_updates_blackteaweb_history_and_broadcast_queue() {
        let mut handler = BlackTeaWebSessionHandler::new(10);
        let mut runtime = create_test_runtime("blackteaweb-sendtextmessage-history");
        let _ = login(&mut handler, &mut runtime);

        let send = handler
            .handle_text_frame(
                r#"{"type":"command","command":"sendtextmessage","data":[{"return_code":"17","targetmode":"2","cid":"1","target":"1","msg":"BlackTeaWeb hello"}]}"#,
                &mut runtime,
            )
            .expect("sendtextmessage should succeed");
        assert_eq!(command_name(&send[0]), "notifytextmessage");
        assert_eq!(command_name(&send[1]), "error");
        let send_payload = parse_frame(&send[0]);
        assert_eq!(send_payload["data"][0]["cid"], "1");
        assert_eq!(send_payload["data"][0]["msg"], "BlackTeaWeb hello");
        assert_eq!(send_payload["data"][0]["invokerid"], "20010");

        let queued = handler.drain_pending_broadcasts();
        assert_eq!(queued.len(), 1);
        match &queued[0] {
            BlackTeaWebFrameBroadcast::Server {
                server_id,
                exclude_client_id,
                ..
            } => {
                assert_eq!(*server_id, 1);
                assert_eq!(*exclude_client_id, Some(20010));
            }
            BlackTeaWebFrameBroadcast::Client { .. } => {
                panic!("channel messages should broadcast to the server session set")
            }
        }

        let fetch = handler
            .handle_text_frame(
                r#"{"type":"command","command":"conversationfetch","data":[{"return_code":"18","cid":"1","cpw":""}]}"#,
                &mut runtime,
            )
            .expect("conversationfetch should succeed after a message");
        assert_eq!(command_name(&fetch[0]), "notifyconversationindex");
        assert_ne!(parse_frame(&fetch[0])["data"][0]["timestamp"], "0");

        let history = handler
            .handle_text_frame(
                r#"{"type":"command","command":"conversationhistory","data":[{"return_code":"19","cid":"1","message_count":"10"}]}"#,
                &mut runtime,
            )
            .expect("conversationhistory should return persisted BlackTeaWeb messages");
        assert_eq!(command_name(&history[0]), "notifyconversationhistory");
        assert_eq!(command_name(&history[1]), "error");
        let history_payload = parse_frame(&history[0]);
        assert_eq!(history_payload["data"][0]["msg"], "BlackTeaWeb hello");
        assert_eq!(history_payload["data"][0]["sender_name"], "Tea Web");
    }

    #[test]
    fn sendtextmessage_queues_query_notifications_for_all_target_modes() {
        let mut runtime = create_test_runtime("blackteaweb-query-notification-targetmodes");
        let mut sender = BlackTeaWebSessionHandler::new(40);
        let mut target = BlackTeaWebSessionHandler::new(41);

        let _ = login_with_identity(
            &mut sender,
            &mut runtime,
            "compat-public-key-sender",
            "BrowserSender",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-target",
            "BrowserTarget",
        );

        let sender_uid = sender
            .self_client_state
            .get("client_unique_identifier")
            .cloned()
            .expect("sender uid should be set");

        sender
            .handle_text_frame(
                r#"{"type":"command","command":"sendtextmessage","data":[{"return_code":"41","targetmode":"2","cid":"1","msg":"BlackTeaWeb channel to Query"}]}"#,
                &mut runtime,
            )
            .expect("channel sendtextmessage should succeed");
        sender
            .handle_text_frame(
                r#"{"type":"command","command":"sendtextmessage","data":[{"return_code":"42","targetmode":"3","msg":"BlackTeaWeb server to Query"}]}"#,
                &mut runtime,
            )
            .expect("server sendtextmessage should succeed");
        sender
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"sendtextmessage","data":[{{"return_code":"43","targetmode":"1","target":"{}","msg":"BlackTeaWeb private to Query"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("private sendtextmessage should succeed");

        let broadcasts = sender.drain_pending_broadcasts();
        assert_eq!(broadcasts.len(), 3);
        match &broadcasts[0] {
            BlackTeaWebFrameBroadcast::Server {
                server_id,
                exclude_client_id,
                frame,
            } => {
                assert_eq!(*server_id, 1);
                assert_eq!(*exclude_client_id, Some(sender.client_id));
                assert_text_frame(
                    frame,
                    "2",
                    "BlackTeaWeb channel to Query",
                    "BrowserSender",
                    &sender_uid,
                );
            }
            BlackTeaWebFrameBroadcast::Client { .. } => {
                panic!("channel message should broadcast to server sessions")
            }
        }
        match &broadcasts[1] {
            BlackTeaWebFrameBroadcast::Server {
                server_id,
                exclude_client_id,
                frame,
            } => {
                assert_eq!(*server_id, 1);
                assert_eq!(*exclude_client_id, Some(sender.client_id));
                assert_text_frame(
                    frame,
                    "3",
                    "BlackTeaWeb server to Query",
                    "BrowserSender",
                    &sender_uid,
                );
            }
            BlackTeaWebFrameBroadcast::Client { .. } => {
                panic!("server message should broadcast to server sessions")
            }
        }
        match &broadcasts[2] {
            BlackTeaWebFrameBroadcast::Client { client_id, frame } => {
                assert_eq!(*client_id, target.client_id);
                assert_text_frame(
                    frame,
                    "1",
                    "BlackTeaWeb private to Query",
                    "BrowserSender",
                    &sender_uid,
                );
            }
            BlackTeaWebFrameBroadcast::Server { .. } => {
                panic!("private message should target a single session")
            }
        }

        let query_notifications = sender.drain_pending_query_notifications();
        assert_eq!(query_notifications.len(), 3);
        assert_text_notification(
            &query_notifications[0],
            2,
            Some(1),
            None,
            "BlackTeaWeb channel to Query",
            sender.client_id,
            "BrowserSender",
            &sender_uid,
        );
        assert_text_notification(
            &query_notifications[1],
            3,
            None,
            None,
            "BlackTeaWeb server to Query",
            sender.client_id,
            "BrowserSender",
            &sender_uid,
        );
        assert_text_notification(
            &query_notifications[2],
            1,
            None,
            Some(target.client_id),
            "BlackTeaWeb private to Query",
            sender.client_id,
            "BrowserSender",
            &sender_uid,
        );
    }

    #[test]
    fn clientpoke_targets_single_session_and_query_notification() {
        let mut runtime = create_test_runtime("blackteaweb-clientpoke-targeting");
        let mut sender = BlackTeaWebSessionHandler::new(52);
        let mut target = BlackTeaWebSessionHandler::new(53);

        let _ = login_with_identity(
            &mut sender,
            &mut runtime,
            "compat-public-key-poke-sender",
            "BrowserPokeSender",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-poke-target",
            "BrowserPokeTarget",
        );

        let response = sender
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientpoke","data":[{{"return_code":"51","clid":"{}","msg":"Wake up"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("clientpoke should succeed");
        assert_eq!(response.len(), 1);
        assert_eq!(command_name(&response[0]), "error");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");

        let broadcasts = sender.drain_pending_broadcasts();
        assert_eq!(broadcasts.len(), 1);
        match &broadcasts[0] {
            BlackTeaWebFrameBroadcast::Client { client_id, frame } => {
                assert_eq!(*client_id, target.client_id);
                let payload = parse_frame(frame);
                assert_eq!(payload["command"], "notifyclientpoke");
                assert_eq!(payload["data"][0]["invokerid"], sender.client_id.to_string());
                assert_eq!(payload["data"][0]["invokername"], "BrowserPokeSender");
                assert_eq!(payload["data"][0]["msg"], "Wake up");
            }
            BlackTeaWebFrameBroadcast::Server { .. } => {
                panic!("clientpoke should target a single session")
            }
        }

        let notifications = sender.drain_pending_query_notifications();
        assert_eq!(notifications.len(), 1);
        match &notifications[0] {
            TransportNotification::ClientPoke {
                server_id,
                target_client_id,
                invoker_id,
                invoker_name,
                message,
                ..
            } => {
                assert_eq!(*server_id, 1);
                assert_eq!(*target_client_id, target.client_id);
                assert_eq!(*invoker_id, sender.client_id);
                assert_eq!(invoker_name, "BrowserPokeSender");
                assert_eq!(message, "Wake up");
            }
            _ => panic!("expected client poke notification"),
        }
    }

    #[test]
    fn clientmove_moves_other_client_and_queues_notifications() {
        let mut runtime = create_test_runtime("blackteaweb-clientmove-other");
        let mut actor = BlackTeaWebSessionHandler::new(54);
        let mut target = BlackTeaWebSessionHandler::new(55);

        let _ = login_with_identity(
            &mut actor,
            &mut runtime,
            "compat-public-key-move-actor",
            "BrowserMoveActor",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-move-target",
            "BrowserMoveTarget",
        );

        let actor_dbid = actor
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("actor dbid should be numeric");
        add_server_group_to_client(&mut runtime, 9054, 6, actor_dbid);

        let response = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientmove","data":[{{"return_code":"54","clid":"{}","cid":"2"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("moving another client should succeed");
        assert_eq!(command_name(&response[0]), "error");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");

        let moved_target = runtime
            .online_client_snapshot(1, target.client_id)
            .expect("moved target should stay online");
        assert_eq!(moved_target.channel_id, 2);

        let broadcasts = actor.drain_pending_broadcasts();
        assert_eq!(broadcasts.len(), 2);
        match &broadcasts[0] {
            BlackTeaWebFrameBroadcast::Client { client_id, frame } => {
                assert_eq!(*client_id, target.client_id);
                let payload = parse_frame(frame);
                assert_eq!(payload["command"], "notifyclientmoved");
                assert_eq!(payload["data"][0]["clid"], target.client_id.to_string());
                assert_eq!(payload["data"][0]["cfid"], "1");
                assert_eq!(payload["data"][0]["ctid"], "2");
                assert_eq!(payload["data"][0]["reasonid"], "1");
                assert_eq!(payload["data"][0]["invokerid"], actor.client_id.to_string());
            }
            BlackTeaWebFrameBroadcast::Server { .. } => panic!("first broadcast should target the moved client"),
        }
        match &broadcasts[1] {
            BlackTeaWebFrameBroadcast::Server {
                server_id,
                exclude_client_id,
                frame,
            } => {
                assert_eq!(*server_id, 1);
                assert_eq!(*exclude_client_id, Some(target.client_id));
                assert_eq!(parse_frame(frame)["command"], "notifyclientmoved");
            }
            BlackTeaWebFrameBroadcast::Client { .. } => panic!("second broadcast should target peers"),
        }

        let notifications = actor.drain_pending_query_notifications();
        assert_eq!(notifications.len(), 1);
        match &notifications[0] {
            TransportNotification::ClientMoved {
                presence,
                from_channel_id,
                reason_id,
                invoker_id,
                ..
            } => {
                assert_eq!(presence.client_id, target.client_id);
                assert_eq!(presence.channel_id, 2);
                assert_eq!(*from_channel_id, 1);
                assert_eq!(*reason_id, 1);
                assert_eq!(*invoker_id, actor.client_id);
            }
            _ => panic!("expected moved notification"),
        }
    }

    #[test]
    fn clientkick_channel_kick_moves_target_and_queues_notifications() {
        let mut runtime = create_test_runtime("blackteaweb-clientkick-channel");
        let mut actor = BlackTeaWebSessionHandler::new(56);
        let mut target = BlackTeaWebSessionHandler::new(57);

        let _ = login_with_identity(
            &mut actor,
            &mut runtime,
            "compat-public-key-kick-actor",
            "BrowserKickActor",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-kick-target",
            "BrowserKickTarget",
        );
        target
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientmove","data":[{{"return_code":"57m","clid":"{}","cid":"2"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("target self-move should succeed");

        let actor_dbid = actor
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("actor dbid should be numeric");
        add_server_group_to_client(&mut runtime, 9056, 6, actor_dbid);

        let response = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientkick","data":[{{"return_code":"56","clid":"{}","reasonid":"4","reasonmsg":"channel kicked"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("channel kick should succeed");
        assert_eq!(command_name(&response[0]), "error");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");

        let kicked_target = runtime
            .online_client_snapshot(1, target.client_id)
            .expect("channel-kicked target should stay online");
        assert_eq!(kicked_target.channel_id, 1);

        let broadcasts = actor.drain_pending_broadcasts();
        assert_eq!(broadcasts.len(), 2);
        let target_payload = match &broadcasts[0] {
            BlackTeaWebFrameBroadcast::Client { client_id, frame } => {
                assert_eq!(*client_id, target.client_id);
                parse_frame(frame)
            }
            BlackTeaWebFrameBroadcast::Server { .. } => panic!("first broadcast should target the kicked client"),
        };
        assert_eq!(target_payload["command"], "notifyclientmoved");
        assert_eq!(target_payload["data"][0]["reasonid"], "4");
        assert_eq!(target_payload["data"][0]["cfid"], "2");
        assert_eq!(target_payload["data"][0]["ctid"], "1");

        let notifications = actor.drain_pending_query_notifications();
        assert_eq!(notifications.len(), 1);
        match &notifications[0] {
            TransportNotification::ClientMoved {
                from_channel_id,
                reason_id,
                ..
            } => {
                assert_eq!(*from_channel_id, 2);
                assert_eq!(*reason_id, 4);
            }
            _ => panic!("expected moved notification for channel kick"),
        }
    }

    #[test]
    fn clientkick_server_kick_removes_target_and_queues_left_view() {
        let mut runtime = create_test_runtime("blackteaweb-clientkick-server");
        let mut actor = BlackTeaWebSessionHandler::new(58);
        let mut target = BlackTeaWebSessionHandler::new(59);

        let _ = login_with_identity(
            &mut actor,
            &mut runtime,
            "compat-public-key-serverkick-actor",
            "BrowserServerKickActor",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-serverkick-target",
            "BrowserServerKickTarget",
        );

        let actor_dbid = actor
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("actor dbid should be numeric");
        add_server_group_to_client(&mut runtime, 9058, 6, actor_dbid);

        let response = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientkick","data":[{{"return_code":"58","clid":"{}","reasonid":"5","reasonmsg":"server kicked"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("server kick should succeed");
        assert_eq!(command_name(&response[0]), "error");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");
        assert!(runtime.online_client_snapshot(1, target.client_id).is_none());

        let broadcasts = actor.drain_pending_broadcasts();
        assert_eq!(broadcasts.len(), 2);
        let target_payload = match &broadcasts[0] {
            BlackTeaWebFrameBroadcast::Client { client_id, frame } => {
                assert_eq!(*client_id, target.client_id);
                parse_frame(frame)
            }
            BlackTeaWebFrameBroadcast::Server { .. } => panic!("first broadcast should target the kicked client"),
        };
        assert_eq!(target_payload["command"], "notifyclientleftview");
        assert_eq!(target_payload["data"][0]["reasonid"], "5");
        assert_eq!(target_payload["data"][0]["invokerid"], actor.client_id.to_string());

        let notifications = actor.drain_pending_query_notifications();
        assert_eq!(notifications.len(), 1);
        match &notifications[0] {
            TransportNotification::ClientLeftView {
                reason_id,
                invoker_id,
                ban_time,
                ..
            } => {
                assert_eq!(*reason_id, 5);
                assert_eq!(*invoker_id, actor.client_id);
                assert_eq!(*ban_time, None);
            }
            _ => panic!("expected left-view notification for server kick"),
        }
    }

    #[test]
    fn banclient_blocks_reconnect_with_3329() {
        let mut runtime = create_test_runtime("blackteaweb-banclient-reconnect");
        let mut actor = BlackTeaWebSessionHandler::new(60);
        let mut target = BlackTeaWebSessionHandler::new(61);

        let _ = login_with_identity(
            &mut actor,
            &mut runtime,
            "compat-public-key-ban-actor",
            "BrowserBanActor",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-ban-target",
            "BrowserBanTarget",
        );

        let actor_dbid = actor
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("actor dbid should be numeric");
        add_server_group_to_client(&mut runtime, 9060, 6, actor_dbid);

        let response = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"banclient","data":[{{"return_code":"60","clid":"{}","time":"60","banreason":"ban hammer"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("banclient should succeed");
        assert_eq!(command_name(&response[0]), "error");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");
        assert!(runtime.online_client_snapshot(1, target.client_id).is_none());

        let notifications = actor.drain_pending_query_notifications();
        assert_eq!(notifications.len(), 1);
        match &notifications[0] {
            TransportNotification::ClientLeftView {
                reason_id,
                ban_time,
                ..
            } => {
                assert_eq!(*reason_id, 6);
                assert_eq!(*ban_time, Some(60));
            }
            _ => panic!("expected ban left-view notification"),
        }

        let mut reconnect = BlackTeaWebSessionHandler::new(62);
        reconnect
            .handle_text_frame(r#"{"type":"enable-raw-commands"}"#, &mut runtime)
            .expect("enable-raw-commands should succeed");
        reconnect
            .handle_text_frame(
                r#"{"type":"command","command":"handshakebegin","data":[{"return_code":"1","intention":0,"authentication_method":1,"publicKey":"compat-public-key-ban-target"}]}"#,
                &mut runtime,
            )
            .expect("handshakebegin should succeed");
        reconnect
            .handle_text_frame(
                r#"{"type":"command","command":"handshakeindentityproof","data":[{"return_code":"2","proof":"signed-proof"}]}"#,
                &mut runtime,
            )
            .expect("identity proof should succeed");
        let reconnect_init = reconnect
            .handle_text_frame(
                r#"{"type":"command","command":"clientinit","data":[{"return_code":"3","client_nickname":"BrowserBanTarget","client_server_password":"","client_default_channel":"/"}]}"#,
                &mut runtime,
            )
            .expect("clientinit should answer with a ban error");
        assert_eq!(command_name(&reconnect_init[0]), "error");
        assert_eq!(parse_frame(&reconnect_init[0])["data"][0]["id"], "3329");
    }

    #[test]
    fn clientupdate_notifies_peer_sessions_and_updates_runtime_state() {
        let mut runtime = create_test_runtime("blackteaweb-clientupdate-peers");
        let mut alpha = BlackTeaWebSessionHandler::new(45);
        let mut beta = BlackTeaWebSessionHandler::new(46);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-update-alpha",
            "UpdateAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-update-beta",
            "UpdateBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);

        let before_presence = alpha.presence();
        let response = alpha
            .handle_text_frame(
                r#"{"type":"command","command":"clientupdate","data":[{"return_code":"45","client_nickname":"Update Alpha Renamed","client_away":"1","client_away_message":"busy","client_output_muted":"1","client_flag_avatar":"avatar-hash"}]}"#,
                &mut runtime,
            )
            .expect("clientupdate should succeed");
        assert_eq!(command_name(&response[0]), "notifyclientupdated");
        assert_eq!(command_name(&response[1]), "error");
        let self_payload = parse_frame(&response[0]);
        assert_eq!(self_payload["data"][0]["clid"], "20045");
        assert_eq!(
            self_payload["data"][0]["client_nickname"],
            "Update Alpha Renamed"
        );
        assert_eq!(self_payload["data"][0]["client_away"], "1");
        assert_eq!(self_payload["data"][0]["client_away_message"], "busy");
        assert_eq!(self_payload["data"][0]["client_output_muted"], "1");
        assert_eq!(self_payload["data"][0]["client_flag_avatar"], "avatar-hash");

        let after_presence = alpha.presence();
        let peer_frames = derive_peer_frames(&before_presence, &after_presence)
            .expect("peer update frames should encode");
        assert_eq!(peer_frames.len(), 1);
        broadcast_frames_for_presence_change(&sessions, &peer_frames)
            .expect("peer update should broadcast");

        assert!(drain_test_frames(&alpha_pending).is_empty());
        let beta_frames = drain_test_frames(&beta_pending);
        assert_eq!(beta_frames.len(), 1);
        let beta_payload = parse_frame(&beta_frames[0]);
        assert_eq!(beta_payload["command"], "notifyclientupdated");
        assert_eq!(beta_payload["data"][0]["clid"], "20045");
        assert_eq!(
            beta_payload["data"][0]["client_nickname"],
            "Update Alpha Renamed"
        );
        assert_eq!(beta_payload["data"][0]["client_away"], "1");
        assert_eq!(beta_payload["data"][0]["client_away_message"], "busy");
        assert_eq!(beta_payload["data"][0]["client_output_muted"], "1");
        assert_eq!(beta_payload["data"][0]["client_flag_avatar"], "avatar-hash");

        let updated_client = runtime
            .online_client_snapshot(1, alpha.client_id)
            .expect("runtime should keep updated web client state");
        assert_eq!(updated_client.nickname, "Update Alpha Renamed");
    }

    #[test]
    fn query_clientupdate_notifications_bridge_into_blackteaweb_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-query-clientupdate-bridge");
        let mut alpha = BlackTeaWebSessionHandler::new(47);
        let mut beta = BlackTeaWebSessionHandler::new(48);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-query-update-alpha",
            "QueryUpdateAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-query-update-beta",
            "QueryUpdateBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let mut query_session = login_query_serveradmin(&mut runtime, 451);
        let before_snapshot = runtime
            .online_client_snapshot(1, query_session.client_id)
            .expect("query client should be online before clientupdate");
        assert!(
            runtime
                .execute(
                    r"clientupdate client_nickname=Query\sBridge client_away=1 client_away_message=Bridge\sBusy client_input_muted=1 client_output_muted=1",
                    &mut query_session,
                )
                .contains("error id=0 msg=ok")
        );
        let after_snapshot = runtime
            .online_client_snapshot(1, query_session.client_id)
            .expect("query client should stay online after clientupdate");

        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ClientUpdated {
                    server_id: 1,
                    before: before_snapshot,
                    after: after_snapshot,
                }],
            )
            .expect("query clientupdate should bridge into BlackTeaWeb");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        for frames in [&alpha_frames, &beta_frames] {
            assert_eq!(frames.len(), 1);
            let payload = parse_frame(&frames[0]);
            assert_eq!(payload["command"], "notifyclientupdated");
            assert_eq!(
                payload["data"][0]["clid"],
                query_session.client_id.to_string()
            );
            assert_eq!(payload["data"][0]["client_nickname"], "Query Bridge");
            assert_eq!(payload["data"][0]["client_away"], "1");
            assert_eq!(payload["data"][0]["client_away_message"], "Bridge Busy");
            assert_eq!(payload["data"][0]["client_input_muted"], "1");
            assert_eq!(payload["data"][0]["client_output_muted"], "1");
        }
    }

    #[test]
    fn query_serveredit_notifications_bridge_into_blackteaweb_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-query-serveredit-bridge");
        let mut alpha = BlackTeaWebSessionHandler::new(49);
        let mut beta = BlackTeaWebSessionHandler::new(50);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-query-server-alpha",
            "QueryServerAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-query-server-beta",
            "QueryServerBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let mut query_session = login_query_serveradmin(&mut runtime, 452);
        let before_snapshot = runtime
            .snapshot_server(1)
            .expect("server snapshot should exist before serveredit");
        assert!(
            runtime
                .execute(
                    r"serveredit virtualserver_name=Bridge\sServer virtualserver_welcomemessage=Bridge\sWelcome virtualserver_hostmessage=Bridge\sHost virtualserver_hostmessage_mode=2 virtualserver_ask_for_privilegekey=1 virtualserver_maxclients=64 virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=3 virtualserver_antiflood_points_needed_ip_block=5 virtualserver_antiflood_ban_time=60",
                    &mut query_session,
                )
                .contains("error id=0 msg=ok")
        );
        let after_snapshot = runtime
            .snapshot_server(1)
            .expect("server snapshot should exist after serveredit");

        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ServerEdited {
                    server_id: 1,
                    before: before_snapshot,
                    after: after_snapshot,
                    invoker_id: query_session.client_id,
                    invoker_name: query_session
                        .authenticated_login
                        .clone()
                        .expect("query session should be authenticated"),
                }],
            )
            .expect("query serveredit should bridge into BlackTeaWeb");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        for frames in [&alpha_frames, &beta_frames] {
            assert_eq!(frames.len(), 1);
            let payload = parse_frame(&frames[0]);
            assert_eq!(payload["command"], "notifyserveredited");
            assert_eq!(payload["data"][0]["virtualserver_id"], "1");
            assert_eq!(payload["data"][0]["virtualserver_name"], "Bridge Server");
            assert_eq!(
                payload["data"][0]["virtualserver_welcomemessage"],
                "Bridge Welcome"
            );
            assert_eq!(
                payload["data"][0]["virtualserver_hostmessage"],
                "Bridge Host"
            );
            assert_eq!(payload["data"][0]["virtualserver_hostmessage_mode"], "2");
            assert_eq!(
                payload["data"][0]["virtualserver_ask_for_privilegekey"],
                "1"
            );
            assert_eq!(payload["data"][0]["virtualserver_maxclients"], "64");
            assert_eq!(
                payload["data"][0]["virtualserver_antiflood_points_tick_reduce"],
                "0"
            );
            assert_eq!(
                payload["data"][0]["virtualserver_antiflood_points_needed_command_block"],
                "3"
            );
            assert_eq!(
                payload["data"][0]["virtualserver_antiflood_points_needed_ip_block"],
                "5"
            );
            assert_eq!(payload["data"][0]["virtualserver_antiflood_ban_time"], "60");
        }

        let fetched = alpha
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"49"}]}"#,
                &mut runtime,
            )
            .expect("servergetvariables should reflect the edited server state");
        assert_eq!(command_name(&fetched[0]), "notifyserverupdated");
        let fetched_payload = parse_frame(&fetched[0]);
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_name"],
            "Bridge Server"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_welcomemessage"],
            "Bridge Welcome"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_hostmessage"],
            "Bridge Host"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_hostmessage_mode"],
            "2"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_ask_for_privilegekey"],
            "1"
        );
        assert_eq!(fetched_payload["data"][0]["virtualserver_maxclients"], "64");
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_antiflood_points_tick_reduce"],
            "0"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_antiflood_points_needed_command_block"],
            "3"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_antiflood_points_needed_ip_block"],
            "5"
        );
        assert_eq!(
            fetched_payload["data"][0]["virtualserver_antiflood_ban_time"],
            "60"
        );
    }

    #[test]
    fn connected_session_applies_antiflood_limits() {
        let mut runtime = create_test_runtime("blackteaweb-antiflood");
        let mut admin_session = login_query_serveradmin(&mut runtime, 91);
        assert!(
            runtime
                .execute(
                    "serveredit virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=2 virtualserver_antiflood_points_needed_ip_block=4 virtualserver_antiflood_ban_time=60",
                    &mut admin_session,
                )
                .contains("error id=0 msg=ok")
        );

        let mut handler = BlackTeaWebSessionHandler::new(64);
        let _ = login(&mut handler, &mut runtime);

        let allowed = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"af1"}]}"#,
                &mut runtime,
            )
            .expect("first servergetvariables should succeed");
        assert_eq!(command_name(&allowed[0]), "notifyserverupdated");

        let blocked = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"af2"}]}"#,
                &mut runtime,
            )
            .expect("second servergetvariables should produce a flood error frame");
        assert_eq!(command_name(&blocked[0]), "error");
        let blocked_payload = parse_frame(&blocked[0]);
        assert_eq!(
            blocked_payload["data"][0]["id"],
            crate::runtime::ERROR_CLIENT_IS_FLOODING.to_string()
        );
        assert_eq!(blocked_payload["data"][0]["msg"], "client is flooding");
    }

    #[test]
    fn connected_sessions_share_antiflood_ip_blocks() {
        let mut runtime = create_test_runtime("blackteaweb-antiflood-shared-ip");
        let mut admin_session = login_query_serveradmin(&mut runtime, 92);
        assert!(
            runtime
                .execute(
                    "serveredit virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=100 virtualserver_antiflood_points_needed_ip_block=4 virtualserver_antiflood_ban_time=60",
                    &mut admin_session,
                )
                .contains("error id=0 msg=ok")
        );

        let mut same_a =
            BlackTeaWebSessionHandler::new_with_connection_ip(65, String::from("198.51.100.10"));
        let mut same_b =
            BlackTeaWebSessionHandler::new_with_connection_ip(66, String::from("198.51.100.10"));
        let mut isolated =
            BlackTeaWebSessionHandler::new_with_connection_ip(67, String::from("198.51.100.11"));

        let _ = login_with_identity(&mut same_a, &mut runtime, "compat-public-key-ip-a", "TeaA");
        let _ = login_with_identity(&mut same_b, &mut runtime, "compat-public-key-ip-b", "TeaB");
        let _ = login_with_identity(
            &mut isolated,
            &mut runtime,
            "compat-public-key-ip-c",
            "TeaIso",
        );

        let mut same_a_blocked = false;
        let mut same_b_blocked = false;
        for attempt in 0..10 {
            let allowed_or_blocked_a = same_a
                .handle_text_frame(
                    &format!(
                        r#"{{"type":"command","command":"servergetvariables","data":[{{"return_code":"shared-a-{attempt}"}}]}}"#
                    ),
                    &mut runtime,
                )
                .expect("shared_a servergetvariables should produce a frame");
            if command_name(&allowed_or_blocked_a[0]) == "error" {
                same_a_blocked = true;
                break;
            }
            assert_eq!(
                command_name(&allowed_or_blocked_a[0]),
                "notifyserverupdated"
            );

            let allowed_or_blocked_b = same_b
                .handle_text_frame(
                    &format!(
                        r#"{{"type":"command","command":"servergetvariables","data":[{{"return_code":"shared-b-{attempt}"}}]}}"#
                    ),
                    &mut runtime,
                )
                .expect("shared_b servergetvariables should produce a frame");
            if command_name(&allowed_or_blocked_b[0]) == "error" {
                same_b_blocked = true;
                break;
            }
            assert_eq!(
                command_name(&allowed_or_blocked_b[0]),
                "notifyserverupdated"
            );
        }

        assert!(
            same_a_blocked || same_b_blocked,
            "same-IP BlackTeaWeb sessions should eventually trigger a shared flood block"
        );

        let isolated_allowed = isolated
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"shared-iso"}]}"#,
                &mut runtime,
            )
            .expect("isolated servergetvariables should still succeed");
        assert_eq!(command_name(&isolated_allowed[0]), "notifyserverupdated");

        let partner_blocked = if same_a_blocked {
            same_b
                .handle_text_frame(
                    r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"shared-b-partner"}]}"#,
                    &mut runtime,
                )
                .expect("same_b follow-up should produce a frame")
        } else {
            same_a
                .handle_text_frame(
                    r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"shared-a-partner"}]}"#,
                    &mut runtime,
                )
                .expect("same_a follow-up should produce a frame")
        };
        assert_eq!(command_name(&partner_blocked[0]), "error");
        let blocked_payload = parse_frame(&partner_blocked[0]);
        assert_eq!(
            blocked_payload["data"][0]["id"],
            crate::runtime::ERROR_CLIENT_IS_FLOODING.to_string()
        );
        assert_eq!(blocked_payload["data"][0]["msg"], "client is flooding");
    }

    #[test]
    fn loopback_sessions_do_not_share_antiflood_ip_blocks() {
        let mut runtime = create_test_runtime("blackteaweb-antiflood-loopback");
        let mut admin_session = login_query_serveradmin(&mut runtime, 93);
        assert!(
            runtime
                .execute(
                    "serveredit virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=100 virtualserver_antiflood_points_needed_ip_block=4 virtualserver_antiflood_ban_time=60",
                    &mut admin_session,
                )
                .contains("error id=0 msg=ok")
        );

        let mut same_a =
            BlackTeaWebSessionHandler::new_with_connection_ip(68, String::from("127.0.0.1"));
        let mut same_b =
            BlackTeaWebSessionHandler::new_with_connection_ip(69, String::from("127.0.0.1"));

        let _ = login_with_identity(
            &mut same_a,
            &mut runtime,
            "compat-public-key-loopback-a",
            "TeaLoopA",
        );
        let _ = login_with_identity(
            &mut same_b,
            &mut runtime,
            "compat-public-key-loopback-b",
            "TeaLoopB",
        );

        for attempt in 0..3 {
            let allowed_a = same_a
                .handle_text_frame(
                    &format!(
                        r#"{{"type":"command","command":"servergetvariables","data":[{{"return_code":"loop-a-{attempt}"}}]}}"#
                    ),
                    &mut runtime,
                )
                .expect("loopback session A should stay below per-session antiflood");
            assert_eq!(command_name(&allowed_a[0]), "notifyserverupdated");

            let allowed_b = same_b
                .handle_text_frame(
                    &format!(
                        r#"{{"type":"command","command":"servergetvariables","data":[{{"return_code":"loop-b-{attempt}"}}]}}"#
                    ),
                    &mut runtime,
                )
                .expect("loopback session B should stay below per-session antiflood");
            assert_eq!(command_name(&allowed_b[0]), "notifyserverupdated");
        }
    }

    #[test]
    fn blackteaweb_presence_changes_derive_query_join_move_leave_notifications() {
        let mut runtime = create_test_runtime("blackteaweb-presence-query-bridge");
        let mut handler = BlackTeaWebSessionHandler::new(44);

        let before_connect = handler.presence();
        let _ = login_with_identity(
            &mut handler,
            &mut runtime,
            "compat-public-key-presence",
            "BrowserPresence",
        );
        let after_connect = handler.presence();

        let join_notifications =
            derive_query_notifications_from_presence(&before_connect, &after_connect);
        assert_eq!(join_notifications.len(), 1);
        match &join_notifications[0] {
            TransportNotification::ClientEnterView {
                presence,
                from_channel_id,
                reason_id,
            } => {
                assert_eq!(presence.client_id, handler.client_id);
                assert_eq!(presence.login_name, "BrowserPresence");
                assert_eq!(presence.server_id, 1);
                assert_eq!(presence.channel_id, 1);
                assert_eq!(
                    presence.unique_identifier,
                    handler
                        .self_client_state
                        .get("client_unique_identifier")
                        .cloned()
                        .expect("handler uid should exist"),
                );
                assert_eq!(*from_channel_id, None);
                assert_eq!(*reason_id, 0);
            }
            _ => panic!("expected join notification"),
        }

        let before_move = handler.presence();
        handler
            .handle_text_frame(
                r#"{"type":"command","command":"clientmove","data":[{"return_code":"44b","clid":"20044","cid":"2"}]}"#,
                &mut runtime,
            )
            .expect("clientmove should succeed");
        let after_move = handler.presence();

        let move_notifications =
            derive_query_notifications_from_presence(&before_move, &after_move);
        assert_eq!(move_notifications.len(), 2);
        match &move_notifications[0] {
            TransportNotification::ClientLeftView {
                presence,
                to_channel_id,
                reason_id,
                reason_message,
                ..
            } => {
                assert_eq!(presence.client_id, handler.client_id);
                assert_eq!(presence.channel_id, 1);
                assert_eq!(*to_channel_id, Some(2));
                assert_eq!(*reason_id, 0);
                assert_eq!(reason_message, "changed channel");
            }
            _ => panic!("expected leave notification for move"),
        }
        match &move_notifications[1] {
            TransportNotification::ClientEnterView {
                presence,
                from_channel_id,
                reason_id,
            } => {
                assert_eq!(presence.client_id, handler.client_id);
                assert_eq!(presence.channel_id, 2);
                assert_eq!(*from_channel_id, Some(1));
                assert_eq!(*reason_id, 0);
            }
            _ => panic!("expected enter notification for move"),
        }

        let disconnect_notifications = derive_query_notifications_from_presence(&after_move, &None);
        assert_eq!(disconnect_notifications.len(), 1);
        match &disconnect_notifications[0] {
            TransportNotification::ClientLeftView {
                presence,
                to_channel_id,
                reason_id,
                reason_message,
                ..
            } => {
                assert_eq!(presence.client_id, handler.client_id);
                assert_eq!(presence.channel_id, 2);
                assert_eq!(*to_channel_id, None);
                assert_eq!(*reason_id, 8);
                assert_eq!(reason_message, "left server");
            }
            _ => panic!("expected disconnect notification"),
        }
    }

    #[test]
    fn privateconversationhistory_uses_stable_web_identity() {
        let mut runtime = create_test_runtime("blackteaweb-private-history");
        let mut sender = BlackTeaWebSessionHandler::new(20);
        let mut target = BlackTeaWebSessionHandler::new(21);

        let _ = login_with_identity(
            &mut sender,
            &mut runtime,
            "compat-public-key-a",
            "Tea Web A",
        );
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-b",
            "Tea Web B",
        );

        let sender_uid = sender
            .self_client_state
            .get("client_unique_identifier")
            .cloned()
            .expect("sender uid should be set");
        assert_eq!(
            sender_uid,
            stable_web_client_unique_identifier("compat-public-key-a")
        );
        assert_eq!(
            sender
                .self_client_state
                .get("client_database_id")
                .and_then(|value| value.parse::<u64>().ok())
                .expect("sender dbid should be numeric"),
            stable_web_client_database_id(&sender_uid)
        );

        let send = sender
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"sendtextmessage","data":[{{"return_code":"31","targetmode":"1","target":"{}","msg":"BlackTeaWeb private"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("private sendtextmessage should succeed");
        assert_eq!(command_name(&send[0]), "notifytextmessage");
        assert_eq!(parse_frame(&send[0])["data"][0]["targetmode"], "1");

        let history = target
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"privateconversationhistory","data":[{{"return_code":"32","cluid":"{}","message_count":"10"}}]}}"#,
                    sender_uid,
                ),
                &mut runtime,
            )
            .expect("privateconversationhistory should succeed");
        assert_eq!(
            command_name(&history[0]),
            "notifyprivateconversationhistory"
        );
        assert_eq!(command_name(&history[1]), "error");
        let history_payload = parse_frame(&history[0]);
        assert_eq!(history_payload["data"][0]["cluid"], sender_uid);
        assert_eq!(history_payload["data"][0]["sender_name"], "Tea Web A");
        assert_eq!(history_payload["data"][0]["msg"], "BlackTeaWeb private");
    }

    #[test]
    fn query_presence_notifications_bridge_into_blackteaweb_sessions_for_join_move_leave() {
        let mut runtime = create_test_runtime("blackteaweb-query-presence-bridge");
        let mut alpha = BlackTeaWebSessionHandler::new(52);
        let mut beta = BlackTeaWebSessionHandler::new(53);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-presence-alpha",
            "PresenceAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-presence-beta",
            "PresenceBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let mut query_session = login_query_serveradmin(&mut runtime, 501);
        let enter_presence = query_presence(&runtime, &query_session);

        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ClientEnterView {
                    presence: enter_presence.clone(),
                    from_channel_id: None,
                    reason_id: 0,
                }],
            )
            .expect("join presence should bridge into BlackTeaWeb");

        let alpha_enter = drain_test_frames(&alpha_pending);
        let beta_enter = drain_test_frames(&beta_pending);
        for frames in [&alpha_enter, &beta_enter] {
            assert_eq!(frames.len(), 1);
            let payload = parse_frame(&frames[0]);
            assert_eq!(payload["command"], "notifycliententerview");
            assert_eq!(payload["data"][0]["clid"], "501");
            assert_eq!(payload["data"][0]["client_nickname"], "serveradmin");
            assert_eq!(payload["data"][0]["cfid"], "0");
            assert_eq!(payload["data"][0]["ctid"], "1");
            assert_eq!(payload["data"][0]["reasonid"], "0");
            assert_eq!(payload["data"][0]["client_type_exact"], "1");
        }

        let before_move = enter_presence.clone();
        assert!(
            runtime
                .execute("clientmove cid=2", &mut query_session)
                .contains("error id=0 msg=ok")
        );
        let after_move = query_presence(&runtime, &query_session);
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[
                    TransportNotification::ClientLeftView {
                        presence: before_move,
                        to_channel_id: Some(after_move.channel_id),
                        reason_id: 0,
                        reason_message: String::from("changed channel"),
                        invoker_id: 501,
                        invoker_name: String::from("serveradmin"),
                        invoker_uid: runtime.query_session_unique_identifier(&query_session),
                        ban_time: None,
                    },
                    TransportNotification::ClientEnterView {
                        presence: after_move.clone(),
                        from_channel_id: Some(1),
                        reason_id: 0,
                    },
                ],
            )
            .expect("move presence should bridge into BlackTeaWeb");

        let alpha_move = drain_test_frames(&alpha_pending);
        let beta_move = drain_test_frames(&beta_pending);
        for frames in [&alpha_move, &beta_move] {
            assert_eq!(frames.len(), 2);
            let left = parse_frame(&frames[0]);
            assert_eq!(left["command"], "notifyclientleftview");
            assert_eq!(left["data"][0]["clid"], "501");
            assert_eq!(left["data"][0]["cfid"], "1");
            assert_eq!(left["data"][0]["ctid"], "2");
            assert_eq!(left["data"][0]["reasonid"], "0");

            let enter = parse_frame(&frames[1]);
            assert_eq!(enter["command"], "notifycliententerview");
            assert_eq!(enter["data"][0]["clid"], "501");
            assert_eq!(enter["data"][0]["cfid"], "1");
            assert_eq!(enter["data"][0]["ctid"], "2");
            assert_eq!(enter["data"][0]["reasonid"], "0");
        }

        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ClientLeftView {
                    presence: after_move,
                    to_channel_id: None,
                    reason_id: 8,
                    reason_message: String::from("left server"),
                    invoker_id: 0,
                    invoker_name: String::new(),
                    invoker_uid: String::new(),
                    ban_time: None,
                }],
            )
            .expect("disconnect presence should bridge into BlackTeaWeb");

        let alpha_leave = drain_test_frames(&alpha_pending);
        let beta_leave = drain_test_frames(&beta_pending);
        for frames in [&alpha_leave, &beta_leave] {
            assert_eq!(frames.len(), 1);
            let payload = parse_frame(&frames[0]);
            assert_eq!(payload["command"], "notifyclientleftview");
            assert_eq!(payload["data"][0]["clid"], "501");
            assert_eq!(payload["data"][0]["cfid"], "2");
            assert_eq!(payload["data"][0]["ctid"], "0");
            assert_eq!(payload["data"][0]["reasonid"], "8");
            assert_eq!(payload["data"][0]["reasonmsg"], "left server");
        }
    }

    #[test]
    fn query_text_notifications_bridge_into_blackteaweb_sessions_for_all_target_modes() {
        let mut runtime = create_test_runtime("blackteaweb-query-text-bridge");
        let mut alpha = BlackTeaWebSessionHandler::new(50);
        let mut beta = BlackTeaWebSessionHandler::new(51);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-alpha",
            "BrowserAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-beta",
            "BrowserBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(1),
                &[
                    TransportNotification::TextMessage {
                        target: TextMessageTarget {
                            target_mode: 2,
                            server_id: 1,
                            channel_id: Some(1),
                            target_client_id: None,
                            message: String::from("Query channel bridge"),
                        },
                        invoker_id: 1,
                        invoker_name: String::from("serveradmin"),
                        invoker_uid: String::from("serveradmin"),
                    },
                    TransportNotification::TextMessage {
                        target: TextMessageTarget {
                            target_mode: 3,
                            server_id: 1,
                            channel_id: None,
                            target_client_id: None,
                            message: String::from("Query server bridge"),
                        },
                        invoker_id: 1,
                        invoker_name: String::from("serveradmin"),
                        invoker_uid: String::from("serveradmin"),
                    },
                    TransportNotification::TextMessage {
                        target: TextMessageTarget {
                            target_mode: 1,
                            server_id: 1,
                            channel_id: None,
                            target_client_id: Some(beta.client_id),
                            message: String::from("Query private bridge"),
                        },
                        invoker_id: 1,
                        invoker_name: String::from("serveradmin"),
                        invoker_uid: String::from("serveradmin"),
                    },
                ],
            )
            .expect("query bridge should broadcast text notifications");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        assert_eq!(alpha_frames.len(), 2);
        assert_eq!(beta_frames.len(), 3);

        assert_text_frame(
            &alpha_frames[0],
            "2",
            "Query channel bridge",
            "serveradmin",
            "serveradmin",
        );
        assert_text_frame(
            &alpha_frames[1],
            "3",
            "Query server bridge",
            "serveradmin",
            "serveradmin",
        );
        assert_text_frame(
            &beta_frames[0],
            "2",
            "Query channel bridge",
            "serveradmin",
            "serveradmin",
        );
        assert_text_frame(
            &beta_frames[1],
            "3",
            "Query server bridge",
            "serveradmin",
            "serveradmin",
        );
        assert_text_frame(
            &beta_frames[2],
            "1",
            "Query private bridge",
            "serveradmin",
            "serveradmin",
        );
    }

    #[test]
    fn query_clientpoke_notifications_bridge_into_blackteaweb_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-query-clientpoke-bridge");
        let mut alpha = BlackTeaWebSessionHandler::new(56);
        let mut beta = BlackTeaWebSessionHandler::new(57);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-poke-alpha",
            "PokeAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-poke-beta",
            "PokeBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let query_session = login_query_serveradmin(&mut runtime, 561);
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ClientPoke {
                    server_id: 1,
                    target_client_id: beta.client_id,
                    invoker_id: query_session.client_id,
                    invoker_name: String::from("serveradmin"),
                    invoker_uid: runtime.query_session_unique_identifier(&query_session),
                    message: String::from("Bridge poke"),
                }],
            )
            .expect("query bridge should deliver clientpoke notifications");

        let alpha_frames = drain_test_frames(&alpha_pending);
        assert!(alpha_frames.is_empty());

        let beta_frames = drain_test_frames(&beta_pending);
        assert_eq!(beta_frames.len(), 1);
        let payload = parse_frame(&beta_frames[0]);
        assert_eq!(payload["command"], "notifyclientpoke");
        assert_eq!(payload["data"][0]["invokerid"], query_session.client_id.to_string());
        assert_eq!(payload["data"][0]["invokername"], "serveradmin");
        assert_eq!(payload["data"][0]["msg"], "Bridge poke");
    }

    #[test]
    fn query_clientmove_notifications_bridge_into_target_and_peer_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-query-clientmove-bridge");
        let mut target = BlackTeaWebSessionHandler::new(63);
        let mut viewer = BlackTeaWebSessionHandler::new(64);

        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-bridge-move-target",
            "BridgeMoveTarget",
        );
        let _ = login_with_identity(
            &mut viewer,
            &mut runtime,
            "compat-public-key-bridge-move-viewer",
            "BridgeMoveViewer",
        );
        target
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientmove","data":[{{"return_code":"64m","clid":"{}","cid":"2"}}]}}"#,
                    target.client_id,
                ),
                &mut runtime,
            )
            .expect("target self-move should succeed");

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let target_pending = register_test_session(&sessions, &target, &runtime);
        let viewer_pending = register_test_session(&sessions, &viewer, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let query_session = login_query_serveradmin(&mut runtime, 563);
        let moved_presence = session_presence_from_blackteaweb_presence(
            &target.presence().expect("target should expose moved presence"),
        );
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ClientMoved {
                    presence: moved_presence,
                    from_channel_id: 1,
                    reason_id: 1,
                    reason_message: String::from("Bridge move"),
                    invoker_id: query_session.client_id,
                    invoker_name: String::from("serveradmin"),
                    invoker_uid: runtime.query_session_unique_identifier(&query_session),
                }],
            )
            .expect("query bridge should deliver moved notifications");

        let target_frames = drain_test_frames(&target_pending);
        assert_eq!(target_frames.len(), 1);
        let target_payload = parse_frame(&target_frames[0]);
        assert_eq!(target_payload["command"], "notifyclientmoved");
        assert_eq!(target_payload["data"][0]["clid"], target.client_id.to_string());
        assert_eq!(target_payload["data"][0]["cfid"], "1");
        assert_eq!(target_payload["data"][0]["ctid"], "2");
        assert_eq!(target_payload["data"][0]["reasonid"], "1");

        let viewer_frames = drain_test_frames(&viewer_pending);
        assert_eq!(viewer_frames.len(), 1);
        let viewer_payload = parse_frame(&viewer_frames[0]);
        assert_eq!(viewer_payload["command"], "notifyclientmoved");
        assert_eq!(viewer_payload["data"][0]["clid"], target.client_id.to_string());
        assert_eq!(viewer_payload["data"][0]["invokerid"], query_session.client_id.to_string());
    }

    #[test]
    fn query_channel_structure_notifications_bridge_into_blackteaweb_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-query-channel-structure-bridge");
        let mut alpha = BlackTeaWebSessionHandler::new(54);
        let mut beta = BlackTeaWebSessionHandler::new(55);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-structure-alpha",
            "StructureAlpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-structure-beta",
            "StructureBeta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let mut query_session = login_query_serveradmin(&mut runtime, 601);
        let invoker_name = query_session
            .authenticated_login
            .clone()
            .expect("query session should be authenticated");

        let created_response = runtime.execute(
            r"channelcreate channel_name=Bridge\sRoom cpid=1 order=0 channel_topic=Build\sQueue",
            &mut query_session,
        );
        let created_id = extract_response_field(&created_response, "cid")
            .and_then(|value| value.parse::<u32>().ok())
            .expect("channelcreate should expose cid");
        let created_snapshot = runtime
            .snapshot_channel(1, created_id)
            .expect("created channel should exist");

        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ChannelCreated {
                    server_id: 1,
                    channel: created_snapshot,
                    invoker_id: query_session.client_id,
                    invoker_name: invoker_name.clone(),
                }],
            )
            .expect("channel create should bridge into BlackTeaWeb");

        assert!(
            runtime
                .execute(
                    &format!(
                        r"channeledit cid={} channel_name=Bridge\sSuite channel_topic=Late\sSession",
                        created_id
                    ),
                    &mut query_session,
                )
                .contains("error id=0 msg=ok")
        );
        let edited_snapshot = runtime
            .snapshot_channel(1, created_id)
            .expect("edited channel should exist");
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ChannelEdited {
                    server_id: 1,
                    channel: edited_snapshot,
                    description_changed: false,
                    invoker_id: query_session.client_id,
                    invoker_name: invoker_name.clone(),
                }],
            )
            .expect("channel edit should bridge into BlackTeaWeb");

        assert!(
            runtime
                .execute(
                    &format!("channelmove cid={} cpid=0 order=0", created_id),
                    &mut query_session,
                )
                .contains("error id=0 msg=ok")
        );
        let moved_snapshot = runtime
            .snapshot_channel(1, created_id)
            .expect("moved channel should exist");
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ChannelMoved {
                    server_id: 1,
                    previous_parent_id: 1,
                    channel: moved_snapshot,
                    invoker_id: query_session.client_id,
                    invoker_name: invoker_name.clone(),
                }],
            )
            .expect("channel move should bridge into BlackTeaWeb");

        let deleted_snapshot = runtime
            .snapshot_channel(1, created_id)
            .expect("channel should still exist before delete");
        assert!(
            runtime
                .execute(
                    &format!("channeldelete cid={} force=1", created_id),
                    &mut query_session,
                )
                .contains("error id=0 msg=ok")
        );
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ChannelDeleted {
                    server_id: 1,
                    channel: deleted_snapshot,
                    invoker_id: query_session.client_id,
                    invoker_name: invoker_name,
                }],
            )
            .expect("channel delete should bridge into BlackTeaWeb");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        for frames in [&alpha_frames, &beta_frames] {
            assert_eq!(frames.len(), 4);

            let created = parse_frame(&frames[0]);
            assert_eq!(created["command"], "notifychannelcreated");
            assert_eq!(created["data"][0]["cid"], created_id.to_string());
            assert_eq!(created["data"][0]["cpid"], "1");
            assert_eq!(created["data"][0]["channel_name"], "Bridge Room");
            assert_eq!(created["data"][0]["channel_topic"], "Build Queue");

            let edited = parse_frame(&frames[1]);
            assert_eq!(edited["command"], "notifychanneledited");
            assert_eq!(edited["data"][0]["cid"], created_id.to_string());
            assert_eq!(edited["data"][0]["channel_name"], "Bridge Suite");
            assert_eq!(edited["data"][0]["channel_topic"], "Late Session");

            let moved = parse_frame(&frames[2]);
            assert_eq!(moved["command"], "notifychannelmoved");
            assert_eq!(moved["data"][0]["cid"], created_id.to_string());
            assert_eq!(moved["data"][0]["cpid"], "0");
            assert_eq!(moved["data"][0]["order"], "0");
            assert_eq!(moved["data"][0]["channel_name"], "Bridge Suite");

            let deleted = parse_frame(&frames[3]);
            assert_eq!(deleted["command"], "notifychanneldeleted");
            assert_eq!(deleted["data"][0]["cid"], created_id.to_string());
            assert_eq!(deleted["data"][0]["cpid"], "0");
        }
    }

    #[test]
    fn query_channel_description_changes_bridge_and_fetch_correctly() {
        let mut runtime = create_test_runtime("blackteaweb-query-channel-description");
        let mut viewer = BlackTeaWebSessionHandler::new(56);

        let _ = login_with_identity(
            &mut viewer,
            &mut runtime,
            "compat-public-key-description-viewer",
            "DescriptionViewer",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let viewer_pending = register_test_session(&sessions, &viewer, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let mut query_session = login_query_serveradmin(&mut runtime, 701);
        let invoker_name = query_session
            .authenticated_login
            .clone()
            .expect("query session should be authenticated");
        assert!(
            runtime
                .execute(
                    r"channeledit cid=1 channel_description=Bridge\sDescription",
                    &mut query_session,
                )
                .contains("error id=0 msg=ok")
        );

        let edited_snapshot = runtime
            .snapshot_channel(1, 1)
            .expect("default channel should exist");
        bridge
            .broadcast_transport_notifications(
                &runtime,
                Some(query_session.client_id),
                &[TransportNotification::ChannelEdited {
                    server_id: 1,
                    channel: edited_snapshot,
                    description_changed: true,
                    invoker_id: query_session.client_id,
                    invoker_name,
                }],
            )
            .expect("channel description update should bridge into BlackTeaWeb");

        let bridged_frames = drain_test_frames(&viewer_pending);
        assert_eq!(bridged_frames.len(), 2);
        let bridged_description_changed = parse_frame(&bridged_frames[0]);
        assert_eq!(
            bridged_description_changed["command"],
            "notifychanneldescriptionchanged"
        );
        assert_eq!(bridged_description_changed["data"][0]["cid"], "1");
        let bridged_payload = parse_frame(&bridged_frames[1]);
        assert_eq!(bridged_payload["command"], "notifychanneledited");
        assert_eq!(bridged_payload["data"][0]["cid"], "1");
        assert_eq!(
            bridged_payload["data"][0]["channel_description"],
            "Bridge Description"
        );

        let fetched = viewer
            .handle_text_frame(
                r#"{"type":"command","command":"channelgetdescription","data":[{"return_code":"56","cid":"1"}]}"#,
                &mut runtime,
            )
            .expect("channelgetdescription should return the updated description");
        assert_eq!(command_name(&fetched[0]), "notifychanneldescriptionchanged");
        assert_eq!(command_name(&fetched[1]), "notifychanneledited");
        assert_eq!(
            parse_frame(&fetched[1])["data"][0]["channel_description"],
            "Bridge Description"
        );
    }

    #[test]
    fn server_variable_frames_reflect_live_online_counts() {
        let mut runtime = create_test_runtime("blackteaweb-server-variable-counts");
        let mut primary = BlackTeaWebSessionHandler::new(57);

        let _ = login_with_identity(
            &mut primary,
            &mut runtime,
            "compat-public-key-server-vars-primary",
            "ServerVarsPrimary",
        );

        let initial = primary
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"57"}]}"#,
                &mut runtime,
            )
            .expect("initial servergetvariables should succeed");
        assert_eq!(command_name(&initial[0]), "notifyserverupdated");
        let initial_payload = parse_frame(&initial[0]);
        let initial_clients = parse_u64_field(&initial_payload, "virtualserver_clientsonline");
        let initial_queries = parse_u64_field(&initial_payload, "virtualserver_queryclientsonline");

        let _query_session = login_query_serveradmin(&mut runtime, 801);
        let mut secondary = BlackTeaWebSessionHandler::new(58);
        let _ = login_with_identity(
            &mut secondary,
            &mut runtime,
            "compat-public-key-server-vars-secondary",
            "ServerVarsSecondary",
        );

        let updated = primary
            .handle_text_frame(
                r#"{"type":"command","command":"servergetvariables","data":[{"return_code":"58"}]}"#,
                &mut runtime,
            )
            .expect("updated servergetvariables should succeed");
        assert_eq!(command_name(&updated[0]), "notifyserverupdated");
        let updated_payload = parse_frame(&updated[0]);
        assert_eq!(
            parse_u64_field(&updated_payload, "virtualserver_clientsonline"),
            initial_clients + 2
        );
        assert_eq!(
            parse_u64_field(&updated_payload, "virtualserver_queryclientsonline"),
            initial_queries + 1
        );

        let connection_info = primary
            .handle_text_frame(
                r#"{"type":"command","command":"serverrequestconnectioninfo","data":[{"return_code":"59"}]}"#,
                &mut runtime,
            )
            .expect("serverrequestconnectioninfo should succeed");
        assert_eq!(
            command_name(&connection_info[0]),
            "notifyserverconnectioninfo"
        );
        let connection_payload = parse_frame(&connection_info[0]);
        assert_eq!(
            parse_u64_field(&connection_payload, "virtualserver_clientsonline"),
            initial_clients + 2
        );
        assert_eq!(connection_payload["data"][0]["connection_ping"], "0");
    }

    #[test]
    fn same_public_key_reuses_stable_identity_but_different_keys_split_it() {
        let mut runtime = create_test_runtime("blackteaweb-stable-identity-reuse");
        let mut same_a = BlackTeaWebSessionHandler::new(60);
        let mut same_b = BlackTeaWebSessionHandler::new(61);
        let mut isolated = BlackTeaWebSessionHandler::new(62);

        let _ = login_with_identity(
            &mut same_a,
            &mut runtime,
            "compat-public-key-same",
            "BrowserSameA",
        );
        let _ = login_with_identity(
            &mut same_b,
            &mut runtime,
            "compat-public-key-same",
            "BrowserSameB",
        );
        let _ = login_with_identity(
            &mut isolated,
            &mut runtime,
            "compat-public-key-other",
            "BrowserOther",
        );

        let same_a_uid = same_a
            .self_client_state
            .get("client_unique_identifier")
            .cloned()
            .expect("same_a uid should be set");
        let same_b_uid = same_b
            .self_client_state
            .get("client_unique_identifier")
            .cloned()
            .expect("same_b uid should be set");
        let isolated_uid = isolated
            .self_client_state
            .get("client_unique_identifier")
            .cloned()
            .expect("isolated uid should be set");

        let same_a_dbid = same_a
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("same_a dbid should parse");
        let same_b_dbid = same_b
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("same_b dbid should parse");
        let isolated_dbid = isolated
            .self_client_state
            .get("client_database_id")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("isolated dbid should parse");

        assert_eq!(same_a_uid, same_b_uid);
        assert_eq!(same_a_dbid, same_b_dbid);
        assert_eq!(
            same_a_uid,
            stable_web_client_unique_identifier("compat-public-key-same")
        );
        assert_eq!(same_a_dbid, stable_web_client_database_id(&same_a_uid));

        assert_ne!(same_a_uid, isolated_uid);
        assert_ne!(same_a_dbid, isolated_dbid);
        assert_eq!(
            isolated_uid,
            stable_web_client_unique_identifier("compat-public-key-other")
        );
        assert_eq!(isolated_dbid, stable_web_client_database_id(&isolated_uid));
    }

    #[test]
    fn plugincmd_channel_targets_only_matching_channel_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-plugincmd-channel");
        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));

        let mut alpha = BlackTeaWebSessionHandler::new(63);
        let mut beta = BlackTeaWebSessionHandler::new(64);
        let mut gamma = BlackTeaWebSessionHandler::new(65);

        attach_test_realtime_support(&mut alpha, Arc::clone(&sessions));
        attach_test_realtime_support(&mut beta, Arc::clone(&sessions));
        attach_test_realtime_support(&mut gamma, Arc::clone(&sessions));

        let _ = login_with_identity(&mut alpha, &mut runtime, "compat-plugin-alpha", "PluginAlpha");
        let _ = login_with_identity(&mut beta, &mut runtime, "compat-plugin-beta", "PluginBeta");
        let _ = login_with_identity(&mut gamma, &mut runtime, "compat-plugin-gamma", "PluginGamma");

        gamma
            .handle_text_frame(
                r#"{"type":"command","command":"clientmove","data":[{"return_code":"plug-move","clid":"20065","cid":"2"}]}"#,
                &mut runtime,
            )
            .expect("gamma move should succeed");

        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let gamma_pending = register_test_session(&sessions, &gamma, &runtime);

        let response = alpha
            .handle_text_frame(
                r#"{"type":"command","command":"plugincmd","data":[{"return_code":"plug-1","name":"blackteaweb.plugin","data":"channel-message","targetmode":"0","target":"1"}]}"#,
                &mut runtime,
            )
            .expect("plugincmd channel send should succeed");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        let gamma_frames = drain_test_frames(&gamma_pending);

        assert_eq!(alpha_frames.len(), 1);
        assert_eq!(beta_frames.len(), 1);
        assert!(gamma_frames.is_empty());

        for frame in [&alpha_frames[0], &beta_frames[0]] {
            let payload = parse_frame(frame);
            assert_eq!(payload["command"], "notifyplugincmd");
            assert_eq!(payload["data"][0]["name"], "blackteaweb.plugin");
            assert_eq!(payload["data"][0]["data"], "channel-message");
            assert_eq!(payload["data"][0]["invokername"], "PluginAlpha");
        }
    }

    #[test]
    fn plugincmd_private_targets_single_client() {
        let mut runtime = create_test_runtime("blackteaweb-plugincmd-private");
        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));

        let mut alpha = BlackTeaWebSessionHandler::new(66);
        let mut beta = BlackTeaWebSessionHandler::new(67);
        let mut gamma = BlackTeaWebSessionHandler::new(68);

        attach_test_realtime_support(&mut alpha, Arc::clone(&sessions));
        attach_test_realtime_support(&mut beta, Arc::clone(&sessions));
        attach_test_realtime_support(&mut gamma, Arc::clone(&sessions));

        let _ = login_with_identity(&mut alpha, &mut runtime, "compat-plugin-private-alpha", "PrivateAlpha");
        let _ = login_with_identity(&mut beta, &mut runtime, "compat-plugin-private-beta", "PrivateBeta");
        let _ = login_with_identity(&mut gamma, &mut runtime, "compat-plugin-private-gamma", "PrivateGamma");

        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let gamma_pending = register_test_session(&sessions, &gamma, &runtime);

        let response = alpha
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"plugincmd","data":[{{"return_code":"plug-2","name":"blackteaweb.plugin","data":"private-message","targetmode":"2","target":"{}"}}]}}"#,
                    beta.client_id,
                ),
                &mut runtime,
            )
            .expect("plugincmd private send should succeed");
        assert_eq!(parse_frame(&response[0])["data"][0]["id"], "0");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        let gamma_frames = drain_test_frames(&gamma_pending);

        assert!(alpha_frames.is_empty());
        assert_eq!(beta_frames.len(), 1);
        assert!(gamma_frames.is_empty());

        let payload = parse_frame(&beta_frames[0]);
        assert_eq!(payload["command"], "notifyplugincmd");
        assert_eq!(payload["data"][0]["name"], "blackteaweb.plugin");
        assert_eq!(payload["data"][0]["data"], "private-message");
        assert_eq!(payload["data"][0]["invokername"], "PrivateAlpha");
    }

    #[test]
    fn rtcsessiondescribe_returns_answer_notify() {
        let mut runtime = create_test_runtime("blackteaweb-rtc-sessiondescribe");
        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));
        let mut handler = BlackTeaWebSessionHandler::new(69);

        attach_test_realtime_support(&mut handler, sessions);
        let _ = login_with_identity(&mut handler, &mut runtime, "compat-rtc-alpha", "RtcAlpha");

        let offer = generate_rtc_test_offer();
        let offer_json = serde_json::to_string(&offer).expect("offer should serialize as json string");
        let response = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"rtcsessiondescribe","data":[{{"return_code":"rtc-1","mode":"offer","sdp":{offer_json}}}]}}"#
                ),
                &mut runtime,
            )
            .expect("rtc session describe should succeed");

        assert_eq!(response.len(), 2, "unexpected rtc response: {response:?}");
        let answer = parse_frame(&response[0]);
        assert_eq!(answer["command"], "notifyrtcsessiondescription");
        assert_eq!(answer["data"][0]["mode"], "answer");
        assert!(answer["data"][0]["sdp"]
            .as_str()
            .is_some_and(|sdp| !sdp.is_empty()));
        assert!(answer["data"][0]["sdp"]
            .as_str()
            .is_some_and(|sdp| sdp.contains("a=ssrc:")));
        assert!(answer["data"][0]["sdp"]
            .as_str()
            .is_some_and(|sdp| sdp.contains("a=rtpmap:126 H264/90000")));
        assert!(answer["data"][0]["sdp"]
            .as_str()
            .is_some_and(|sdp| sdp.contains("a=rtpmap:120 VP8/90000")));
        assert!(answer["data"][0]["sdp"]
            .as_str()
            .is_some_and(|sdp| sdp.contains("a=rtpmap:98 VP9/90000")));
        assert!(answer["data"][0]["sdp"].as_str().is_some_and(|sdp| {
            sdp.lines()
                .find(|line| line.starts_with("m=video "))
                .is_some_and(|line| line.contains("120") && line.contains("98") && line.contains("126"))
        }));
        assert!(answer["data"][0]["sdp"].as_str().is_some_and(|sdp| {
            let mut in_video = false;
            for line in sdp.lines() {
                if line.starts_with("m=") {
                    in_video = line.starts_with("m=video ");
                    continue;
                }
                if in_video && line == "a=recvonly" {
                    return false;
                }
            }
            true
        }));
        assert_eq!(parse_frame(&response[1])["data"][0]["id"], "0");
    }

    #[test]
    fn presence_rows_cover_enter_move_and_left_view() {
        let presence = BlackTeaWebPresence {
            client_id: 20042,
            server_id: 1,
            channel_id: 2,
            client_state: default_self_client_state(20042),
        };

        let enter = presence_enter_view_row(&presence, Some(1), 2);
        assert_eq!(enter["clid"], "20042");
        assert_eq!(enter["cfid"], "1");
        assert_eq!(enter["ctid"], "2");
        assert_eq!(enter["client_type_exact"], "3");

        let moved = presence_move_row(&presence, 1, 0, "changed channel");
        assert_eq!(moved["clid"], "20042");
        assert_eq!(moved["cfid"], "1");
        assert_eq!(moved["ctid"], "2");
        assert_eq!(moved["reasonmsg"], "changed channel");
        assert_eq!(moved["invokeruid"], "compat-web-20042");

        let left = presence_left_view_row(&presence, None, 8, "left server");
        assert_eq!(left["clid"], "20042");
        assert_eq!(left["cfid"], "2");
        assert_eq!(left["ctid"], "0");
        assert_eq!(left["reasonid"], "8");
        assert_eq!(left["reasonmsg"], "left server");

        let mut updated_presence = presence.clone();
        updated_presence.client_state.insert(
            String::from("client_nickname"),
            String::from("Tea Web Peer"),
        );
        updated_presence
            .client_state
            .insert(String::from("client_output_muted"), String::from("1"));

        let peer_frames = derive_peer_frames(&Some(presence), &Some(updated_presence))
            .expect("peer frames should encode");
        match &peer_frames[0] {
            PresenceBroadcast::PeerUpdate { before, after, .. } => {
                let row = presence_update_row(before, after).expect("peer update row should exist");
                assert_eq!(row["clid"], "20042");
                assert_eq!(row["client_nickname"], "Tea Web Peer");
                assert_eq!(row["client_output_muted"], "1");
            }
            other => panic!("unexpected peer broadcast: {other:?}"),
        }
    }

    #[test]
    fn ping_is_answered_without_login() {
        let mut handler = BlackTeaWebSessionHandler::new(3);
        let mut runtime = create_test_runtime("blackteaweb-permission-before-login");

        let messages = handler
            .handle_text_frame(r#"{"type":"ping","payload":"44"}"#, &mut runtime)
            .expect("ping should succeed");
        let pong = parse_frame(&messages[0]);

        assert_eq!(pong["type"], "pong");
        assert_eq!(pong["payload"], "44");
        assert_eq!(pong["ping_native"], "0");
    }

    #[test]
    fn permissionlist_before_login_is_rejected() {
        let mut handler = BlackTeaWebSessionHandler::new(1);
        let mut runtime = create_test_runtime("blackteaweb-ping-no-login");

        let messages = handler
            .handle_text_frame(
                r#"{"type":"command","command":"permissionlist","data":[{"return_code":"9"}]}"#,
                &mut runtime,
            )
            .expect("permissionlist should respond");
        let error = parse_frame(&messages[0]);

        assert_eq!(error["command"], "error");
        assert_eq!(error["data"][0]["id"], "1794");
    }

    #[test]
    fn web_servergroupaddperm_returns_bulk_success_rows() {
        let mut handler = BlackTeaWebSessionHandler::new(71);
        let mut runtime = create_test_runtime("blackteaweb-servergroupaddperm-bulk");
        let _ = login(&mut handler, &mut runtime);

        let self_client_database_id = handler
            .self_client_database_id()
            .expect("logged in handler should expose its client database id");
        promote_web_permission_actor(&mut runtime, self_client_database_id, 1071);

        let messages = handler
            .handle_text_frame(
                r#"{"type":"command","command":"servergroupaddperm","data":[{"return_code":"41","sgid":"8","permsid":"i_client_private_textmessage_power","permvalue":"42","permnegated":"0","permskip":"0"},{"return_code":"41","permsid":"i_client_poke_power","permvalue":"21","permnegated":"0","permskip":"0"}]}"#,
                &mut runtime,
            )
            .expect("servergroupaddperm should succeed");

        assert_eq!(command_name(&messages[0]), "notifyclientneededpermissions");
        let result = parse_frame(messages.last().expect("result frame should exist"));
        assert_eq!(result["command"], "error");
        assert_eq!(
            result["data"]
                .as_array()
                .expect("bulk rows should be returned")
                .len(),
            2
        );
        assert!(
            result["data"]
                .as_array()
                .expect("bulk rows should be returned")
                .iter()
                .all(|row| row["id"] == "0" && row["msg"] == "ok")
        );

        let mut admin = login_query_serveradmin(&mut runtime, 2071);
        let permlist = runtime.execute("servergrouppermlist sgid=8 -permsid", &mut admin);
        assert!(permlist.contains("permsid=i_client_private_textmessage_power"));
        assert!(permlist.contains("permvalue=42"));
        assert!(permlist.contains("permsid=i_client_poke_power"));
        assert!(permlist.contains("permvalue=21"));
    }

    #[test]
    fn web_channelclientaddperm_updates_target_permissions() {
        let mut actor = BlackTeaWebSessionHandler::new(72);
        let mut target = BlackTeaWebSessionHandler::new(73);
        let mut runtime = create_test_runtime("blackteaweb-channelclientaddperm");
        let _ = login(&mut actor, &mut runtime);
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-2",
            "Tea Web Peer",
        );

        let actor_client_database_id = actor
            .self_client_database_id()
            .expect("actor should expose its client database id");
        let target_client_database_id = target
            .self_client_database_id()
            .expect("target should expose its client database id");
        promote_web_permission_actor(&mut runtime, actor_client_database_id, 1072);

        let messages = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"channelclientaddperm","data":[{{"return_code":"42","cid":"1","cldbid":"{}","permsid":"i_client_talk_power","permvalue":"19","permnegated":"0","permskip":"0"}}]}}"#,
                    target_client_database_id,
                ),
                &mut runtime,
            )
            .expect("channelclientaddperm should succeed");

        assert_eq!(command_name(&messages[0]), "notifyclientneededpermissions");
        let result = parse_frame(messages.last().expect("result frame should exist"));
        assert_eq!(result["command"], "error");
        assert_eq!(result["data"][0]["id"], "0");

        let mut admin = login_query_serveradmin(&mut runtime, 2072);
        let permlist = runtime.execute(
            &format!(
                "channelclientpermlist cid=1 cldbid={} -permsid",
                target_client_database_id
            ),
            &mut admin,
        );
        assert!(permlist.contains("permsid=i_client_talk_power"));
        assert!(permlist.contains("permvalue=19"));
    }

    #[test]
    fn web_group_rename_and_delete_commands_bridge_for_web_actor() {
        let mut handler = BlackTeaWebSessionHandler::new(74);
        let mut runtime = create_test_runtime("blackteaweb-group-rename-delete");
        let _ = login(&mut handler, &mut runtime);

        let self_client_database_id = handler
            .self_client_database_id()
            .expect("logged in handler should expose its client database id");
        promote_web_permission_actor(&mut runtime, self_client_database_id, 1074);

        let mut admin = login_query_serveradmin(&mut runtime, 2074);
        let server_group_create = runtime.execute(
            r"servergroupcopy ssgid=8 tsgid=0 name=Bridge\sServer\sGroup",
            &mut admin,
        );
        let server_group_id = extract_response_field(&server_group_create, "sgid")
            .expect("servergroupcopy should expose sgid");

        let server_rename = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"servergrouprename","data":[{{"return_code":"74-sgr","sgid":"{}","name":"Bridge Server Group Renamed"}}]}}"#,
                    server_group_id,
                ),
                &mut runtime,
            )
            .expect("servergrouprename should succeed");
        assert_eq!(
            command_name(server_rename.last().expect("result frame should exist")),
            "error"
        );
        assert_eq!(
            parse_frame(server_rename.last().expect("result frame should exist"))["data"][0]["id"],
            "0"
        );
        let server_groups = runtime.execute("servergrouplist", &mut admin);
        assert!(server_groups.contains(&format!("sgid={}", server_group_id)));
        assert!(server_groups.contains(r"name=Bridge\sServer\sGroup\sRenamed"));

        let server_delete = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"servergroupdel","data":[{{"return_code":"74-sgd","sgid":"{}","force":"1"}}]}}"#,
                    server_group_id,
                ),
                &mut runtime,
            )
            .expect("servergroupdel should succeed");
        assert_eq!(
            command_name(server_delete.last().expect("result frame should exist")),
            "error"
        );
        assert_eq!(
            parse_frame(server_delete.last().expect("result frame should exist"))["data"][0]["id"],
            "0"
        );
        let server_groups_after_delete = runtime.execute("servergrouplist", &mut admin);
        assert!(!server_groups_after_delete.contains(&format!("sgid={}", server_group_id)));

        let channel_group_create = runtime.execute(
            r"channelgroupcopy scgid=8 tcgid=0 name=Bridge\sChannel\sGroup",
            &mut admin,
        );
        let channel_group_id = extract_response_field(&channel_group_create, "cgid")
            .expect("channelgroupcopy should expose cgid");

        let channel_rename = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"channelgrouprename","data":[{{"return_code":"74-cgr","cgid":"{}","name":"Bridge Channel Group Renamed"}}]}}"#,
                    channel_group_id,
                ),
                &mut runtime,
            )
            .expect("channelgrouprename should succeed");
        assert_eq!(
            command_name(channel_rename.last().expect("result frame should exist")),
            "error"
        );
        assert_eq!(
            parse_frame(channel_rename.last().expect("result frame should exist"))["data"][0]["id"],
            "0"
        );
        let channel_groups = runtime.execute("channelgrouplist", &mut admin);
        assert!(channel_groups.contains(&format!("cgid={}", channel_group_id)));
        assert!(channel_groups.contains(r"name=Bridge\sChannel\sGroup\sRenamed"));

        let channel_delete = handler
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"channelgroupdel","data":[{{"return_code":"74-cgd","cgid":"{}","force":"1"}}]}}"#,
                    channel_group_id,
                ),
                &mut runtime,
            )
            .expect("channelgroupdel should succeed");
        assert_eq!(
            command_name(channel_delete.last().expect("result frame should exist")),
            "error"
        );
        assert_eq!(
            parse_frame(channel_delete.last().expect("result frame should exist"))["data"][0]["id"],
            "0"
        );
        let channel_groups_after_delete = runtime.execute("channelgrouplist", &mut admin);
        assert!(!channel_groups_after_delete.contains(&format!("cgid={}", channel_group_id)));
    }

    #[test]
    fn web_clientaddperm_updates_direct_permissions_for_normal_clients() {
        let mut actor = BlackTeaWebSessionHandler::new(75);
        let mut target = BlackTeaWebSessionHandler::new(76);
        let mut runtime = create_test_runtime("blackteaweb-clientaddperm");
        let _ = login(&mut actor, &mut runtime);
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-clientperm-target",
            "Tea Web Target",
        );

        let actor_client_database_id = actor
            .self_client_database_id()
            .expect("actor should expose its client database id");
        let target_client_database_id = target
            .self_client_database_id()
            .expect("target should expose its client database id");
        promote_web_permission_actor(&mut runtime, actor_client_database_id, 1075);

        let add_messages = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientaddperm","data":[{{"return_code":"75-add","cldbid":"{}","permsid":"i_client_private_textmessage_power","permvalue":"17","permnegated":"0","permskip":"0"}}]}}"#,
                    target_client_database_id,
                ),
                &mut runtime,
            )
            .expect("clientaddperm should succeed");

        let add_result = parse_frame(add_messages.last().expect("result frame should exist"));
        assert_eq!(add_result["command"], "error");
        assert_eq!(add_result["data"][0]["id"], "0");

        let mut admin = login_query_serveradmin(&mut runtime, 2075);
        let permlist = runtime.execute(
            &format!(
                "clientpermlist cldbid={} -permsid",
                target_client_database_id,
            ),
            &mut admin,
        );
        assert!(permlist.contains("permsid=i_client_private_textmessage_power"));
        assert!(permlist.contains("permvalue=17"));

        let permoverview = runtime.execute(
            &format!(
                "permoverview cid=1 cldbid={} permsid=i_client_private_textmessage_power",
                target_client_database_id,
            ),
            &mut admin,
        );
        assert!(permoverview.contains(&format!("id1={}", target_client_database_id)));
        assert!(permoverview.contains("t=1"));
        assert!(permoverview.contains("v=17"));

        let permfind = runtime.execute(
            "permfind permsid=i_client_private_textmessage_power",
            &mut admin,
        );
        assert!(permfind.contains(&format!("id1={}", target_client_database_id)));
        assert!(permfind.contains("t=1"));

        let del_messages = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientdelperm","data":[{{"return_code":"75-del","cldbid":"{}","permsid":"i_client_private_textmessage_power"}}]}}"#,
                    target_client_database_id,
                ),
                &mut runtime,
            )
            .expect("clientdelperm should succeed");

        let del_result = parse_frame(del_messages.last().expect("result frame should exist"));
        assert_eq!(del_result["command"], "error");
        assert_eq!(del_result["data"][0]["id"], "0");

        let permlist_after_delete = runtime.execute(
            &format!(
                "clientpermlist cldbid={} -permsid",
                target_client_database_id,
            ),
            &mut admin,
        );
        assert!(!permlist_after_delete.contains("permsid=i_client_private_textmessage_power"));
    }

    #[test]
    fn web_permission_mutations_refresh_other_connected_blackteaweb_sessions() {
        let mut actor = BlackTeaWebSessionHandler::new(77);
        let mut target = BlackTeaWebSessionHandler::new(78);
        let mut runtime = create_test_runtime("blackteaweb-permission-refresh-web");
        let _ = login(&mut actor, &mut runtime);
        let _ = login_with_identity(
            &mut target,
            &mut runtime,
            "compat-public-key-refresh-target",
            "Refresh Target",
        );

        let actor_client_database_id = actor
            .self_client_database_id()
            .expect("actor should expose its client database id");
        let target_client_database_id = target
            .self_client_database_id()
            .expect("target should expose its client database id");
        promote_web_permission_actor(&mut runtime, actor_client_database_id, 1077);

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));
        let actor_pending = register_test_session(&sessions, &actor, &runtime);
        let target_pending = register_test_session(&sessions, &target, &runtime);

        let messages = actor
            .handle_text_frame(
                &format!(
                    r#"{{"type":"command","command":"clientaddperm","data":[{{"return_code":"77-add","cldbid":"{}","permsid":"i_client_private_textmessage_power","permvalue":"17","permnegated":"0","permskip":"0"}}]}}"#,
                    target_client_database_id,
                ),
                &mut runtime,
            )
            .expect("clientaddperm should succeed");

        let result = parse_frame(messages.last().expect("result frame should exist"));
        assert_eq!(result["command"], "error");
        assert_eq!(result["data"][0]["id"], "0");

        let pending_refreshes = actor.drain_pending_permission_refreshes();
        assert_eq!(pending_refreshes.len(), 1);
        broadcast_permission_refreshes(&sessions, &runtime, &pending_refreshes)
            .expect("queued refreshes should broadcast");

        let actor_frames = drain_test_frames(&actor_pending);
        let target_frames = drain_test_frames(&target_pending);
        assert!(
            actor_frames
                .iter()
                .any(|frame| command_name(frame) == "notifyclientneededpermissions")
        );
        assert!(
            target_frames
                .iter()
                .any(|frame| command_name(frame) == "notifyclientneededpermissions")
        );
    }

    #[test]
    fn query_permission_refresh_bridge_updates_registered_blackteaweb_sessions() {
        let mut runtime = create_test_runtime("blackteaweb-permission-refresh-query");
        let mut alpha = BlackTeaWebSessionHandler::new(79);
        let mut beta = BlackTeaWebSessionHandler::new(80);

        let _ = login_with_identity(
            &mut alpha,
            &mut runtime,
            "compat-public-key-refresh-alpha",
            "Refresh Alpha",
        );
        let _ = login_with_identity(
            &mut beta,
            &mut runtime,
            "compat-public-key-refresh-beta",
            "Refresh Beta",
        );

        let sessions: SharedBlackTeaWebSessions = Arc::new(Mutex::new(HashMap::new()));
        let alpha_pending = register_test_session(&sessions, &alpha, &runtime);
        let beta_pending = register_test_session(&sessions, &beta, &runtime);
        let bridge = BlackTeaWebNotificationBridge {
            sessions: Arc::clone(&sessions),
        };

        let mut admin = login_query_serveradmin(&mut runtime, 2079);
        assert!(runtime
            .execute(
                "servergroupaddperm sgid=8 permsid=i_client_private_textmessage_power permvalue=42 permnegated=0 permskip=0",
                &mut admin,
            )
            .contains("error id=0 msg=ok"));

        bridge
            .broadcast_permission_refreshes(
                &runtime,
                1,
                permission_refresh_scope("servergroupaddperm"),
            )
            .expect("query-side permission refresh should broadcast");

        let alpha_frames = drain_test_frames(&alpha_pending);
        let beta_frames = drain_test_frames(&beta_pending);
        for frames in [&alpha_frames, &beta_frames] {
            assert!(
                frames
                    .iter()
                    .any(|frame| command_name(frame) == "notifyclientneededpermissions")
            );
            assert!(
                frames
                    .iter()
                    .any(|frame| command_name(frame) == "notifyservergrouplist")
            );
        }
    }
}
