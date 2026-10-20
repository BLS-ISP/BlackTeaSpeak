use super::*;
#[tokio::test]
async fn test_ts3_compat_udp_commands() {
    let state_path = temp_state_path("ts3-compat-udp");
    cleanup_state_path(&state_path);
    let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should build");
    let runtime_arc = std::sync::Arc::new(Mutex::new(runtime));
    
    // Bind socket for testing
    let socket = std::sync::Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 12345));
    
    // Create UdpSession
    let mut session = blackteaspeak_server::teaspeak_transport::UdpSession {
        client_id: 1,
        seed_client: vec![1, 2, 3],
        seed_server: vec![4, 5, 6],
        server_sec: p256::SecretKey::random(&mut rand::thread_rng()),
        shared_secret: [0u8; 64],
        client_uid: "client-uid-123".to_string(),
        server_uid: "server-uid-456".to_string(),
        server_packet_id: std::sync::Arc::new(Mutex::new(1)),
        server_ack_packet_id: 0,
        pending_rtc_describe: false,
        pending_rtc_is_video: false,
        rtc_describe_buffer: Vec::new(),
        ephemeral_sec: None,
        entry2_hash: None,
        client_nickname: "InitialNickname".to_string(),
        command_fragment_buffer: Vec::new(),
        command_fragment_active: false,
        is_active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        is_teaspeak_client: false,
        is_ts6_client: false,
    };
    
    let iv_struct = [0u8; 20];
    let server_id = 1;
    let packet_id = 10;
    
    // 1. clientinit
    let payload_init = "clientinit client_nickname=InitialNickname";
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        payload_init,
        payload_init.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    // Check that client is online with ID 1 and grant Server Admin (sgid=6) for permanent channel mutations
    {
        let mut rt = runtime_arc.lock().unwrap();
        let client = rt.store.online_clients.get(&1).unwrap();
        assert_eq!(client.nickname, "InitialNickname");
        let cldbid = client.database_id;
        let mut admin = QuerySessionState::default();
        assert!(rt.execute("login serveradmin serveradmin", &mut admin).contains("error id=0 msg=ok"));
        assert!(rt.execute("use sid=1", &mut admin).contains("error id=0 msg=ok"));
        assert!(rt.execute(&format!("servergroupaddclient sgid=6 cldbid={}", cldbid), &mut admin).contains("error id=0 msg=ok"));
    }
    
    // 2. clientupdate
    let payload_update = "clientupdate client_nickname=NewNickname rt-123";
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        payload_update,
        payload_update.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    {
        let rt = runtime_arc.lock().unwrap();
        let client = rt.store.online_clients.get(&1).unwrap();
        assert_eq!(client.nickname, "NewNickname");
    }
    
    // 3. clientmove
    let payload_move = "clientmove clid=1 cid=2 rt-124";
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        payload_move,
        payload_move.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    {
        let rt = runtime_arc.lock().unwrap();
        let client = rt.store.online_clients.get(&1).unwrap();
        assert_eq!(client.channel_id, 2);
    }
    
    // 4. sendtextmessage
    let payload_msg = "sendtextmessage targetmode=2 target=1 msg=Hello\\sWorld rt-125";
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        payload_msg,
        payload_msg.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    // 5. channelcreate
    let payload_create = "channelcreate channel_name=TestRoom cpid=1 channel_flag_permanent=1 rt-126";
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        payload_create,
        payload_create.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    let created_cid = {
        let rt = runtime_arc.lock().unwrap();
        let channels = rt.store.channels.get(&server_id).unwrap();
        let chan = channels.iter().find(|c| c.name == "TestRoom").unwrap();
        assert_eq!(chan.parent_id, 1);
        assert_eq!(chan.kind, blackteaspeak_server::runtime::ChannelKind::Permanent);
        chan.id
    };
    
    // 6. channeledit
    let payload_edit = format!("channeledit cid={} channel_name=TestRoomEdited rt-127", created_cid);
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        &payload_edit,
        payload_edit.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    {
        let rt = runtime_arc.lock().unwrap();
        let channels = rt.store.channels.get(&server_id).unwrap();
        let chan = channels.iter().find(|c| c.id == created_cid).unwrap();
        assert_eq!(chan.name, "TestRoomEdited");
    }
    
    // 7. channeldelete
    let payload_delete = format!("channeldelete cid={} rt-128", created_cid);
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        &payload_delete,
        payload_delete.as_bytes(),
        &mut session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        server_id,
        packet_id,
    ).await;
    assert!(ok);
    
    {
        let rt = runtime_arc.lock().unwrap();
        let channels = rt.store.channels.get(&server_id).unwrap();
        assert!(channels.iter().all(|c| c.id != created_cid));
    }
    
    // Explicit cleanup
    cleanup_state_path(&state_path);
}

#[tokio::test]
async fn test_three_protocols_unified_interoperability() {
    let state_path = temp_state_path("three-protocols-unified");
    cleanup_state_path(&state_path);
    let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should build");
    let runtime_arc = std::sync::Arc::new(Mutex::new(runtime));

    let events = std::sync::Arc::new(Mutex::new(Vec::<blackteaspeak_server::transport::TransportNotification>::new()));
    {
        let events_clone = std::sync::Arc::clone(&events);
        runtime_arc.lock().unwrap().subscribe_events(Box::new(move |_rt, _sid, notif| {
            events_clone.lock().unwrap().push(notif.clone());
        }));
    }

    // 1. TS3 UDP client (clid=101) connects
    let socket = std::sync::Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 12346));
    let mut ts3_session = blackteaspeak_server::teaspeak_transport::UdpSession {
        client_id: 101,
        seed_client: vec![1, 2, 3],
        seed_server: vec![4, 5, 6],
        server_sec: p256::SecretKey::random(&mut rand::thread_rng()),
        shared_secret: [0u8; 64],
        client_uid: "ts3-uid-101".to_string(),
        server_uid: "server-uid".to_string(),
        server_packet_id: std::sync::Arc::new(Mutex::new(1)),
        server_ack_packet_id: 0,
        pending_rtc_describe: false,
        pending_rtc_is_video: false,
        rtc_describe_buffer: Vec::new(),
        ephemeral_sec: None,
        entry2_hash: None,
        client_nickname: "Ts3Peer".to_string(),
        command_fragment_buffer: Vec::new(),
        command_fragment_active: false,
        is_active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        is_teaspeak_client: false,
        is_ts6_client: false,
    };
    let iv_struct = [0u8; 20];
    let ok = blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
        "clientinit client_nickname=Ts3Peer",
        b"clientinit client_nickname=Ts3Peer",
        &mut ts3_session,
        addr,
        &socket,
        &iv_struct,
        &runtime_arc,
        1,
        1,
    )
    .await;
    assert!(ok);

    // 2. BTEA Desktop client (clid=1001) connects and sees Ts3Peer in initial notifycliententerview
    let mut btea_handler = blackteaspeak_server::transport::handlers::btea::BteaSessionHandler::new(
        1001,
        "127.0.0.1".to_string(),
    );
    let (btea_init_lines, _btea_notifs) = {
        let mut rt = runtime_arc.lock().unwrap();
        btea_handler.handle_command("clientinit client_nickname=BteaPeer", &mut rt)
    };
    assert!(
        btea_init_lines[0].contains("aclid=1001 client_id=1001"),
        "initserver should contain aclid and client_id for BTEA client"
    );
    assert!(
        btea_init_lines
            .iter()
            .any(|line| line.starts_with("notifycliententerview") && line.contains("clid=101") && line.contains("client_nickname=Ts3Peer")),
        "BTEA client should receive notifycliententerview for existing TS3 peer"
    );

    // 3. Web client (clid=20001) joins on the same runtime
    {
        let mut rt = runtime_arc.lock().unwrap();
        rt.upsert_web_client(
            20001,
            1,
            1,
            "WebPeer".to_string(),
            "web-uid-20001".to_string(),
            21001,
            "BlackTeaWeb".to_string(),
            "Web".to_string(),
            "127.0.0.1".to_string(),
        );
    }

    // 4. BTEA Desktop client moves to channel 2 and sends a channel chat message
    {
        let mut rt = runtime_arc.lock().unwrap();
        let (move_resp, _) = btea_handler.handle_command("clientmove clid=1001 cid=2", &mut rt);
        assert!(move_resp[0].contains("error id=0 msg=ok"));
        let (chat_resp, _) = btea_handler.handle_command("sendtextmessage targetmode=2 target=2 msg=HelloFromBtea", &mut rt);
        assert!(chat_resp[0].contains("error id=0 msg=ok"));

        // Verify all 3 clients remain in online_clients
        assert!(rt.store.online_clients.contains_key(&101));
        assert_eq!(rt.store.online_clients.get(&1001).unwrap().channel_id, 2);
        assert!(rt.store.online_clients.contains_key(&20001));
    }

    // 5. Verify event bus captured ClientEnterView, ClientMoved, and TextMessage across protocols
    {
        let recorded = events.lock().unwrap();
        assert!(recorded.iter().any(|n| matches!(
            n,
            blackteaspeak_server::transport::TransportNotification::ClientEnterView { presence, .. }
            if presence.client_id == 101 && presence.login_name == "Ts3Peer"
        )));
        assert!(recorded.iter().any(|n| matches!(
            n,
            blackteaspeak_server::transport::TransportNotification::ClientEnterView { presence, .. }
            if presence.client_id == 1001 && presence.login_name == "BteaPeer"
        )));
        assert!(recorded.iter().any(|n| matches!(
            n,
            blackteaspeak_server::transport::TransportNotification::ClientMoved { presence, from_channel_id: 1, .. }
            if presence.client_id == 1001 && presence.channel_id == 2
        )));
        assert!(recorded.iter().any(|n| matches!(
            n,
            blackteaspeak_server::transport::TransportNotification::TextMessage { invoker_id: 1001, invoker_name, .. }
            if invoker_name == "BteaPeer"
        )));
    }

    cleanup_state_path(&state_path);
}

#[tokio::test]
async fn test_live_wire_crypto_handshakes_and_voice_all_three_protocols() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use sha2::{Digest, Sha512};
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};

    let state_path = temp_state_path("live-wire-crypto-handshakes");
    cleanup_state_path(&state_path);
    let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should build");
    let runtime_arc = std::sync::Arc::new(Mutex::new(runtime));
    let shared_secrets = std::sync::Arc::new(Mutex::new(std::collections::HashMap::<u64, Vec<u8>>::new()));
    let should_stop = std::sync::Arc::new(AtomicBool::new(false));

    // 1. Start DesktopTcpTransportServer on an ephemeral port
    let tcp_server = blackteaspeak_server::desktop_tcp_transport::DesktopTcpTransportServer::bind_with_shared_runtime(
        1,
        std::sync::Arc::clone(&runtime_arc),
        "127.0.0.1:0",
        std::sync::Arc::clone(&shared_secrets),
    )
    .expect("desktop tcp server should bind");
    let tcp_addr = tcp_server.local_addr().expect("tcp local addr");
    let stop_tcp = std::sync::Arc::clone(&should_stop);
    std::thread::spawn(move || {
        let _ = tcp_server.run(stop_tcp);
    });

    // 2. Pick a free UDP port and start TeaSpeakTransportServer with shared_secrets (handling both TS3 UDP and BTEA UDP)
    let udp_port = {
        let tmp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        tmp.local_addr().unwrap().port()
    };
    let udp_bind_str = format!("127.0.0.1:{}", udp_port);
    let udp_addr: std::net::SocketAddr = udp_bind_str.parse().unwrap();
    let ts3_server = blackteaspeak_server::teaspeak_transport::TeaSpeakTransportServer::bind_with_shared_runtime(
        1,
        std::sync::Arc::clone(&runtime_arc),
        &udp_bind_str,
    )
    .expect("teaspeak udp server should bind")
    .with_shared_secrets(std::sync::Arc::clone(&shared_secrets));
    let stop_udp = std::sync::Arc::clone(&should_stop);
    std::thread::spawn(move || {
        let _ = ts3_server.run(stop_udp);
    });

    // Give the UDP thread a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 3. Connect Live TS3 UDP Client and execute full TS3INIT1 + ECDH P-256 + AES-128-EAX handshake
    let ts3_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    ts3_sock.connect(udp_addr).await.unwrap();

    // Step 3a: Send TS3Init GetCookie (34 bytes)
    let mut get_cookie = [0u8; 34];
    get_cookie[0..8].copy_from_slice(b"TS3INIT1");
    get_cookie[8..10].copy_from_slice(&101u16.to_be_bytes());
    get_cookie[10..12].copy_from_slice(&0u16.to_be_bytes());
    get_cookie[12] = 0x88;
    get_cookie[13 + 4] = 0x00; // GetCookie command
    get_cookie[13 + 9..13 + 13].copy_from_slice(&0xCAFEBABEu32.to_le_bytes());
    ts3_sock.send(&get_cookie).await.unwrap();

    let mut recv_buf = [0u8; 4096];
    let len = tokio::time::timeout(std::time::Duration::from_secs(2), ts3_sock.recv(&mut recv_buf))
        .await
        .expect("SetCookie timeout")
        .expect("SetCookie recv");
    assert_eq!(len, 32, "SetCookie must be 32 bytes");
    assert_eq!(&recv_buf[0..8], b"TS3INIT1");
    assert_eq!(recv_buf[11], 0x01); // SetCookie
    let cookie = u64::from_le_bytes(recv_buf[12..20].try_into().unwrap());
    let echoed_seq = u32::from_le_bytes(recv_buf[28..32].try_into().unwrap());
    assert_eq!(echoed_seq, 0xCAFEBABE);

    // Step 3b: Send TS3Init GetPuzzle (38 bytes)
    let mut get_puzzle = [0u8; 38];
    get_puzzle[0..8].copy_from_slice(b"TS3INIT1");
    get_puzzle[8..10].copy_from_slice(&101u16.to_be_bytes());
    get_puzzle[12] = 0x88;
    get_puzzle[13 + 4] = 0x02; // GetPuzzle
    get_puzzle[13 + 5..13 + 13].copy_from_slice(&cookie.to_le_bytes());
    ts3_sock.send(&get_puzzle).await.unwrap();

    let len = tokio::time::timeout(std::time::Duration::from_secs(2), ts3_sock.recv(&mut recv_buf))
        .await
        .expect("SetPuzzle timeout")
        .expect("SetPuzzle recv");
    assert_eq!(len, 244, "SetPuzzle must be 244 bytes");
    assert_eq!(recv_buf[11], 0x03); // SetPuzzle

    // Step 3c: Send TS3Init SolvePuzzle (TeaSpeak ECDH P-256 clientinitiv)
    let (client_p256_sec, client_p256_pub) = blackteaspeak_server::desktop_crypto::generate_server_keypair();
    let client_omega_asn1 = blackteaspeak_server::desktop_crypto::export_public_key_libtomcrypt_asn1(&client_p256_pub);
    let client_alpha_bytes = [0x42u8; 10];
    let alpha_b64 = blackteaspeak_server::teaspeak_ts3::ts3_escape(&BASE64_STANDARD.encode(client_alpha_bytes));
    let omega_b64 = blackteaspeak_server::teaspeak_ts3::ts3_escape(&BASE64_STANDARD.encode(&client_omega_asn1));
    let solve_cmd = format!(
        "clientinitiv alpha={} omega={} ip=127.0.0.1 ot=1 teaspeak=1",
        alpha_b64, omega_b64
    );
    let mut solve_pkt = vec![0u8; 18];
    solve_pkt[0..8].copy_from_slice(b"TS3INIT1");
    solve_pkt[8..10].copy_from_slice(&101u16.to_be_bytes());
    solve_pkt[12] = 0x88;
    solve_pkt[13 + 4] = 0x04; // SolvePuzzle
    solve_pkt.extend_from_slice(solve_cmd.as_bytes());
    ts3_sock.send(&solve_pkt).await.unwrap();

    let len = tokio::time::timeout(std::time::Duration::from_secs(2), ts3_sock.recv(&mut recv_buf))
        .await
        .expect("initivexpand timeout")
        .expect("initivexpand recv");
    let initivexpand_str = String::from_utf8_lossy(&recv_buf[11..len]).to_string();
    assert!(initivexpand_str.starts_with("initivexpand "), "Expected initivexpand, got: {}", initivexpand_str);

    let beta_esc = extract_field(&initivexpand_str, "beta").expect("beta field");
    let server_omega_esc = extract_field(&initivexpand_str, "omega").expect("omega field");
    let beta_bytes = BASE64_STANDARD
        .decode(blackteaspeak_server::teaspeak_ts3::ts3_unescape(&beta_esc))
        .expect("decode beta");
    let server_omega_bytes = BASE64_STANDARD
        .decode(blackteaspeak_server::teaspeak_ts3::ts3_unescape(&server_omega_esc))
        .expect("decode server omega");
    let server_sec1 = blackteaspeak_server::desktop_crypto::import_public_key_libtomcrypt_asn1(&server_omega_bytes)
        .expect("import server sec1");
    let shared_20 = blackteaspeak_server::desktop_crypto::calculate_shared_secret(&server_sec1, &client_p256_sec)
        .expect("ECDH shared secret");
    let mut ts3_shared_secret = [0u8; 64];
    ts3_shared_secret[0..20].copy_from_slice(&shared_20);
    let ts3_iv_struct = blackteaspeak_server::desktop_crypto::derive_iv_struct(
        &ts3_shared_secret,
        &client_alpha_bytes,
        &beta_bytes,
    );

    // Step 3d: Encrypt and send clientinit over TS3 UDP
    let ts3_init_payload = b"clientinit client_nickname=LiveTs3Client";
    let ts3_pkt_id = 1u16;
    let mut ts3_hdr = [0u8; 5];
    ts3_hdr[0..2].copy_from_slice(&ts3_pkt_id.to_be_bytes());
    ts3_hdr[2..4].copy_from_slice(&0u16.to_be_bytes());
    ts3_hdr[4] = 0x22; // NewProtocol | Command
    let ts3_enc = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        ts3_pkt_id,
        0,
        0x22,
        &ts3_hdr,
        ts3_init_payload,
        &ts3_iv_struct,
        false,
    );
    let mut ts3_wire_pkt = Vec::with_capacity(13 + ts3_enc.len() - 8);
    ts3_wire_pkt.extend_from_slice(&ts3_enc[0..8]);
    ts3_wire_pkt.extend_from_slice(&ts3_hdr);
    ts3_wire_pkt.extend_from_slice(&ts3_enc[8..]);
    ts3_sock.send(&ts3_wire_pkt).await.unwrap();

    // Receive TS3 server responses (ACK + initserver fragments + error ok) and decrypt them
    let mut ts3_received_initserver = false;
    for _ in 0..30 {
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), ts3_sock.recv(&mut recv_buf)).await {
            if n >= 11 {
                let s_pkt_id = u16::from_be_bytes([recv_buf[8], recv_buf[9]]);
                let s_flags = recv_buf[10];
                if (s_flags & 0x80) == 0 {
                    let s_hdr = &recv_buf[8..11];
                    let mut payload_with_mac = Vec::new();
                    payload_with_mac.extend_from_slice(&recv_buf[0..8]);
                    payload_with_mac.extend_from_slice(&recv_buf[11..n]);
                    if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
                        s_pkt_id,
                        0,
                        s_flags,
                        s_hdr,
                        &payload_with_mac,
                        &ts3_iv_struct,
                        true,
                    ) {
                        let text = String::from_utf8_lossy(&dec);
                        if text.contains("initserver") {
                            ts3_received_initserver = true;
                        }
                        if text.contains("error id=0 msg=ok") {
                            break;
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
    assert!(ts3_received_initserver, "TS3 UDP client must receive and decrypt initserver");

    // 4. Connect Live BTEA Desktop Client over TCP + UDP (exact X25519 + SHA512 + AES-128-EAX handshake)
    let mut btea_tcp = std::net::TcpStream::connect(tcp_addr).expect("connect BTEA TCP");
    btea_tcp.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
    let btea_client_sec = x25519_dalek::EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let btea_client_pub = x25519_dalek::PublicKey::from(&btea_client_sec);
    let mut btea_init_req = [0u8; 45];
    btea_init_req[0..4].copy_from_slice(&41u32.to_be_bytes());
    btea_init_req[4..12].copy_from_slice(b"BTEAINIT");
    btea_init_req[12] = 0x01;
    btea_init_req[13..45].copy_from_slice(btea_client_pub.as_bytes());
    btea_tcp.write_all(&btea_init_req).unwrap();

    let mut btea_init_resp = [0u8; 45];
    btea_tcp.read_exact(&mut btea_init_resp).unwrap();
    assert_eq!(u32::from_be_bytes(btea_init_resp[0..4].try_into().unwrap()), 41);
    assert_eq!(&btea_init_resp[4..12], b"BTEAINIT");
    assert_eq!(btea_init_resp[12], 0x02);
    let mut srv_x25519_bytes = [0u8; 32];
    srv_x25519_bytes.copy_from_slice(&btea_init_resp[13..45]);
    let srv_x25519_pub = x25519_dalek::PublicKey::from(srv_x25519_bytes);
    let dh_shared = btea_client_sec.diffie_hellman(&srv_x25519_pub);
    let mut hasher = Sha512::new();
    hasher.update(b"BTEA_SECRET_DERIVATION");
    hasher.update(dh_shared.as_bytes());
    let btea_secret = hasher.finalize().to_vec();

    // Send encrypted clientinit over BTEA TCP
    let btea_init_cmd = b"clientinit client_nickname=LiveBteaClient";
    let mut btea_hdr = [0u8; 5];
    btea_hdr[0..2].copy_from_slice(&1u16.to_be_bytes());
    btea_hdr[2..4].copy_from_slice(&0u16.to_be_bytes());
    btea_hdr[4] = 0x02;
    let btea_enc = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        1,
        0,
        0x02,
        &btea_hdr,
        btea_init_cmd,
        &btea_secret,
        false,
    );
    let mut btea_frame = Vec::new();
    let frame_len = (13 + btea_enc.len() - 8) as u32;
    btea_frame.extend_from_slice(&frame_len.to_be_bytes());
    btea_frame.extend_from_slice(&btea_enc[0..8]);
    btea_frame.extend_from_slice(&btea_hdr);
    btea_frame.extend_from_slice(&btea_enc[8..]);
    btea_tcp.write_all(&btea_frame).unwrap();

    // Read encrypted clientinit response over BTEA TCP
    let mut len_buf = [0u8; 4];
    btea_tcp.read_exact(&mut len_buf).unwrap();
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut resp_pkt = vec![0u8; resp_len];
    btea_tcp.read_exact(&mut resp_pkt).unwrap();
    let assigned_btea_clid = u16::from_be_bytes([resp_pkt[10], resp_pkt[11]]);
    assert!(assigned_btea_clid >= 1001, "Expected BTEA client ID >= 1001, got {}", assigned_btea_clid);
    let resp_pkt_id = u16::from_be_bytes([resp_pkt[8], resp_pkt[9]]);
    let resp_flags = resp_pkt[12];
    let mut resp_payload_mac = Vec::new();
    resp_payload_mac.extend_from_slice(&resp_pkt[0..8]);
    resp_payload_mac.extend_from_slice(&resp_pkt[13..]);
    let btea_dec = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
        resp_pkt_id,
        0,
        resp_flags,
        &resp_pkt[8..13],
        &resp_payload_mac,
        &btea_secret,
        true,
    )
    .expect("BTEA TCP response must decrypt cleanly");
    let btea_init_text = String::from_utf8(btea_dec).unwrap();
    assert!(btea_init_text.contains("initserver"));
    assert!(
        btea_init_text.contains("client_nickname=LiveTs3Client"),
        "BTEA client should see LiveTs3Client in initial snapshot: {}",
        btea_init_text
    );

    // Verify LiveTs3Client received encrypted notifycliententerview for LiveBteaClient over UDP
    let mut ts3_saw_btea_join = false;
    for _ in 0..5 {
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), ts3_sock.recv(&mut recv_buf)).await {
            if n >= 11 && (recv_buf[10] & 0x80) == 0 {
                let s_pkt_id = u16::from_be_bytes([recv_buf[8], recv_buf[9]]);
                let s_flags = recv_buf[10];
                let mut payload_with_mac = Vec::new();
                payload_with_mac.extend_from_slice(&recv_buf[0..8]);
                payload_with_mac.extend_from_slice(&recv_buf[11..n]);
                if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
                    s_pkt_id,
                    0,
                    s_flags,
                    &recv_buf[8..11],
                    &payload_with_mac,
                    &ts3_iv_struct,
                    true,
                ) {
                    let text = String::from_utf8_lossy(&dec);
                    if text.contains("notifycliententerview") && text.contains("LiveBteaClient") {
                        ts3_saw_btea_join = true;
                        break;
                    }
                }
            }
        }
    }
    assert!(ts3_saw_btea_join, "TS3 UDP client must receive encrypted notifycliententerview when BTEA client joins");

    // 5. Connect BTEA UDP socket to the shared UDP port and test bidirectional encrypted Opus voice routing
    let btea_udp = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    btea_udp.connect(udp_addr).await.unwrap();

    // Send BTEA UDP initial keepalive (flags = 0x00, header[2..4] = assigned_btea_clid, generation_id = 0)
    let mut ka_hdr = [0u8; 5];
    ka_hdr[0..2].copy_from_slice(&4u16.to_be_bytes());
    ka_hdr[2..4].copy_from_slice(&assigned_btea_clid.to_be_bytes());
    ka_hdr[4] = 0x00;
    let ka_enc = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        4,
        0,
        0x00,
        &ka_hdr,
        &[],
        &btea_secret,
        false,
    );
    let mut ka_pkt = Vec::new();
    ka_pkt.extend_from_slice(&ka_enc[0..8]);
    ka_pkt.extend_from_slice(&ka_hdr);
    ka_pkt.extend_from_slice(&ka_enc[8..]);
    btea_udp.send(&ka_pkt).await.unwrap();

    // Step 5a: BTEA UDP sends encrypted Opus voice packet (0x0A) -> TS3 UDP receives TS3 voice packet (0xA0)
    let opus_from_btea = vec![0xF8, 0xFF, 0xFE, 0x11, 0x22];
    let mut v_hdr = [0u8; 5];
    v_hdr[0..2].copy_from_slice(&5u16.to_be_bytes());
    v_hdr[2..4].copy_from_slice(&assigned_btea_clid.to_be_bytes());
    v_hdr[4] = 0x0A;
    let v_enc = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        5,
        0,
        0x0A,
        &v_hdr,
        &opus_from_btea,
        &btea_secret,
        false,
    );
    let mut v_pkt = Vec::new();
    v_pkt.extend_from_slice(&v_enc[0..8]);
    v_pkt.extend_from_slice(&v_hdr);
    v_pkt.extend_from_slice(&v_enc[8..]);
    btea_udp.send(&v_pkt).await.unwrap();

    let mut ts3_received_voice = false;
    for _ in 0..5 {
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(500), ts3_sock.recv(&mut recv_buf)).await {
            if n >= 16 && recv_buf[10] == 0xA0 {
                let sender_id = u16::from_be_bytes([recv_buf[13], recv_buf[14]]);
                let codec = recv_buf[15];
                let opus_payload = &recv_buf[16..n];
                if sender_id == assigned_btea_clid && codec == 4 && opus_payload == opus_from_btea.as_slice() {
                    ts3_received_voice = true;
                    break;
                }
            }
        }
    }
    assert!(ts3_received_voice, "TS3 UDP client must receive Opus audio sent by BTEA UDP client");

    // Step 5b: TS3 UDP sends encrypted Opus voice packet (0x20) -> BTEA UDP receives and decrypts BTEA voice packet (0x0A)
    let opus_from_ts3 = vec![0xF8, 0xFF, 0xFE, 0x33, 0x44];
    let mut ts3_v_payload = vec![0x00, 0x01, 0x04]; // 2-byte v_pkt_id + codec=4 (Opus Music/Voice)
    ts3_v_payload.extend_from_slice(&opus_from_ts3);
    let mut ts3_v_hdr = [0u8; 5];
    ts3_v_hdr[0..2].copy_from_slice(&2u16.to_be_bytes());
    ts3_v_hdr[2..4].copy_from_slice(&0u16.to_be_bytes());
    ts3_v_hdr[4] = 0x20; // NewProtocol | Voice (0x00)
    let ts3_v_enc = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        2,
        0,
        0x20,
        &ts3_v_hdr,
        &ts3_v_payload,
        &ts3_iv_struct,
        false,
    );
    let mut ts3_v_wire = Vec::new();
    ts3_v_wire.extend_from_slice(&ts3_v_enc[0..8]);
    ts3_v_wire.extend_from_slice(&ts3_v_hdr);
    ts3_v_wire.extend_from_slice(&ts3_v_enc[8..]);
    ts3_sock.send(&ts3_v_wire).await.unwrap();

    let mut btea_received_voice = false;
    for _ in 0..5 {
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(500), btea_udp.recv(&mut recv_buf)).await {
            if n > 13 {
                let p_id = u16::from_be_bytes([recv_buf[8], recv_buf[9]]);
                let sender_id = u16::from_be_bytes([recv_buf[10], recv_buf[11]]);
                let p_flags = recv_buf[12];
                let mut payload_mac = Vec::new();
                payload_mac.extend_from_slice(&recv_buf[0..8]);
                payload_mac.extend_from_slice(&recv_buf[13..n]);
                if let Some(dec_opus) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
                    p_id,
                    0,
                    p_flags,
                    &recv_buf[8..13],
                    &payload_mac,
                    &btea_secret,
                    true,
                ) {
                    if p_flags == 0x0A && sender_id >= 101 && dec_opus == opus_from_ts3 {
                        btea_received_voice = true;
                        break;
                    }
                }
            }
        }
    }
    assert!(btea_received_voice, "BTEA UDP client must receive and decrypt Opus audio sent by TS3 UDP client");

    // 6. Start BlackTeaWebTransportServer (HTTP/3 QUIC WebTransport) and connect a real wtransport client
    let tls_dir = TempDirectory::new(unique_temp_dir("live-wt-tls"));
    let cert_path = tls_dir.path().join("cert.pem");
    let key_path = tls_dir.path().join("key.pem");
    blackteaspeak_server::web::generate_localhost_tls_assets(&cert_path, &key_path)
        .expect("generate localhost tls assets");

    let cert_hash_bytes: [u8; 32] = {
        let file = std::fs::File::open(&cert_path).unwrap();
        let mut reader = std::io::BufReader::new(file);
        let cert_der = rustls_pemfile::certs(&mut reader).next().unwrap().unwrap();
        <sha2::Sha256 as sha2::Digest>::digest(&cert_der).into()
    };

    let wt_port = {
        let tmp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        tmp.local_addr().unwrap().port()
    };
    let wt_bind_str = format!("127.0.0.1:{}", wt_port);
    let file_transfers = std::sync::Arc::new(blackteaspeak_server::file_transfer::FileTransferRegistry::new(workspace_root()));
    let web_server = blackteaspeak_server::web::BlackTeaWebTransportServer::bind_with_shared_runtime(
        std::sync::Arc::clone(&runtime_arc),
        &wt_bind_str,
        &cert_path,
        &key_path,
        file_transfers,
    )
    .expect("bind BlackTeaWebTransportServer");
    std::thread::spawn(move || {
        let _ = web_server.run();
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let wt_client_cfg = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([wtransport::tls::Sha256Digest::new(cert_hash_bytes)])
        .build();
    let wt_endpoint = wtransport::Endpoint::client(wt_client_cfg).expect("wtransport client endpoint");
    let wt_url = format!("https://127.0.0.1:{}/", wt_port);
    let wt_conn = tokio::time::timeout(std::time::Duration::from_secs(3), wt_endpoint.connect(&wt_url))
        .await
        .expect("WebTransport connect timeout")
        .expect("WebTransport connect");

    let (mut wt_send, wt_recv) = wt_conn.open_bi().await.expect("open_bi").await.expect("bi stream ready");
    let mut wt_reader = tokio::io::BufReader::new(wt_recv);
    use tokio::io::AsyncBufReadExt;

    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"handshakebegin\",\"data\":[{\"intention\":\"0\",\"authentication_method\":\"0\",\"public_key\":\"test-pub\"}]}\n")
        .await
        .expect("send WebTransport handshakebegin");
    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"handshakeindentityproof\",\"data\":[{\"proof\":\"test-proof\"}]}\n")
        .await
        .expect("send WebTransport handshakeindentityproof");
    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"clientinit\",\"data\":[{\"client_nickname\":\"LiveWebClient\",\"client_version\":\"BlackTeaWeb\",\"client_platform\":\"Web\"}]}\n")
        .await
        .expect("send WebTransport clientinit");

    let mut wt_init_lines = Vec::new();
    for _ in 0..25 {
        let mut line = String::new();
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), wt_reader.read_line(&mut line)).await {
            if n == 0 {
                break;
            }
            wt_init_lines.push(line.clone());
            if line.contains("channellistfinished") {
                break;
            }
        } else {
            break;
        }
    }
    let wt_joined_output = wt_init_lines.join("");
    assert!(wt_joined_output.contains("initserver"), "WebTransport client must receive initserver: {}", wt_joined_output);
    assert!(
        wt_joined_output.contains("LiveTs3Client") && wt_joined_output.contains("LiveBteaClient"),
        "WebTransport client must see both LiveTs3Client and LiveBteaClient on join: {}",
        wt_joined_output
    );

    // Step 6a: WebTransport client sends Opus voice datagram -> BTEA UDP and TS3 UDP receive it!
    let opus_from_web = vec![0xF8, 0xFF, 0xFE, 0x55, 0x66];
    let mut wt_dgram_out = vec![0x01]; // packet_type = 1 (voice)
    wt_dgram_out.extend_from_slice(&opus_from_web);
    wt_conn.send_datagram(&wt_dgram_out).expect("send WebTransport datagram");

    let mut btea_received_web_voice = false;
    for _ in 0..8 {
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(400), btea_udp.recv(&mut recv_buf)).await {
            if n > 13 {
                let p_id = u16::from_be_bytes([recv_buf[8], recv_buf[9]]);
                let sender_id = u16::from_be_bytes([recv_buf[10], recv_buf[11]]);
                let p_flags = recv_buf[12];
                let mut payload_mac = Vec::new();
                payload_mac.extend_from_slice(&recv_buf[0..8]);
                payload_mac.extend_from_slice(&recv_buf[13..n]);
                if let Some(dec_opus) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
                    p_id,
                    0,
                    p_flags,
                    &recv_buf[8..13],
                    &payload_mac,
                    &btea_secret,
                    true,
                ) {
                    if p_flags == 0x0A && sender_id >= 20001 && dec_opus == opus_from_web {
                        btea_received_web_voice = true;
                        break;
                    }
                }
            }
        }
    }
    assert!(btea_received_web_voice, "BTEA UDP client must receive Opus audio sent by WebTransport client");

    // Step 6b: BTEA UDP sends Opus voice packet -> WebTransport client receives QUIC datagram!
    let opus_btea_to_web = vec![0xF8, 0xFF, 0xFE, 0x77, 0x88];
    let mut v2_hdr = [0u8; 5];
    v2_hdr[0..2].copy_from_slice(&6u16.to_be_bytes());
    v2_hdr[2..4].copy_from_slice(&assigned_btea_clid.to_be_bytes());
    v2_hdr[4] = 0x0A;
    let v2_enc = blackteaspeak_server::desktop_crypto::encrypt_btea_packet(
        6,
        0,
        0x0A,
        &v2_hdr,
        &opus_btea_to_web,
        &btea_secret,
        false,
    );
    let mut v2_pkt = Vec::new();
    v2_pkt.extend_from_slice(&v2_enc[0..8]);
    v2_pkt.extend_from_slice(&v2_hdr);
    v2_pkt.extend_from_slice(&v2_enc[8..]);
    btea_udp.send(&v2_pkt).await.unwrap();

    let wt_dgram_in = tokio::time::timeout(std::time::Duration::from_secs(2), wt_conn.receive_datagram())
        .await
        .expect("WebTransport datagram recv timeout")
        .expect("WebTransport datagram recv");
    let dgram_bytes = wt_dgram_in.payload();
    assert!(dgram_bytes.len() > 9, "WebTransport datagram must contain 1-byte type + 8-byte clientId + Opus");
    assert_eq!(dgram_bytes[0], 1, "WebTransport voice datagram packet_type must be 1");
    let wt_sender_id = u64::from_le_bytes(dgram_bytes[1..9].try_into().unwrap());
    assert_eq!(wt_sender_id, assigned_btea_clid as u64);
    assert_eq!(&dgram_bytes[9..], opus_btea_to_web.as_slice());

    // 7. Test Official TS3 Client (ot=1 without teaspeak=1): initivexpand2 + Ed25519 license chain + clientek
    let ts3_ot1_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    ts3_ot1_sock.connect(udp_addr).await.unwrap();

    ts3_ot1_sock.send(&get_cookie).await.unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), ts3_ot1_sock.recv(&mut recv_buf))
        .await
        .expect("ot=1 SetCookie timeout")
        .expect("ot=1 SetCookie recv");

    ts3_ot1_sock.send(&get_puzzle).await.unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), ts3_ot1_sock.recv(&mut recv_buf))
        .await
        .expect("ot=1 SetPuzzle timeout")
        .expect("ot=1 SetPuzzle recv");

    let ot1_alpha_bytes = [0x24u8; 32];
    let ot1_alpha_b64 = blackteaspeak_server::teaspeak_ts3::ts3_escape(&BASE64_STANDARD.encode(ot1_alpha_bytes));
    let ot1_solve_cmd = format!(
        "clientinitiv alpha={} omega={} ip=127.0.0.1 ot=1",
        ot1_alpha_b64, omega_b64
    );
    let mut ot1_solve_pkt = vec![0u8; 18];
    ot1_solve_pkt[0..8].copy_from_slice(b"TS3INIT1");
    ot1_solve_pkt[8..10].copy_from_slice(&101u16.to_be_bytes());
    ot1_solve_pkt[12] = 0x88;
    ot1_solve_pkt[13 + 4] = 0x04;
    ot1_solve_pkt.extend_from_slice(ot1_solve_cmd.as_bytes());
    ts3_ot1_sock.send(&ot1_solve_pkt).await.unwrap();

    // Receive and reassemble fragmented initivexpand2 encrypted with DUMMY_KEY
    let mut initivexpand2_bytes = Vec::new();
    for _ in 0..6 {
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), ts3_ot1_sock.recv(&mut recv_buf)).await {
            if n >= 11 {
                let pkt_id = u16::from_be_bytes([recv_buf[8], recv_buf[9]]);
                let hdr = &recv_buf[8..11];
                let mac: [u8; 8] = recv_buf[0..8].try_into().unwrap();
                let chunk = blackteaspeak_server::desktop_crypto::decrypt_with_dummy_key(
                    pkt_id,
                    hdr,
                    &mac,
                    &recv_buf[11..n],
                )
                .expect("initivexpand2 fragment should decrypt with dummy key");
                initivexpand2_bytes.extend_from_slice(&chunk);
                if initivexpand2_bytes.windows(4).any(|w| w == b"ot=1") {
                    break;
                }
            }
        }
    }
    let initivexpand2_str = String::from_utf8(initivexpand2_bytes).expect("valid utf8 initivexpand2");
    assert!(initivexpand2_str.starts_with("initivexpand2 "), "Expected initivexpand2, got: {}", initivexpand2_str);

    let ot1_beta_esc = extract_field(&initivexpand2_str, "beta").expect("ot=1 beta");
    let ot1_chain_esc = extract_field(&initivexpand2_str, "l").expect("ot=1 chain l");
    let ot1_beta_bytes = BASE64_STANDARD
        .decode(blackteaspeak_server::teaspeak_ts3::ts3_unescape(&ot1_beta_esc))
        .expect("decode ot=1 beta");
    let ot1_chain_bytes = BASE64_STANDARD
        .decode(blackteaspeak_server::teaspeak_ts3::ts3_unescape(&ot1_chain_esc))
        .expect("decode ot=1 chain");
    assert_eq!(ot1_beta_bytes.len(), 54, "ot=1 beta must be 54 bytes");
    assert!(ot1_chain_bytes.len() > 42, "ot=1 chain must contain ephemeral entry");

    // Extract the 42-byte ephemeral entry appended at the end of the chain
    let entry2_bytes = &ot1_chain_bytes[ot1_chain_bytes.len() - 42..];
    assert_eq!(entry2_bytes[0], 0x00);
    assert_eq!(entry2_bytes[33], 0x20); // LicenseType::EPHEMERAL
    let ephemeral_pub_bytes: [u8; 32] = entry2_bytes[1..33].try_into().unwrap();
    let entry2_hash: [u8; 64] = Sha512::digest(&entry2_bytes[1..]).into();

    // Derive server's combined public key point and compute shared_secret_64 via clientek
    let (_, identity_sec, _) = blackteaspeak_server::desktop_crypto::load_protocol_key().unwrap();
    let parent_private = curve25519_dalek::scalar::Scalar::from_bytes_mod_order(identity_sec.try_into().unwrap());
    let parent_public_point = &parent_private * curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
    let ephemeral_public_point = curve25519_dalek::edwards::CompressedEdwardsY(ephemeral_pub_bytes)
        .decompress()
        .expect("decompress ephemeral public key");

    let mut clamped_hash_bytes = [0u8; 32];
    clamped_hash_bytes.copy_from_slice(&entry2_hash[0..32]);
    clamped_hash_bytes[0] &= 0xF8;
    clamped_hash_bytes[31] &= 0x3F;
    clamped_hash_bytes[31] |= 0x40;
    let clamped_hash = curve25519_dalek::scalar::Scalar::from_bytes_mod_order(clamped_hash_bytes);
    let derived_server_pub_point = (ephemeral_public_point * clamped_hash) + parent_public_point;
    let mut derived_server_pub_flipped = derived_server_pub_point.compress().to_bytes();
    derived_server_pub_flipped[31] ^= 0x80;

    let mut client_ek_priv = [0x11u8; 32];
    client_ek_priv[0] &= 0xF8;
    client_ek_priv[31] &= 0x3F;
    client_ek_priv[31] |= 0x40;
    let client_ek_scalar = curve25519_dalek::scalar::Scalar::from_bytes_mod_order(client_ek_priv);
    let mut client_ek_pub_flipped = (&client_ek_scalar * curve25519_dalek::constants::ED25519_BASEPOINT_TABLE)
        .compress()
        .to_bytes();
    client_ek_pub_flipped[31] ^= 0x80;

    let ot1_shared_secret = blackteaspeak_server::desktop_crypto::get_shared_secret2(
        &derived_server_pub_flipped,
        &client_ek_scalar.to_bytes(),
    )
    .expect("client compute get_shared_secret2");
    let ot1_iv_struct = blackteaspeak_server::desktop_crypto::derive_iv_struct(
        &ot1_shared_secret,
        &ot1_alpha_bytes,
        &ot1_beta_bytes,
    );

    let ek_b64 = blackteaspeak_server::teaspeak_ts3::ts3_escape(&BASE64_STANDARD.encode(client_ek_pub_flipped));
    let clientek_cmd = format!("clientek ek={} proof=AA==", ek_b64);
    let mut ek_hdr = [0u8; 5];
    ek_hdr[0..2].copy_from_slice(&1u16.to_be_bytes());
    ek_hdr[2..4].copy_from_slice(&0u16.to_be_bytes());
    ek_hdr[4] = 0x22;
    let ek_wire = blackteaspeak_server::desktop_crypto::encrypt_with_dummy_key(1, &ek_hdr, clientek_cmd.as_bytes());
    ts3_ot1_sock.send(&ek_wire).await.unwrap();

    let ack_len = tokio::time::timeout(std::time::Duration::from_secs(2), ts3_ot1_sock.recv(&mut recv_buf))
        .await
        .expect("clientek ACK timeout")
        .expect("clientek ACK recv");
    assert_eq!(ack_len, 13, "clientek ACK must be 13 bytes");
    assert_eq!(recv_buf[10], 0xA6, "clientek ACK flags must be 0xA6");
    let expected_ack_mac = {
        use sha1::{Digest as _, Sha1};
        let h = Sha1::digest(&ot1_iv_struct);
        let mut m = [0u8; 8];
        m.copy_from_slice(&h[0..8]);
        m
    };
    assert_eq!(
        &recv_buf[0..8],
        &expected_ack_mac,
        "clientek ACK MAC must match SHA1(iv_struct)[0..8], proving identical 64-byte Ed25519 shared secret derivation"
    );

    should_stop.store(true, Ordering::SeqCst);
    cleanup_state_path(&state_path);
}

#[tokio::test]
async fn all_client_protocols_can_query_and_modify_permissions_when_authorized() {
    use std::sync::{Arc, Mutex as StdMutex, atomic::{AtomicBool, Ordering}};
    use tokio::io::AsyncBufReadExt;

    let state_path = temp_state_path("all-protocols-permissions");
    cleanup_state_path(&state_path);
    let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should build");
    let runtime_arc = Arc::new(StdMutex::new(runtime));

    // Create two Server Admin privilege keys (sgid=6) via ServerQuery admin session:
    // one for the BTEA Desktop client, one for the WebTransport client, and one for the TS3 UDP client.
    let (btea_token, web_token, ts3_token) = {
        let mut rt = runtime_arc.lock().unwrap();
        let mut admin = QuerySessionState::default();
        assert!(rt.execute("login serveradmin serveradmin", &mut admin).contains("error id=0 msg=ok"));
        assert!(rt.execute("use sid=1", &mut admin).contains("error id=0 msg=ok"));

        let t1_out = rt.execute(
            "tokenadd tokentype=0 tokenid1=6 tokenid2=0 tokendescription=BTEA\\sAdmin",
            &mut admin,
        );
        assert!(t1_out.contains("error id=0 msg=ok"), "tokenadd 1 failed: {}", t1_out);
        let t1 = extract_field(&t1_out, "token").expect("btea token");

        let t2_out = rt.execute(
            "tokenadd tokentype=0 tokenid1=6 tokenid2=0 tokendescription=Web\\sAdmin",
            &mut admin,
        );
        assert!(t2_out.contains("error id=0 msg=ok"), "tokenadd 2 failed: {}", t2_out);
        let t2 = extract_field(&t2_out, "token").expect("web token");

        let t3_out = rt.execute(
            "tokenadd tokentype=0 tokenid1=6 tokenid2=0 tokendescription=TS3\\sAdmin",
            &mut admin,
        );
        assert!(t3_out.contains("error id=0 msg=ok"), "tokenadd 3 failed: {}", t3_out);
        let t3 = extract_field(&t3_out, "token").expect("ts3 token");

        (t1, t2, t3)
    };

    // =========================================================================
    // 1. BTEA Desktop Client (BteaSessionHandler)
    // =========================================================================
    {
        let mut btea = blackteaspeak_server::transport::btea::BteaSessionHandler::new(501, "127.0.0.1".to_string());
        let mut rt = runtime_arc.lock().unwrap();

        // Connect via clientinit
        let (init_out, _) = btea.handle_command("clientinit client_nickname=BteaPermTester", &mut rt);
        let init_joined = init_out.join("\n");
        assert!(init_joined.contains("notifyservergrouplist"), "BTEA init must include notifyservergrouplist");
        assert!(init_joined.contains("notifychannelgrouplist"), "BTEA init must include notifychannelgrouplist");
        assert!(init_joined.contains("notifyclientneededpermissions"), "BTEA init must include notifyclientneededpermissions");

        // Query permission catalog, group lists, and effective permissions as Guest (sgid=8)
        let (permlist_out, _) = btea.handle_command("permissionlist", &mut rt);
        assert!(permlist_out[0].contains("permissionlist ") && permlist_out[0].contains("permname="));

        let (sglist_out, _) = btea.handle_command("servergrouplist", &mut rt);
        assert!(sglist_out[0].contains("servergrouplist ") && sglist_out[0].contains("name=Server\\sAdmin"));

        let (cglist_out, _) = btea.handle_command("channelgrouplist", &mut rt);
        assert!(cglist_out[0].contains("channelgrouplist ") && cglist_out[0].contains("name=Channel\\sAdmin"));

        let (myperms_out, _) = btea.handle_command("desktopgetmyperms", &mut rt);
        assert!(myperms_out[0].contains("desktopgetmyperms ") && myperms_out[0].contains("permname="));

        // Unauthorized Guest attempt to modify servergroup permissions or create permanent channel must fail with 2568
        let (denied_out, _) = btea.handle_command(
            "servergroupaddperm sgid=8 permsid=b_channel_create_permanent permvalue=1 permskip=0 permnegated=0",
            &mut rt,
        );
        assert!(
            denied_out[0].contains("error id=2568 msg=insufficient\\sclient\\spermissions"),
            "Guest BTEA client must be rejected with 2568: {:?}",
            denied_out
        );

        let (denied_sgadd, _) = btea.handle_command("servergroupadd name=UnauthorizedGroup type=1", &mut rt);
        assert!(
            denied_sgadd[0].contains("error id=2568 msg=insufficient\\sclient\\spermissions"),
            "Guest BTEA client servergroupadd must be rejected with 2568: {:?}",
            denied_sgadd
        );

        // Redeem Server Admin privilege key via tokenuse
        let (tokenuse_out, _) = btea.handle_command(&format!("tokenuse token={}", btea_token), &mut rt);
        let tokenuse_joined = tokenuse_out.join("\n");
        assert!(tokenuse_joined.contains("error id=0 msg=ok"), "BTEA tokenuse failed: {}", tokenuse_joined);
        assert!(
            tokenuse_joined.contains("desktopgetmyperms") && tokenuse_joined.contains("b_virtualserver_servergroup_permission_list"),
            "BTEA tokenuse should push updated desktopgetmyperms: {}",
            tokenuse_joined
        );

        // Now authorized as Server Admin (sgid=6), query and modify permissions & groups
        let (sgpermlist_out, _) = btea.handle_command("servergrouppermlist sgid=8 -permsid", &mut rt);
        assert!(
            sgpermlist_out[0].contains("servergrouppermlist ") && sgpermlist_out[0].contains("permsid="),
            "Authorized BTEA client must be able to query servergrouppermlist -permsid: {:?}",
            sgpermlist_out
        );

        let (addperm_out, _) = btea.handle_command(
            "servergroupaddperm sgid=8 permsid=b_channel_create_temporary permvalue=1 permskip=0 permnegated=0",
            &mut rt,
        );
        assert!(addperm_out[0].contains("error id=0 msg=ok"), "Authorized BTEA servergroupaddperm failed: {:?}", addperm_out);

        let (sgadd_out, _) = btea.handle_command("servergroupadd name=BteaModerator type=1", &mut rt);
        assert!(sgadd_out[0].contains("sgid=") && sgadd_out[0].contains("error id=0 msg=ok"), "Authorized BTEA servergroupadd failed: {:?}", sgadd_out);

        let (clpermlist_out, _) = btea.handle_command("clientpermlist cldbid=1501 -permsid", &mut rt);
        assert!(clpermlist_out[0].contains("error id=0 msg=ok"), "Authorized BTEA clientpermlist on empty client perms should return ok: {:?}", clpermlist_out);

        let (claddperm_out, _) = btea.handle_command(
            "clientaddperm cldbid=1501 permsid=i_client_talk_power permvalue=55 permskip=0 permnegated=0",
            &mut rt,
        );
        assert!(claddperm_out[0].contains("error id=0 msg=ok"), "Authorized BTEA clientaddperm failed: {:?}", claddperm_out);

        let (clpermlist_after, _) = btea.handle_command("clientpermlist cldbid=1501 -permsid", &mut rt);
        assert!(
            clpermlist_after[0].contains("permsid=i_client_talk_power") && clpermlist_after[0].contains("permvalue=55"),
            "Authorized BTEA clientpermlist after clientaddperm failed: {:?}",
            clpermlist_after
        );
    }

    // =========================================================================
    // 2. BlackTeaWeb Client (WebTransport / WebSocket JSON framing)
    // =========================================================================
    let tls_dir = TempDirectory::new(unique_temp_dir("perm-wt-tls"));
    let cert_path = tls_dir.path().join("cert.pem");
    let key_path = tls_dir.path().join("key.pem");
    blackteaspeak_server::web::generate_localhost_tls_assets(&cert_path, &key_path)
        .expect("generate localhost tls assets");

    let cert_hash_bytes: [u8; 32] = {
        let file = std::fs::File::open(&cert_path).unwrap();
        let mut reader = std::io::BufReader::new(file);
        let cert_der = rustls_pemfile::certs(&mut reader).next().unwrap().unwrap();
        <sha2::Sha256 as sha2::Digest>::digest(&cert_der).into()
    };

    let wt_port = {
        let tmp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        tmp.local_addr().unwrap().port()
    };
    let wt_bind_str = format!("127.0.0.1:{}", wt_port);
    let file_transfers = Arc::new(blackteaspeak_server::file_transfer::FileTransferRegistry::new(workspace_root()));
    let web_server = blackteaspeak_server::web::BlackTeaWebTransportServer::bind_with_shared_runtime(
        Arc::clone(&runtime_arc),
        &wt_bind_str,
        &cert_path,
        &key_path,
        file_transfers,
    )
    .expect("bind BlackTeaWebTransportServer");
    std::thread::spawn(move || {
        let _ = web_server.run();
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let wt_client_cfg = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([wtransport::tls::Sha256Digest::new(cert_hash_bytes)])
        .build();
    let wt_endpoint = wtransport::Endpoint::client(wt_client_cfg).expect("wtransport client endpoint");
    let wt_url = format!("https://127.0.0.1:{}/", wt_port);
    let wt_conn = tokio::time::timeout(std::time::Duration::from_secs(3), wt_endpoint.connect(&wt_url))
        .await
        .expect("WebTransport connect timeout")
        .expect("WebTransport connect");

    let (mut wt_send, wt_recv) = wt_conn.open_bi().await.expect("open_bi").await.expect("bi stream ready");
    let mut wt_reader = tokio::io::BufReader::new(wt_recv);

    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"handshakebegin\",\"data\":[{\"intention\":\"0\",\"authentication_method\":\"0\",\"public_key\":\"web-perm-pub\"}]}\n")
        .await
        .unwrap();
    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"handshakeindentityproof\",\"data\":[{\"proof\":\"web-perm-proof\"}]}\n")
        .await
        .unwrap();
    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"clientinit\",\"data\":[{\"client_nickname\":\"WebPermTester\",\"client_version\":\"BlackTeaWeb\",\"client_platform\":\"Web\"}]}\n")
        .await
        .unwrap();

    for _ in 0..25 {
        let mut line = String::new();
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), wt_reader.read_line(&mut line)).await {
            if n == 0 || line.contains("channellistfinished") {
                break;
            }
        } else {
            break;
        }
    }

    // Drain remaining post-init lines
    while let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(50), wt_reader.read_line(&mut String::new())).await {
        if n == 0 {
            break;
        }
    }

    // Guest Web client tries servergroupaddperm -> must fail with code 2568
    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"servergroupaddperm\",\"return_code\":\"web-deny-1\",\"data\":[{\"sgid\":\"8\",\"permsid\":\"b_channel_create_permanent\",\"permvalue\":\"1\",\"permskip\":\"0\",\"permnegated\":\"0\"}]}\n")
        .await
        .unwrap();
    let mut web_deny_resp = String::new();
    for _ in 0..10 {
        let mut line = String::new();
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), wt_reader.read_line(&mut line)).await {
            if n == 0 { break; }
            web_deny_resp.push_str(&line);
            if line.contains("web-deny-1") { break; }
        } else { break; }
    }
    assert!(
        web_deny_resp.contains("\"id\":\"2568\"") || web_deny_resp.contains("insufficient client permissions"),
        "Guest Web client must be rejected with 2568: {}",
        web_deny_resp
    );

    // Web client redeems privilege key via privilegekeyuse
    let web_token_cmd = format!(
        "{{\"type\":\"command\",\"command\":\"privilegekeyuse\",\"return_code\":\"web-tok-1\",\"data\":[{{\"token\":\"{}\"}}]}}\n",
        web_token
    );
    wt_send.write_all(web_token_cmd.as_bytes()).await.unwrap();
    let mut web_tok_resp = String::new();
    for _ in 0..10 {
        let mut line = String::new();
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), wt_reader.read_line(&mut line)).await {
            if n == 0 { break; }
            web_tok_resp.push_str(&line);
            if line.contains("web-tok-1") { break; }
        } else { break; }
    }
    assert!(
        web_tok_resp.contains("\"id\":\"0\"") && web_tok_resp.contains("notifyclientneededpermissions"),
        "Web client privilegekeyuse should succeed and broadcast notifyclientneededpermissions: {}",
        web_tok_resp
    );

    // Authorized Web client queries servergrouppermlist with flags: ["-permsid"] and modifies channel permissions
    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"servergrouppermlist\",\"return_code\":\"web-sgperm-1\",\"flags\":[\"-permsid\"],\"data\":[{\"sgid\":\"8\"}]}\n")
        .await
        .unwrap();
    let mut web_sgperm_resp = String::new();
    for _ in 0..10 {
        let mut line = String::new();
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), wt_reader.read_line(&mut line)).await {
            if n == 0 { break; }
            web_sgperm_resp.push_str(&line);
            if line.contains("web-sgperm-1") { break; }
        } else { break; }
    }
    assert!(
        web_sgperm_resp.contains("notifyservergrouppermlist") && web_sgperm_resp.contains("\"permsid\""),
        "Web client servergrouppermlist with -permsid flag must include permsid: {}",
        web_sgperm_resp
    );

    wt_send
        .write_all(b"{\"type\":\"command\",\"command\":\"channeladdperm\",\"return_code\":\"web-chadd-1\",\"data\":[{\"cid\":\"1\",\"permsid\":\"i_channel_needed_join_power\",\"permvalue\":\"25\"}]}\n")
        .await
        .unwrap();
    let mut web_chadd_resp = String::new();
    for _ in 0..10 {
        let mut line = String::new();
        if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(300), wt_reader.read_line(&mut line)).await {
            if n == 0 { break; }
            web_chadd_resp.push_str(&line);
            if line.contains("web-chadd-1") { break; }
        } else { break; }
    }
    assert!(
        web_chadd_resp.contains("\"id\":\"0\""),
        "Authorized Web client channeladdperm must succeed: {}",
        web_chadd_resp
    );

    // =========================================================================
    // 3. TS3 UDP Client (handle_ts3_command)
    // =========================================================================
    let udp_port = {
        let tmp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        tmp.local_addr().unwrap().port()
    };
    let should_stop = Arc::new(AtomicBool::new(false));
    let shared_secrets = Arc::new(StdMutex::new(std::collections::HashMap::<u64, Vec<u8>>::new()));
    let ts3_server = blackteaspeak_server::teaspeak_transport::TeaSpeakTransportServer::bind_with_shared_runtime(
        1,
        Arc::clone(&runtime_arc),
        &format!("127.0.0.1:{}", udp_port),
    )
    .unwrap()
    .with_shared_secrets(Arc::clone(&shared_secrets));
    let should_stop_udp = Arc::clone(&should_stop);
    std::thread::spawn(move || {
        let _ = ts3_server.run(should_stop_udp);
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let ts3_sock = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let ts3_peer_addr = ts3_sock.local_addr().unwrap();
    let server_udp_sock = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());

    let mut ts3_session = blackteaspeak_server::teaspeak_transport::UdpSession {
        client_id: 303,
        seed_client: vec![0x11; 16],
        seed_server: vec![0x22; 16],
        server_sec: p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng),
        shared_secret: [0x33; 64],
        client_uid: "ts3-perm-uid-303".to_string(),
        server_uid: "server-uid".to_string(),
        server_packet_id: Arc::new(StdMutex::new(1)),
        server_ack_packet_id: 1,
        pending_rtc_describe: false,
        pending_rtc_is_video: false,
        rtc_describe_buffer: Vec::new(),
        ephemeral_sec: None,
        entry2_hash: None,
        client_nickname: "Ts3PermTester".to_string(),
        command_fragment_buffer: Vec::new(),
        command_fragment_active: false,
        is_active: Arc::new(AtomicBool::new(true)),
        is_teaspeak_client: false,
        is_ts6_client: false,
    };
    let iv_struct = blackteaspeak_server::desktop_crypto::derive_iv_struct(
        &ts3_session.shared_secret,
        &ts3_session.seed_client,
        &ts3_session.seed_server,
    );

    // Bootstrap TS3 client in runtime as Guest (sgid=8)
    {
        let mut rt = runtime_arc.lock().unwrap();
        let mut qsession = QuerySessionState {
            client_id: 303,
            connection_ip: "127.0.0.1".to_string(),
            actor_client_database_id_override: Some(1303),
            is_desktop_client: true,
            ..Default::default()
        };
        let req = blackteaspeak_server::query::parse_request_line("clientinit client_nickname=Ts3PermTester").unwrap();
        let _ = blackteaspeak_server::transport::handlers::perform_client_bootstrap(
            &mut rt,
            &mut qsession,
            &req,
            "ts3-perm-uid-303",
            false,
        )
        .unwrap();
    }

    // Helper to collect decrypted TS3 UDP packets sent by handle_ts3_command
    let recv_ts3_packets = |sock: Arc<tokio::net::UdpSocket>, iv: Vec<u8>| async move {
        let mut collected = Vec::new();
        let mut buf = [0u8; 2048];
        let mut frag_buf = Vec::new();
        let mut in_frag = false;
        while let Ok(Ok((n, _))) = tokio::time::timeout(std::time::Duration::from_millis(120), sock.recv_from(&mut buf)).await {
            if n >= 11 {
                let pid = u16::from_be_bytes([buf[8], buf[9]]);
                let flags = buf[10];
                let mut payload_mac = Vec::new();
                payload_mac.extend_from_slice(&buf[0..8]);
                payload_mac.extend_from_slice(&buf[11..n]);
                if let Some(dec) = blackteaspeak_server::desktop_crypto::decrypt_btea_packet(
                    pid,
                    0,
                    flags,
                    &buf[8..11],
                    &payload_mac,
                    &iv,
                    true,
                ) {
                    if (flags & 0x10) != 0 {
                        if !in_frag {
                            in_frag = true;
                            frag_buf.clear();
                            frag_buf.extend_from_slice(&dec);
                        } else {
                            frag_buf.extend_from_slice(&dec);
                            in_frag = false;
                            collected.push(String::from_utf8_lossy(&frag_buf).to_string());
                        }
                    } else if in_frag {
                        frag_buf.extend_from_slice(&dec);
                    } else {
                        collected.push(String::from_utf8_lossy(&dec).to_string());
                    }
                }
            }
        }
        collected.join("\n")
    };

    // Guest TS3 UDP client attempts servergroupaddperm -> must receive error id=2568
    let deny_cmd = "servergroupaddperm sgid=8 permsid=b_channel_create_permanent permvalue=1 permskip=0 permnegated=0 return_code=ts3-deny";
    assert!(
        blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
            deny_cmd,
            deny_cmd.as_bytes(),
            &mut ts3_session,
            ts3_peer_addr,
            &server_udp_sock,
            &iv_struct,
            &runtime_arc,
            1,
            10,
        )
        .await
    );
    let ts3_deny_out = recv_ts3_packets(Arc::clone(&ts3_sock), iv_struct.clone()).await;
    assert!(
        ts3_deny_out.contains("error id=2568") && ts3_deny_out.contains("return_code=ts3-deny"),
        "Guest TS3 UDP client must receive error id=2568 with return_code: {}",
        ts3_deny_out
    );

    // Guest TS3 UDP client redeems privilege key via tokenuse
    let use_cmd = format!("tokenuse token={} return_code=ts3-tok", ts3_token);
    assert!(
        blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
            &use_cmd,
            use_cmd.as_bytes(),
            &mut ts3_session,
            ts3_peer_addr,
            &server_udp_sock,
            &iv_struct,
            &runtime_arc,
            1,
            11,
        )
        .await
    );
    let ts3_tok_out = recv_ts3_packets(Arc::clone(&ts3_sock), iv_struct.clone()).await;
    assert!(
        ts3_tok_out.contains("error id=0 msg=ok") && ts3_tok_out.contains("notifyclientneededpermissions"),
        "TS3 UDP tokenuse must succeed and push notifyclientneededpermissions: {}",
        ts3_tok_out
    );

    // Now authorized as Server Admin (sgid=6), TS3 UDP client queries servergrouppermlist and edits channel
    let sgperm_cmd = "servergrouppermlist sgid=8 -permsid return_code=ts3-sgperm";
    assert!(
        blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
            sgperm_cmd,
            sgperm_cmd.as_bytes(),
            &mut ts3_session,
            ts3_peer_addr,
            &server_udp_sock,
            &iv_struct,
            &runtime_arc,
            1,
            12,
        )
        .await
    );
    let ts3_sgperm_out = recv_ts3_packets(Arc::clone(&ts3_sock), iv_struct.clone()).await;
    assert!(
        ts3_sgperm_out.contains("notifyservergrouppermlist") && ts3_sgperm_out.contains("permsid=") && ts3_sgperm_out.contains("error id=0 msg=ok"),
        "Authorized TS3 UDP client must receive notifyservergrouppermlist with permsid: {}",
        ts3_sgperm_out
    );

    let chedit_cmd = "channeledit cid=1 channel_topic=Updated\\sBy\\sTS3\\sAdmin return_code=ts3-chedit";
    assert!(
        blackteaspeak_server::teaspeak_ts3::handle_ts3_command(
            chedit_cmd,
            chedit_cmd.as_bytes(),
            &mut ts3_session,
            ts3_peer_addr,
            &server_udp_sock,
            &iv_struct,
            &runtime_arc,
            1,
            13,
        )
        .await
    );
    let ts3_chedit_out = recv_ts3_packets(Arc::clone(&ts3_sock), iv_struct.clone()).await;
    assert!(
        ts3_chedit_out.contains("error id=0 msg=ok") && ts3_chedit_out.contains("notifychanneledited"),
        "Authorized TS3 UDP client channeledit must succeed: {}",
        ts3_chedit_out
    );

    should_stop.store(true, Ordering::SeqCst);
    cleanup_state_path(&state_path);
}

#[test]
fn permreset_recovers_locked_out_admin_and_generates_new_privilege_key() {
    let root = workspace_root();
    let state_path = temp_state_path("permreset-recovery");
    let mut runtime = create_baseline_runtime_with_state_path(&root, &state_path)
        .expect("baseline runtime should initialize");

    // 1. Verify startup initial admin token generation and idempotence
    let (initial_token_1, newly_created_1) = runtime.ensure_initial_admin_token(1);
    assert!(newly_created_1, "first call should create an initial admin token");
    assert!(!initial_token_1.is_empty());

    let (initial_token_2, newly_created_2) = runtime.ensure_initial_admin_token(1);
    assert!(!newly_created_2, "second call should reuse existing unused initial admin token");
    assert_eq!(initial_token_1, initial_token_2);

    // 2. Simulate an admin locking themselves out completely:
    // Login as serveradmin, then strip modify power permissions from Admin Server Query (sgid=6)!
    let mut admin_session = QuerySessionState::default();
    assert_eq!(
        runtime.execute("login serveradmin serveradmin", &mut admin_session),
        "error id=0 msg=ok"
    );
    assert_eq!(
        runtime.execute("use sid=1", &mut admin_session),
        "error id=0 msg=ok"
    );
    assert_eq!(
        runtime.execute(
            "servergroupdelperm sgid=6 permsid=b_permission_modify_power_ignore|permsid=i_permission_modify_power|permsid=i_channel_permission_modify_power|permsid=i_client_permission_modify_power|permsid=i_group_modify_power|permsid=i_server_group_modify_power",
            &mut admin_session
        ),
        "error id=0 msg=ok"
    );

    // Confirm serveradmin is now locked out from normal admin actions
    let locked_out_resp = runtime.execute(
        "servergroupaddperm sgid=8 permsid=i_channel_create_modify_with_codec_maxquality permvalue=8 permnegated=0 permskip=0",
        &mut admin_session,
    );
    assert!(
        locked_out_resp.contains("error id=2568 msg=insufficient\\sclient\\spermissions"),
        "locked out admin should fail normal permission changes: {locked_out_resp}"
    );

    // 3. Execute `permreset` as locked-out serveradmin (Admin Console / ServerQuery recovery)
    let reset_resp = runtime.execute("permreset", &mut admin_session);
    assert!(
        reset_resp.contains("error id=0 msg=ok") && reset_resp.contains("token="),
        "permreset must succeed even when serveradmin is locked out: {reset_resp}"
    );
    let recovery_token = extract_field(&reset_resp, "token")
        .expect("permreset response must include new privilege key token");

    // 4. Verify serveradmin is back in sgid=6 and can modify permissions again
    let restored_resp = runtime.execute(
        "servergroupaddperm sgid=8 permsid=i_channel_create_modify_with_codec_maxquality permvalue=8 permnegated=0 permskip=0",
        &mut admin_session,
    );
    assert_eq!(restored_resp, "error id=0 msg=ok");

    // 5. Verify a normal online client (clid=10, cldbid=40) can redeem the new recovery token from permreset
    let mut client_session = QuerySessionState {
        client_id: 10,
        selected_virtual_server_id: Some(1),
        current_channel_id: Some(1),
        actor_client_database_id_override: Some(40),
        is_desktop_client: true,
        ..QuerySessionState::default()
    };
    let use_resp = runtime.execute(
        &format!("tokenuse token={recovery_token}"),
        &mut client_session,
    );
    assert_eq!(use_resp, "error id=0 msg=ok");
    let groups_after = runtime.execute("servergroupsbyclientid cldbid=40", &mut admin_session);
    assert!(
        groups_after.contains("sgid=9"),
        "redeeming permreset token should grant Server Admin (sgid=9): {groups_after}"
    );

    cleanup_state_path(&state_path);
}

