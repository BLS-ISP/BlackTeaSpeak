use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::UdpSocket;


use crate::runtime::BaselineRuntime;
use std::collections::HashMap;
use crate::desktop_proto::ObservedDesktopPacket;

pub(crate) fn decompress_if_needed(data: &[u8]) -> Vec<u8> {
    if data.len() < 3 {
        return data.to_vec();
    }
    let first = data[0];
    let is_compressed = (first & 1) == 1;
    let level = (first >> 2) & 0x0F;

    if is_compressed && level == 1 {
        // Read decompressed size from header
        let n = if (first & 2) == 2 { 4 } else { 1 };
        if data.len() < 1 + n + n {
            return data.to_vec();
        }
        let offset = 1 + n;
        let mut original_size: u32 = 0;
        for i in 0..n {
            original_size |= (data[offset + i] as u32) << (i * 8);
        }
        
        let mut cursor = data;
        match quicklz::decompress(&mut cursor, original_size) {
            Ok(decompressed) => {
                println!("QuickLZ Decompression success! Size {} -> {}", data.len(), decompressed.len());
                decompressed
            }
            Err(e) => {
                eprintln!("QuickLZ Decompression failed: {:?}", e);
                data.to_vec()
            }
        }
    } else {
        if (first & 1) == 0 && level == 1 {
            let n = if (first & 2) == 2 { 4 } else { 1 };
            let header_len = 2 * n + 1;
            if data.len() > header_len {
                println!("QuickLZ Uncompressed payload: stripping {} bytes header", header_len);
                return data[header_len..].to_vec();
            }
        }
        data.to_vec()
    }
}

const TS3INIT1_MAGIC: &[u8; 8] = b"TS3INIT1";
const TS3INIT_PACKET_ID: u16 = 101;

const TS3INIT_FLAGS: u8 = 0x88;
const TS3INIT_SET_COOKIE_PACKET_LENGTH: usize = 32;
const TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH: usize = 244;

static NEXT_TS3_CLIENT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(101);

pub struct UdpSession {
    pub client_id: u64,
    pub seed_client: Vec<u8>,
    pub seed_server: Vec<u8>,
    pub server_sec: p256::SecretKey,
    pub shared_secret: [u8; 64],
    pub client_uid: String,
    pub server_uid: String,
    pub server_packet_id: Arc<Mutex<u16>>,
    pub server_ack_packet_id: u16,
    pub pending_rtc_describe: bool,
    pub pending_rtc_is_video: bool,
    pub rtc_describe_buffer: Vec<u8>,
    pub ephemeral_sec: Option<[u8; 32]>,
    pub entry2_hash: Option<[u8; 64]>,
    // Client nickname cached from clientinit used to respond to clientgetvariables queries.
    pub client_nickname: String,
    // Buffer used to accumulate incoming fragmented command packets (marked with flag 0x10).
    pub command_fragment_buffer: Vec<u8>,
    // Indicator whether a fragmented command sequence is actively being received.
    pub command_fragment_active: bool,
    // Cancellation token to prevent old background tasks from sending commands on connection reset
    pub is_active: Arc<std::sync::atomic::AtomicBool>,
    // Distinguishes TeaSpeak clients from official TeamSpeak 3 clients
    pub is_teaspeak_client: bool,
    // Distinguishes TeamSpeak 6 clients (with native TS6 STREAM_P2P video/desktop streaming chain)
    pub is_ts6_client: bool,
}

pub struct TeaSpeakTransportServer {
    pub server_id: u32,
    pub bind_addr: String,
    pub runtime: Arc<Mutex<BaselineRuntime>>,
    pub rtc_manager: Arc<crate::rtc_manager::RtcBroadcastManager>,
    pub shared_secrets: Option<Arc<Mutex<HashMap<u64, Vec<u8>>>>>,
}

impl TeaSpeakTransportServer {
    pub fn bind_with_shared_runtime(
        server_id: u32,
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
    ) -> Result<Self> {
        let rtc_manager = Arc::clone(&runtime.lock().unwrap().rtc_manager);
        Ok(Self {
            server_id,
            bind_addr: bind_addr.to_string(),
            runtime,
            rtc_manager,
            shared_secrets: None,
        })
    }

    pub fn with_shared_secrets(
        mut self,
        shared_secrets: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    ) -> Self {
        self.shared_secrets = Some(shared_secrets);
        self
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.bind_addr.parse().context("invalid bind address")
    }

    pub fn run(self, should_stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("failed to build tokio runtime for teaspeak transport")?;

        let (notif_tx, mut notif_rx) = tokio::sync::mpsc::unbounded_channel::<(u32, crate::transport::TransportNotification)>();
        let mut media_rx = {
            let rt_lock = self.runtime.lock().unwrap();
            rt_lock.subscribe_events(Box::new(move |_rt, sid, notif| {
                let _ = notif_tx.send((sid, notif.clone()));
            }));
            rt_lock.desktop_btea_media_tx.as_ref().unwrap().subscribe()
        };

        rt.block_on(async move {
            let socket = UdpSocket::bind(&self.bind_addr)
                .await
                .context("failed to bind teaspeak udp socket")?;
            
            let socket = Arc::new(socket);
            let mut buf = vec![0u8; 4096];
            let mut sessions: HashMap<SocketAddr, UdpSession> = HashMap::new();
            let mut btea_udp_sessions: HashMap<u64, (SocketAddr, u16)> = HashMap::new();
            let mut btea_addr_to_cid: HashMap<SocketAddr, u64> = HashMap::new();
            let mut voice_out_packet_id: u16 = 0;
            let mut music_status_interval = tokio::time::interval(Duration::from_secs(1));
            let mut pending_song_info_sync: HashMap<u64, (std::time::Instant, u8)> = HashMap::new();

            loop {
                if should_stop.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
                    Some((notif_server_id, notif)) = notif_rx.recv() => {
                        if notif_server_id == self.server_id {
                            if matches!(&notif, crate::transport::TransportNotification::TalkStatus { .. }) {
                                // TS3/TeaSpeak UDP sessions use voice packets (type 0x00/0x01) for speaking status, not text commands
                                continue;
                            }
                            let mut has_client_field_changes = true;
                            let mut music_song_changed = false;
                            let mut music_sync_changed = false;
                            let mut music_loaded_changed = false;
                            let mut prev_loaded_songs = String::new();
                            let mut music_payload = None;
                            if let crate::transport::TransportNotification::ClientUpdated { before, after, .. } = &notif {
                                has_client_field_changes = crate::transport::server::client_update_fields(before, after).len() > 1;
                                if after.client_type_exact == 4 || after.unique_identifier.starts_with("compat-musicbot-") {
                                    music_song_changed = before.extra_properties.get("_music_song_state") != after.extra_properties.get("_music_song_state");
                                    music_sync_changed = before.extra_properties.get("_music_sync_state") != after.extra_properties.get("_music_sync_state")
                                        || before.extra_properties.get("player_volume") != after.extra_properties.get("player_volume");
                                    music_loaded_changed = before.extra_properties.get("_music_loaded_songs") != after.extra_properties.get("_music_loaded_songs");
                                    if music_loaded_changed {
                                        prev_loaded_songs = before
                                            .extra_properties
                                            .get("_music_loaded_songs")
                                            .cloned()
                                            .unwrap_or_default();
                                    }
                                    if music_song_changed || music_sync_changed || music_loaded_changed {
                                        music_payload = self
                                            .runtime
                                            .lock()
                                            .unwrap()
                                            .music_bot_notify_payload_by_identifier(self.server_id, after.id);
                                    }
                                }
                                if !has_client_field_changes && !music_song_changed && !music_sync_changed && !music_loaded_changed {
                                    continue;
                                }
                            }
                            let ts6_bridge_cmds = if let crate::transport::TransportNotification::VideoBroadcastChanged { server_id: b_sid, .. } = &notif {
                                crate::protocols::ts6::stream_manager::sync_non_ts6_broadcasts_to_ts6(&self.runtime, *b_sid)
                            } else {
                                Vec::new()
                            };
                            for (peer_addr, peer_session) in sessions.iter() {
                                if peer_session.client_nickname.is_empty() || !peer_session.is_active.load(Ordering::SeqCst) {
                                    continue;
                                }
                                let is_self_already_sent = match &notif {
                                    crate::transport::TransportNotification::ClientEnterView { presence, from_channel_id: None, .. } => presence.client_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ClientMoved { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::TextMessage { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ChannelCreated { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ChannelDeleted { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ChannelEdited { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ChannelMoved { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ServerEdited { invoker_id, .. } => *invoker_id == peer_session.client_id,
                                    crate::transport::TransportNotification::ClientUpdated { .. } => false,
                                    crate::transport::TransportNotification::ClientLeftView { presence, invoker_id, .. } => {
                                        (presence.client_id == peer_session.client_id && *invoker_id == 0)
                                            || (*invoker_id == peer_session.client_id && presence.client_id != peer_session.client_id)
                                    }
                                    _ => false,
                                };
                                if is_self_already_sent {
                                    continue;
                                }
                                match &notif {
                                    crate::transport::TransportNotification::Ts6StreamNotify {
                                        target_client_id,
                                        exclude_client_id,
                                        ..
                                    } => {
                                        if !peer_session.is_ts6_client {
                                            continue;
                                        }
                                        if *exclude_client_id == Some(peer_session.client_id) {
                                            continue;
                                        }
                                        if let Some(tid) = target_client_id
                                            && *tid != peer_session.client_id
                                        {
                                            continue;
                                        }
                                    }
                                    crate::transport::TransportNotification::ClientPoke { target_client_id, .. } => {
                                        if *target_client_id != peer_session.client_id {
                                            continue;
                                        }
                                    }
                                    crate::transport::TransportNotification::TextMessage { target, .. } => {
                                        if target.target_mode == 1 && target.target_client_id != Some(peer_session.client_id) {
                                            continue;
                                        }
                                        if target.target_mode == 2 {
                                            if let Some(target_cid) = target.channel_id {
                                                let peer_cid = {
                                                    let rt_lock = self.runtime.lock().unwrap();
                                                    rt_lock.online_client_snapshot(self.server_id, peer_session.client_id).map(|c| c.channel_id)
                                                };
                                                if let Some(pcid) = peer_cid {
                                                    if pcid != target_cid {
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                let cmd_str = match &notif {
                                    crate::transport::TransportNotification::ClientEnterView { presence, from_channel_id, reason_id } => {
                                        let is_music_bot = presence.client_type == 4
                                            || presence.unique_identifier.starts_with("compat-musicbot-");
                                        let wire_client_type = if is_music_bot { 0 } else { presence.client_type };
                                        let (
                                            real_dbid,
                                            real_sg_str,
                                            real_cg_id,
                                            real_talk_power,
                                            input_muted,
                                            output_muted,
                                            outputonly_muted,
                                            input_hardware,
                                            output_hardware,
                                            is_recording,
                                            away,
                                            away_msg,
                                            flag_avatar,
                                            talk_request,
                                            talk_request_msg,
                                            description,
                                            is_talker,
                                            is_channel_commander,
                                            country,
                                            badges,
                                        ) = {
                                            let rt_lock = self.runtime.lock().unwrap();
                                            let online_opt = rt_lock.store.online_clients.get(&presence.client_id);
                                            let (dbid, sg_str) = online_opt
                                                .map(|c| {
                                                    let sg = if c.server_groups.is_empty() {
                                                        String::from("8")
                                                    } else {
                                                        c.server_groups
                                                            .iter()
                                                            .map(u32::to_string)
                                                            .collect::<Vec<_>>()
                                                            .join(",")
                                                    };
                                                    (c.database_id, sg)
                                                })
                                                .unwrap_or_else(|| (presence.client_id + 1000, String::from("8")));
                                            let cg_id = rt_lock.effective_channel_group_id_for_client(
                                                self.server_id,
                                                presence.channel_id,
                                                dbid,
                                            );
                                            let tp = rt_lock.client_talk_power(
                                                self.server_id,
                                                presence.channel_id,
                                                dbid,
                                            );
                                            let in_m = online_opt.map(|c| i32::from(c.input_muted)).unwrap_or(0);
                                            let out_m = online_opt.map(|c| i32::from(c.output_muted)).unwrap_or(0);
                                            let outonly_m = online_opt
                                                .and_then(|c| c.extra_properties.get("client_outputonly_muted"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("0"));
                                            let in_hw = online_opt
                                                .and_then(|c| c.extra_properties.get("client_input_hardware"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("1"));
                                            let out_hw = online_opt
                                                .and_then(|c| c.extra_properties.get("client_output_hardware"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("1"));
                                            let rec = online_opt
                                                .and_then(|c| c.extra_properties.get("client_is_recording"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("0"));
                                            let aw = online_opt.map(|c| i32::from(c.away)).unwrap_or(0);
                                            let aw_msg = online_opt.map(|c| c.away_message.clone()).unwrap_or_default();
                                            let av = online_opt
                                                .and_then(|c| c.extra_properties.get("client_flag_avatar"))
                                                .cloned()
                                                .unwrap_or_default();
                                            let tr = online_opt
                                                .and_then(|c| c.extra_properties.get("client_talk_request"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("0"));
                                            let tr_msg = online_opt
                                                .and_then(|c| c.extra_properties.get("client_talk_request_msg"))
                                                .cloned()
                                                .unwrap_or_default();
                                            let desc = online_opt
                                                .and_then(|c| c.extra_properties.get("client_description"))
                                                .cloned()
                                                .unwrap_or_default();
                                            let talker = online_opt
                                                .and_then(|c| c.extra_properties.get("client_is_talker"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("0"));
                                            let cc = online_opt
                                                .and_then(|c| c.extra_properties.get("client_is_channel_commander"))
                                                .cloned()
                                                .unwrap_or_else(|| String::from("0"));
                                            let cntry = online_opt
                                                .map(|c| if c.country.is_empty() { String::from("DE") } else { c.country.clone() })
                                                .unwrap_or_else(|| String::from("DE"));
                                            let bdg = online_opt
                                                .and_then(|c| c.extra_properties.get("client_badges"))
                                                .cloned()
                                                .unwrap_or_default();
                                            (
                                                dbid, sg_str, cg_id, tp, in_m, out_m, outonly_m, in_hw, out_hw,
                                                rec, aw, aw_msg, av, tr, tr_msg, desc, talker, cc, cntry, bdg,
                                            )
                                        };
                                        let extra_tail = if peer_session.is_teaspeak_client {
                                            if is_music_bot {
                                                let (p_state, p_vol, p_plid, c_dis, c_notify, c_btype, c_uptime) = {
                                                    let rt = self.runtime.lock().unwrap();
                                                    let online_opt = rt.store.online_clients.get(&presence.client_id);
                                                    let p_state = online_opt
                                                        .and_then(|c| c.extra_properties.get("player_state"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("4"));
                                                    let p_vol = online_opt
                                                        .and_then(|c| c.extra_properties.get("player_volume"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("1"));
                                                    let p_plid = online_opt
                                                        .and_then(|c| c.extra_properties.get("client_playlist_id"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("0"));
                                                    let c_dis = online_opt
                                                        .and_then(|c| c.extra_properties.get("client_disabled"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("0"));
                                                    let c_notify = online_opt
                                                        .and_then(|c| c.extra_properties.get("client_flag_notify_song_change"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("0"));
                                                    let c_btype = online_opt
                                                        .and_then(|c| c.extra_properties.get("client_bot_type"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("0"));
                                                    let c_uptime = online_opt
                                                        .and_then(|c| c.extra_properties.get("client_uptime_mode"))
                                                        .cloned()
                                                        .unwrap_or_else(|| String::from("0"));
                                                    (p_state, p_vol, p_plid, c_dis, c_notify, c_btype, c_uptime)
                                                };
                                                format!(
                                                    " client_version=3.6.2 client_platform=Windows client_type_exact=4 player_state={} player_volume={} client_playlist_id={} client_disabled={} client_flag_notify_song_change={} client_bot_type={} client_uptime_mode={}",
                                                    crate::teaspeak_ts3::ts3_escape(&p_state),
                                                    crate::teaspeak_ts3::ts3_escape(&p_vol),
                                                    crate::teaspeak_ts3::ts3_escape(&p_plid),
                                                    crate::teaspeak_ts3::ts3_escape(&c_dis),
                                                    crate::teaspeak_ts3::ts3_escape(&c_notify),
                                                    crate::teaspeak_ts3::ts3_escape(&c_btype),
                                                    crate::teaspeak_ts3::ts3_escape(&c_uptime),
                                                )
                                            } else {
                                                String::from(" client_version=3.6.2 client_platform=Windows")
                                            }
                                        } else if peer_session.is_ts6_client {
                                            let is_str = if crate::protocols::ts6::stream_manager::is_client_streaming_ts6(self.server_id, presence.client_id) {
                                                "1"
                                            } else {
                                                "0"
                                            };
                                            format!(" client_is_streaming={}", is_str)
                                        } else {
                                            String::new()
                                        };
                                        format!(
                                            "notifycliententerview cfid={} ctid={} reasonid={} clid={} client_unique_identifier={} client_nickname={} client_input_muted={} client_output_muted={} client_outputonly_muted={} client_input_hardware={} client_output_hardware={} client_meta_data= client_is_recording={} client_database_id={} client_channel_group_id={} client_servergroups={} client_totalconnections=1 client_away={} client_away_message={} client_type={} client_flag_avatar={} client_talk_power={} client_talk_request={} client_talk_request_msg={} client_description={} client_is_talker={} client_is_priority_speaker=0 client_unread_messages=0 client_nickname_phonetic= client_needed_serverquery_view_power=0 client_icon_id=0 client_is_channel_commander={} client_country={} client_channel_group_inherited_channel_id={} client_badges={} client_myteamspeak_id= client_integrations={}",
                                            from_channel_id.unwrap_or(0),
                                            presence.channel_id,
                                            reason_id,
                                            presence.client_id,
                                            crate::teaspeak_ts3::ts3_escape(&presence.unique_identifier),
                                            crate::teaspeak_ts3::ts3_escape(&presence.login_name),
                                            input_muted,
                                            output_muted,
                                            crate::teaspeak_ts3::ts3_escape(&outputonly_muted),
                                            crate::teaspeak_ts3::ts3_escape(&input_hardware),
                                            crate::teaspeak_ts3::ts3_escape(&output_hardware),
                                            crate::teaspeak_ts3::ts3_escape(&is_recording),
                                            real_dbid,
                                            real_cg_id,
                                            real_sg_str,
                                            away,
                                            crate::teaspeak_ts3::ts3_escape(&away_msg),
                                            wire_client_type,
                                            crate::teaspeak_ts3::ts3_escape(&flag_avatar),
                                            real_talk_power,
                                            crate::teaspeak_ts3::ts3_escape(&talk_request),
                                            crate::teaspeak_ts3::ts3_escape(&talk_request_msg),
                                            crate::teaspeak_ts3::ts3_escape(&description),
                                            crate::teaspeak_ts3::ts3_escape(&is_talker),
                                            crate::teaspeak_ts3::ts3_escape(&is_channel_commander),
                                            crate::teaspeak_ts3::ts3_escape(&country),
                                            presence.channel_id,
                                            crate::teaspeak_ts3::ts3_escape(&badges),
                                            extra_tail,
                                        )
                                    }
                                    crate::transport::TransportNotification::VideoBroadcastChanged { server_id: b_sid, channel_id } => {
                                        if peer_session.is_ts6_client {
                                            if !ts6_bridge_cmds.is_empty() {
                                                let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                                    &peer_session.shared_secret,
                                                    &peer_session.seed_client,
                                                    &peer_session.seed_server,
                                                );
                                                for ts6_cmd in &ts6_bridge_cmds {
                                                    crate::protocols::ts6::wire_formatting::send_ts6_command(
                                                        ts6_cmd,
                                                        peer_session,
                                                        *peer_addr,
                                                        &socket,
                                                        &iv_struct,
                                                    )
                                                    .await;
                                                }
                                            }
                                            continue;
                                        }
                                        if !peer_session.is_teaspeak_client {
                                            continue;
                                        }
                                        let rt_lock = self.runtime.lock().unwrap();
                                        let parts: Vec<String> = rt_lock
                                            .store
                                            .active_broadcasts
                                            .values()
                                            .filter(|b| {
                                                b.server_id == *b_sid
                                                    && b.channel_id == *channel_id
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
                                        if parts.is_empty() {
                                            format!("notifybroadcastvideo cid={}", channel_id)
                                        } else {
                                            format!("notifybroadcastvideo {}", parts.join("|"))
                                        }
                                    }
                                    _ => crate::transport::render_notification(&notif),
                                };
                                let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                    &peer_session.shared_secret,
                                    &peer_session.seed_client,
                                    &peer_session.seed_server,
                                );
                                if !peer_session.is_teaspeak_client {
                                    if !matches!(&notif, crate::transport::TransportNotification::ClientUpdated { .. }) || has_client_field_changes {
                                        if peer_session.is_ts6_client {
                                            crate::protocols::ts6::wire_formatting::send_ts6_command(&cmd_str, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        } else {
                                            crate::teaspeak_ts3::send_command(&cmd_str, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        }
                                    }
                                } else if let Some(payload) = &music_payload {
                                    if music_loaded_changed && payload.playlist_id > 0 {
                                        for loaded_row in &payload.loaded_songs {
                                            if let (Some(sid_str), Some(meta_str)) = (loaded_row.get("song_id"), loaded_row.get("song_metadata")) {
                                                let summary_entry = format!("{sid_str}={meta_str}");
                                                if !prev_loaded_songs.lines().any(|line| line == summary_entry) {
                                                    let loaded_cmd = format!(
                                                        "notifyplaylistsongloaded {}",
                                                        crate::protocols::teaspeak::wire_formatting::render_teaspeak_rows_only(std::slice::from_ref(loaded_row))
                                                    );
                                                    crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(
                                                        &loaded_cmd,
                                                        peer_session,
                                                        *peer_addr,
                                                        &socket,
                                                        &iv_struct,
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                    if music_song_changed {
                                        let song_cmd = format!(
                                            "notifymusicplayersongchange {}",
                                            crate::protocols::teaspeak::wire_formatting::render_teaspeak_rows_only(std::slice::from_ref(&payload.song_change))
                                        );
                                        let p_state = payload
                                            .client_updates
                                            .get("player_state")
                                            .map(String::as_str)
                                            .unwrap_or("4");
                                        let p_vol = payload
                                            .client_updates
                                            .get("player_volume")
                                            .map(String::as_str)
                                            .unwrap_or("1");
                                        let p_plid = payload
                                            .client_updates
                                            .get("client_playlist_id")
                                            .map(String::as_str)
                                            .unwrap_or("0");
                                        let client_upd_cmd = format!(
                                            "notifyclientupdated clid={} player_state={} player_volume={} client_playlist_id={}",
                                            payload.client_id,
                                            crate::protocols::teaspeak::wire_formatting::teaspeak_escape(p_state),
                                            crate::protocols::teaspeak::wire_formatting::teaspeak_escape(p_vol),
                                            crate::protocols::teaspeak::wire_formatting::teaspeak_escape(p_plid),
                                        );
                                        let status_cmd = format!(
                                            "notifymusicstatusupdate {}",
                                            crate::protocols::teaspeak::wire_formatting::render_teaspeak_rows_only(std::slice::from_ref(&payload.status_update))
                                        );
                                        crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(&song_cmd, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(&client_upd_cmd, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(&status_cmd, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        if p_state == "2" {
                                            pending_song_info_sync.insert(payload.client_id, (std::time::Instant::now(), 0));
                                        } else {
                                            pending_song_info_sync.remove(&payload.client_id);
                                        }
                                    } else {
                                        if !matches!(&notif, crate::transport::TransportNotification::ClientUpdated { .. }) || has_client_field_changes {
                                            crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(&cmd_str, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        }
                                        if music_sync_changed {
                                            let status_cmd = format!(
                                                "notifymusicstatusupdate {}",
                                                crate::protocols::teaspeak::wire_formatting::render_teaspeak_rows_only(std::slice::from_ref(&payload.status_update))
                                            );
                                            crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(&status_cmd, peer_session, *peer_addr, &socket, &iv_struct).await;
                                        }
                                    }
                                } else if !matches!(&notif, crate::transport::TransportNotification::ClientUpdated { .. }) || has_client_field_changes {
                                    crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(&cmd_str, peer_session, *peer_addr, &socket, &iv_struct).await;
                                }
                                if let crate::transport::TransportNotification::ClientLeftView { presence, to_channel_id: None, .. } = &notif {
                                    if presence.client_id == peer_session.client_id {
                                        peer_session.is_active.store(false, Ordering::SeqCst);
                                    }
                                }
                            }
                            sessions.retain(|_, s| s.is_active.load(Ordering::SeqCst));
                        }
                    }
                    Ok((media_server_id, sender_client_id, packet_type, raw_opus)) = media_rx.recv() => {
                        if media_server_id == self.server_id {
                            let (sender_channel_id, can_speak) = {
                                let rt_lock = self.runtime.lock().unwrap();
                                let cid = rt_lock.online_client_snapshot(self.server_id, sender_client_id).map(|c| c.channel_id).unwrap_or(0);
                                let can_speak = rt_lock.client_can_speak_in_channel(self.server_id, sender_client_id);
                                (cid, can_speak)
                            };
                            if !can_speak {
                                continue;
                            }
                            for (peer_addr, peer_session) in sessions.iter() {
                                if peer_session.client_id == sender_client_id || peer_session.client_nickname.is_empty() || !peer_session.is_active.load(Ordering::SeqCst) {
                                    continue;
                                }
                                let peer_cid = {
                                    let rt_lock = self.runtime.lock().unwrap();
                                    rt_lock.online_client_snapshot(self.server_id, peer_session.client_id).map(|c| c.channel_id).unwrap_or(0)
                                };
                                if sender_channel_id != 0 && peer_cid == sender_channel_id {
                                    let v_pkt_id = voice_out_packet_id;
                                    voice_out_packet_id = voice_out_packet_id.wrapping_add(1);
                                    let mut ts3_voice_payload = Vec::with_capacity(5 + raw_opus.len());
                                    ts3_voice_payload.extend_from_slice(&v_pkt_id.to_be_bytes());
                                    ts3_voice_payload.extend_from_slice(&(sender_client_id as u16).to_be_bytes());
                                    ts3_voice_payload.push(4);
                                    ts3_voice_payload.extend_from_slice(&raw_opus);

                                    let mut out_header = [0u8; 3];
                                    out_header[0..2].copy_from_slice(&v_pkt_id.to_be_bytes());
                                    out_header[2] = 0xA0; // Unencrypted | NewProtocol | Voice

                                    let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                        &peer_session.shared_secret,
                                        &peer_session.seed_client,
                                        &peer_session.seed_server,
                                    );
                                    let mac = if peer_session.shared_secret.iter().any(|&b| b != 0) {
                                        use sha1::{Sha1, Digest as _};
                                        let hash = Sha1::digest(&iv_struct);
                                        let mut m = [0u8; 8];
                                        m.copy_from_slice(&hash[0..8]);
                                        m
                                    } else {
                                        [0u8; 8]
                                    };
                                    let mut final_packet = Vec::with_capacity(11 + ts3_voice_payload.len());
                                    final_packet.extend_from_slice(&mac);
                                    final_packet.extend_from_slice(&out_header);
                                    final_packet.extend_from_slice(&ts3_voice_payload);
                                    let _ = socket.send_to(&final_packet, *peer_addr).await;
                                }
                            }
                            if !raw_opus.is_empty() {
                                if let Some(secrets_arc) = &self.shared_secrets {
                                for (peer_cid, (peer_addr, next_pkt_id)) in btea_udp_sessions.iter_mut() {
                                    if *peer_cid == sender_client_id {
                                        continue;
                                    }
                                    let peer_chan = {
                                        let rt_lock = self.runtime.lock().unwrap();
                                        rt_lock.online_client_snapshot(self.server_id, *peer_cid).map(|c| c.channel_id).unwrap_or(0)
                                    };
                                    if sender_channel_id != 0 && peer_chan == sender_channel_id {
                                        let secret_opt = secrets_arc.lock().unwrap().get(peer_cid).cloned();
                                        if let Some(secret) = secret_opt {
                                            let out_pkt_id = *next_pkt_id;
                                            *next_pkt_id = next_pkt_id.wrapping_add(1);
                                            let out_flags = if packet_type == 0x0B { 0x0B } else { 0x0A };
                                            let mut out_header = [0u8; 5];
                                            out_header[0..2].copy_from_slice(&out_pkt_id.to_be_bytes());
                                            out_header[2..4].copy_from_slice(&(sender_client_id as u16).to_be_bytes());
                                            out_header[4] = out_flags;
                                            let encrypted_out = crate::desktop_crypto::encrypt_btea_packet(
                                                out_pkt_id, 0, out_flags, &out_header, &raw_opus, &secret, true
                                            );
                                            let mut final_packet = Vec::with_capacity(13 + encrypted_out.len() - 8);
                                            final_packet.extend_from_slice(&encrypted_out[0..8]);
                                            final_packet.extend_from_slice(&out_header);
                                            final_packet.extend_from_slice(&encrypted_out[8..]);
                                            let _ = socket.send_to(&final_packet, *peer_addr).await;
                                        }
                                    }
                                    }
                                }
                            }
                        }
                    }
                    result = socket.recv_from(&mut buf) => {
                        if let Ok((size, addr)) = result {
                            let packet = &buf[..size];
                            
                            if let Some(parsed) = ObservedDesktopPacket::parse(packet) {
                                match parsed {
                                    ObservedDesktopPacket::Ts3InitGetCookie { random_sequence } => {
                                        println!("teaspeak udp: received GetCookie from {} (seq: {})", addr, random_sequence);
                                        println!("teaspeak udp: RAW PACKET = {:02X?}", packet);
                                        if let Some(old_session) = sessions.get(&addr) {
                                            old_session.is_active.store(false, Ordering::SeqCst);
                                            println!("Deactivating old session for {} on GetCookie to cancel background tasks", addr);
                                        }
                                        // Send SetCookie
                                        let mut out = [0_u8; TS3INIT_SET_COOKIE_PACKET_LENGTH];
                                        out[..8].copy_from_slice(TS3INIT1_MAGIC);
                                        out[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
                                        out[10] = TS3INIT_FLAGS;
                                        out[11] = 0x01;
                                        
                                        let cookie: u64 = 0x1122334455667788; // Dummy cookie
                                        let packet_index = 0;
                                        out[12..20].copy_from_slice(&cookie.to_le_bytes());
                                        out[20] = packet_index;
                                        out[21..28].fill(0);
                                        out[28..32].copy_from_slice(&random_sequence.to_le_bytes());
                                        
                                        let _ = socket.send_to(&out, addr).await;
                                    }
                                    ObservedDesktopPacket::Ts3InitGetPuzzle { cookie, packet_index: _ } => {
                                        println!("teaspeak udp: received GetPuzzle from {} (cookie: {})", addr, cookie);
                                        // Send SetPuzzle
                                        let mut out = [0_u8; TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH];
                                        out[..8].copy_from_slice(TS3INIT1_MAGIC);
                                        out[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
                                        out[10] = TS3INIT_FLAGS;
                                        out[11] = 0x03;

                                        // Observed on-wire success path from a real BlackTeaSpeak server.
                                        out[75] = 0x01;
                                        out[139] = 0x01;
                                        out[142] = 0x03;
                                        out[143] = 0xE8;
                                        
                                        let _ = socket.send_to(&out, addr).await;
                                    }
                                    ObservedDesktopPacket::Ts3InitSolvePuzzle { payload } => {
                                        let payload_str = String::from_utf8_lossy(&payload);
                                        println!("teaspeak udp: received SolvePuzzle from {}", addr);
                                        println!("teaspeak udp: SolvePuzzle payload: {}", payload_str);
                                        
                                        let is_teaspeak = payload_str.contains("teaspeak");
                                        
                                        if is_teaspeak {
                                            println!("Handling TeaSpeak TCP handshake (initivexpand)...");
                                            
                                            // Extract alpha and omega
                                            let mut alpha_val = "";
                                            let mut client_omega_val = "";
                                            for part in payload_str.split_whitespace() {
                                                if part.starts_with("alpha=") {
                                                    alpha_val = &part[6..];
                                                } else if part.starts_with("omega=") {
                                                    client_omega_val = &part[6..];
                                                }
                                            }
                                            
                                            let ts3_unescape = |s: &str| -> String {
                                                s.replace("\\s", " ").replace("\\p", "|").replace("\\a", "\x07").replace("\\b", "\x08").replace("\\f", "\x0c").replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t").replace("\\v", "\x0b").replace("\\/", "/").replace("\\\\", "\\")
                                            };
                                            let alpha_unescaped = ts3_unescape(alpha_val);
                                            let client_omega_unescaped = ts3_unescape(client_omega_val);
                                            
                                            let mut beta_bytes = [0u8; 10];
                                            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut beta_bytes);
                                            use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
                                            
                                            let (server_sec, server_pub) = crate::desktop_crypto::load_or_create_server_keypair();
                                            let omega_bytes = crate::desktop_crypto::export_public_key_libtomcrypt_asn1(&server_pub);
                                            
                                            // Calculate shared secret using client's omega
                                            let client_omega_bytes = BASE64_STANDARD.decode(&client_omega_unescaped).unwrap_or_default();
                                            let mut shared_secret_array = [0u8; 64];
                                            if let Some(client_sec1) = crate::desktop_crypto::import_public_key_libtomcrypt_asn1(&client_omega_bytes) {
                                                println!("Successfully parsed client omega to sec1! len={}", client_sec1.len());
                                                if let Some(shared_sec_20) = crate::desktop_crypto::calculate_shared_secret(&client_sec1, &server_sec) {
                                                    println!("Successfully calculated shared secret! shared_sec={:?}", shared_sec_20);
                                                    shared_secret_array[0..20].copy_from_slice(&shared_sec_20);
                                                } else {
                                                    println!("Failed to calculate shared secret!");
                                                }
                                            } else {
                                                println!("Failed to parse client omega! Bytes: {:02X?}", client_omega_bytes);
                                            }
                                            
                                            let ts3_escape = |s: &str| -> String {
                                                s.replace("\\", "\\\\").replace(" ", "\\s").replace("/", "\\/").replace("|", "\\p").replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t").replace("\x0b", "\\v")
                                            };
                                            
                                            let beta_b64 = ts3_escape(&BASE64_STANDARD.encode(&beta_bytes));
                                            let omega_b64 = ts3_escape(&BASE64_STANDARD.encode(&omega_bytes));
                                            
                                            let initivexpand_cmd = format!("initivexpand alpha={} beta={} omega={} teaspeak=1", alpha_val, beta_b64, omega_b64);
                                            
                                            let mut header = [0u8; 3];
                                            header[0..2].copy_from_slice(&0u16.to_be_bytes());
                                            header[2] = 0x82; // Unencrypted Command
                                            
                                            let mut encrypted_payload = Vec::new();
                                            encrypted_payload.extend_from_slice(&[0u8; 8]); // Dummy MAC
                                            encrypted_payload.extend_from_slice(&header);
                                            encrypted_payload.extend_from_slice(initivexpand_cmd.as_bytes());
                                            
                                            use sha1::{Digest, Sha1};
                                            let mut hasher = Sha1::new();
                                            hasher.update(client_omega_unescaped.as_bytes());
                                            let client_uid = BASE64_STANDARD.encode(hasher.finalize());

                                            let mut server_hasher = Sha1::new();
                                            let omega_string = BASE64_STANDARD.encode(&omega_bytes);
                                            server_hasher.update(omega_string.as_bytes());
                                            let server_uid = BASE64_STANDARD.encode(server_hasher.finalize());

                                            let assigned_clid = if let Some(old_session) = sessions.get(&addr) {
                                                old_session.is_active.store(false, Ordering::SeqCst);
                                                println!("Deactivating old session for {} on native SolvePuzzle", addr);
                                                old_session.client_id
                                            } else {
                                                NEXT_TS3_CLIENT_ID.fetch_add(1, Ordering::SeqCst)
                                            };
                                            sessions.insert(addr, UdpSession {
                                                client_id: assigned_clid,
                                                seed_client: BASE64_STANDARD.decode(&alpha_unescaped).unwrap_or_default(),
                                                seed_server: beta_bytes.to_vec(),
                                                server_sec,
                                                shared_secret: shared_secret_array,
                                                client_uid,
                                                server_uid,
                                                server_packet_id: Arc::new(Mutex::new(1)),
                                                server_ack_packet_id: 0,
                                                pending_rtc_describe: false,
                                                pending_rtc_is_video: false,
                                                rtc_describe_buffer: Vec::new(),
                                                ephemeral_sec: None,
                                                entry2_hash: None,
                                                command_fragment_buffer: Vec::new(),
                                                command_fragment_active: false,
                                                client_nickname: String::new(),
                                                is_active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                                                is_teaspeak_client: true,
                                                is_ts6_client: false,
                                            });
                                            
                                            let _ = socket.send_to(&encrypted_payload, addr).await;
                                            println!("Sent initivexpand to {}: {}", addr, initivexpand_cmd);
                                        } else {
                                             println!("Handling new protocol (ot=1)...");
                                             
                                             if let Some(parsed_payload) = crate::desktop_proto::parse_ts3init_solve_puzzle_payload(&payload) {
                                                 let mut beta_bytes = [0u8; 54];
                                                 rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut beta_bytes);
                                                 
                                                 use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
                                                 
                                                  let (chain_b64, _identity_sec, root_key_pbl) = crate::desktop_crypto::load_protocol_key().unwrap();
                                                  let mut crypto_chain = BASE64_STANDARD.decode(&chain_b64).unwrap();
                                                  println!("DEBUG: raw chain from protocol_key.txt: {} bytes", crypto_chain.len());
                                                  
                                                  // Step 1: Remove the 0x12-tagged secondary block if present
                                                  // The chain from protocol_key.txt may contain a second chain block 
                                                  // starting at some offset with tag 0x12. We need to find and remove it.
                                                  // Walk entries to find where the valid chain ends.
                                                  {
                                                      let mut pos = 1usize; // skip version byte
                                                      while pos < crypto_chain.len() {
                                                          let tag = crypto_chain[pos];
                                                          if tag == 0x12 {
                                                              // Found secondary block marker - truncate here
                                                              println!("DEBUG: Found 0x12 marker at offset {}, truncating", pos);
                                                              crypto_chain.truncate(pos);
                                                              break;
                                                          }
                                                          if tag != 0x00 {
                                                              println!("DEBUG: Unknown tag 0x{:02x} at offset {}, truncating", tag, pos);
                                                              crypto_chain.truncate(pos);
                                                              break;
                                                          }
                                                          // Skip past this entry: tag(1) + pubkey(32) + type(1) + begin(4) + end(4) = 42 minimum
                                                          if pos + 42 > crypto_chain.len() { break; }
                                                          let entry_type = crypto_chain[pos + 33];
                                                          pos += 42; // base entry size
                                                          match entry_type {
                                                              0x00 => { // Intermediate: + dummy(4) + null-terminated string
                                                                  pos += 4; // dummy
                                                                  while pos < crypto_chain.len() && crypto_chain[pos] != 0 { pos += 1; }
                                                                  if pos < crypto_chain.len() { pos += 1; } // null terminator
                                                              }
                                                              0x02 => { // Server: + license_type(1) + slots(4) + null-terminated string
                                                                  pos += 5; // license_type + slots
                                                                  while pos < crypto_chain.len() && crypto_chain[pos] != 0 { pos += 1; }
                                                                  if pos < crypto_chain.len() { pos += 1; } // null terminator
                                                              }
                                                              0x20 => { // Ephemeral: no extra content - remove it
                                                                  println!("DEBUG: Found existing ephemeral entry at offset {}, removing", pos - 42);
                                                                  crypto_chain.truncate(pos - 42);
                                                              }
                                                              _ => { break; }
                                                          }
                                                      }
                                                  }
                                                  println!("DEBUG: chain after cleanup: {} bytes", crypto_chain.len());
                                                  
                                                  let (server_sec, server_pub) = crate::desktop_crypto::load_or_create_server_keypair();
                                                  let omega_bytes_asn1 = crate::desktop_crypto::export_public_key_libtomcrypt_asn1(&server_pub);
                                                  
                                                  let is_teaspeak = parsed_payload.teaspeak;
                                                  
                                                  // Ephemeral Ed25519 Keypair generation (only for official TS3 client, not teaspeak client)
                                                  // Matches C++ _ed25519_create_keypair: SHA512(seed)[0..32] clamped = private_key,
                                                  // public_key = private_key * Basepoint
                                                  let mut ephemeral_sec_bytes = [0u8; 32];
                                                  let mut ephemeral_pub_bytes = [0u8; 32];
                                                  if !is_teaspeak {
                                                      use sha2::{Digest as _, Sha512};
                                                      use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
                                                      use curve25519_dalek::scalar::Scalar;
                                                      
                                                      // 1. Generate random seed
                                                      let mut seed = [0u8; 32];
                                                      let mut rng = rand::thread_rng();
                                                      rand::RngCore::fill_bytes(&mut rng, &mut seed);
                                                      
                                                      // 2. SHA512(seed) → expanded[0..32] with Ed25519 clamping
                                                      let expanded = Sha512::digest(&seed);
                                                      ephemeral_sec_bytes.copy_from_slice(&expanded[0..32]);
                                                      ephemeral_sec_bytes[0] &= 0xF8;
                                                      ephemeral_sec_bytes[31] &= 0x7F;
                                                      ephemeral_sec_bytes[31] |= 0x40;
                                                      
                                                      // 3. public_key = scalar * Basepoint (direct, no further hashing)
                                                      let scalar = Scalar::from_bytes_mod_order(ephemeral_sec_bytes);
                                                      let public_point = &scalar * ED25519_BASEPOINT_TABLE;
                                                      ephemeral_pub_bytes = public_point.compress().to_bytes();
                                                  }
                                                  
                                                  // Construct Ephemeral Entry
                                                  let mut entry2_bytes = Vec::new();
                                                  entry2_bytes.push(0x00); // Fixed prefix/separator
                                                  if !is_teaspeak {
                                                      entry2_bytes.extend_from_slice(&ephemeral_pub_bytes);
                                                  } else {
                                                      let omega_bytes_raw = crate::desktop_crypto::export_public_key_libtomcrypt(&server_pub);
                                                      entry2_bytes.extend_from_slice(&omega_bytes_raw[0..32]);
                                                  }
                                                  entry2_bytes.push(0x20); // LicenseType::EPHEMERAL (0x20)
                                                  
                                                  const TIMESTAMP_OFFSET: u64 = 1356998400;
                                                  let validation_time = std::time::SystemTime::now()
                                                      .duration_since(std::time::UNIX_EPOCH)
                                                      .unwrap_or_default()
                                                      .as_secs();
                                                  let begin = (validation_time - TIMESTAMP_OFFSET) as u32;
                                                  let end = (validation_time + 30 * 24 * 3600 - TIMESTAMP_OFFSET) as u32;
                                                  entry2_bytes.extend_from_slice(&begin.to_be_bytes());
                                                  entry2_bytes.extend_from_slice(&end.to_be_bytes());
                                                  
                                                  let mut entry2_hash_bytes = [0u8; 64];
                                                  if !is_teaspeak {
                                                      // Compute SHA-512 hash of ephemeral entry body (excluding 0x00 tag byte)
                                                      // This matches C++ LicenseEntry::hash() which does digest::sha512(write().substr(1))
                                                      use sha2::{Digest as _, Sha512};
                                                      let mut hasher = Sha512::new();
                                                      hasher.update(&entry2_bytes[1..]);
                                                      entry2_hash_bytes = hasher.finalize().into();
                                                  }
                                                  
                                                  // Append Ephemeral Entry to Chain
                                                  println!("DEBUG: crypto_chain len BEFORE ephemeral: {}", crypto_chain.len());
                                                  println!("DEBUG: entry2_bytes len: {}", entry2_bytes.len());
                                                  crypto_chain.extend_from_slice(&entry2_bytes);
                                                  println!("DEBUG: crypto_chain len AFTER ephemeral: {}", crypto_chain.len());
                                                  
                                                  let chain_b64_new = BASE64_STANDARD.encode(&crypto_chain);
                                                  println!("DEBUG: chain_b64_new len: {} chars", chain_b64_new.len());
                                                  let root_b64 = BASE64_STANDARD.encode(&root_key_pbl);
                                                  
                                                  let proof = crate::desktop_crypto::generate_server_proof(
                                                      &server_sec.to_bytes(), 
                                                      &crypto_chain
                                                  ).unwrap();
                                                 
                                                 let mut shared_secret_array = [0u8; 64];
                                                 if is_teaspeak {
                                                     if let Some(client_sec1) = crate::desktop_crypto::import_public_key_libtomcrypt_asn1(&parsed_payload.omega_bytes) {
                                                         if let Some(shared_sec_20) = crate::desktop_crypto::calculate_shared_secret(&client_sec1, &server_sec) {
                                                             println!("ot=1 successfully calculated teaspeak shared secret!");
                                                             shared_secret_array[0..20].copy_from_slice(&shared_sec_20);
                                                         }
                                                     }
                                                 }
                                                 
                                                 use sha1::{Digest, Sha1};
                                                 let client_uid = {
                                                     let mut hasher = Sha1::new();
                                                     hasher.update(parsed_payload.omega.as_bytes());
                                                     BASE64_STANDARD.encode(hasher.finalize())
                                                 };
                                                 let server_uid = {
                                                     let mut hasher = Sha1::new();
                                                     let omega_string = BASE64_STANDARD.encode(&omega_bytes_asn1);
                                                     hasher.update(omega_string.as_bytes());
                                                     BASE64_STANDARD.encode(hasher.finalize())
                                                 };
                                                 
                                                 let assigned_clid = if let Some(old_session) = sessions.get(&addr) {
                                                     old_session.is_active.store(false, Ordering::SeqCst);
                                                     println!("Deactivating old session for {} on ot=1 SolvePuzzle", addr);
                                                     old_session.client_id
                                                 } else {
                                                     NEXT_TS3_CLIENT_ID.fetch_add(1, Ordering::SeqCst)
                                                 };
                                                 sessions.insert(addr, UdpSession {
                                                     client_id: assigned_clid,
                                                     seed_client: parsed_payload.alpha_bytes.clone(),
                                                     seed_server: beta_bytes.to_vec(),
                                                     server_sec: server_sec.clone(),
                                                     shared_secret: shared_secret_array,
                                                     client_uid,
                                                     server_uid,
                                                     server_packet_id: Arc::new(Mutex::new(1)),
                                                     server_ack_packet_id: 0,
                                                     pending_rtc_describe: false,
                                                     pending_rtc_is_video: false,
                                                     rtc_describe_buffer: Vec::new(),
                                                     ephemeral_sec: if is_teaspeak { None } else { Some(ephemeral_sec_bytes) },
                                                     entry2_hash: if is_teaspeak { None } else { Some(entry2_hash_bytes) },
                                                     command_fragment_buffer: Vec::new(),
                                                     command_fragment_active: false,
                                                     client_nickname: String::new(),
                                                     is_active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                                                     is_teaspeak_client: is_teaspeak,
                                                     is_ts6_client: false,
                                                 });
                                                
                                                let ts3_escape = |s: &str| -> String {
                                                    s.replace("\\", "\\\\").replace(" ", "\\s").replace("/", "\\/").replace("|", "\\p").replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t").replace("\x0b", "\\v")
                                                };
                                                
                                                let beta_b64 = ts3_escape(&BASE64_STANDARD.encode(&beta_bytes));
                                                let omega_b64 = ts3_escape(&BASE64_STANDARD.encode(&omega_bytes_asn1));
                                                let proof_b64 = ts3_escape(&BASE64_STANDARD.encode(&proof));
                                                let root_b64_esc = ts3_escape(&root_b64);
                                                let chain_b64_esc = ts3_escape(&chain_b64_new);
                                                
                                                let initivexpand2_cmd = format!("initivexpand2 time={} l={} beta={} omega={} proof={} tvd= root={} ot=1",
                                                    validation_time,
                                                    chain_b64_esc,
                                                    beta_b64,
                                                    omega_b64,
                                                    proof_b64,
                                                    root_b64_esc
                                                );
                                                
                                                 let payload_bytes = initivexpand2_cmd.as_bytes();
                                                 let chunk_size = 400; // TS3 fragment limit
                                                 let chunks: Vec<&[u8]> = payload_bytes.chunks(chunk_size).collect();
                                                 let total_chunks = chunks.len();
                                                 
                                                 for (i, chunk) in chunks.iter().enumerate() {
                                                     let mut flags = 0x22; // NewProtocol | Command
                                                     if total_chunks > 1 && (i == 0 || i == total_chunks - 1) {
                                                         flags |= 0x10; // Fragmented
                                                     }
                                                     
                                                     let out_packet_id = i as u16;
                                                     let mut header = [0u8; 3];
                                                     header[0..2].copy_from_slice(&out_packet_id.to_be_bytes());
                                                     header[2] = flags;
                                                     
                                                     let encrypted_payload = crate::desktop_crypto::encrypt_with_dummy_key(out_packet_id, &header, chunk);
                                                     let _ = socket.send_to(&encrypted_payload, addr).await;
                                                 }
                                                 
                                                 if let Some(session) = sessions.get_mut(&addr) {
                                                     let mut lock = session.server_packet_id.lock().unwrap();
                                                     *lock = total_chunks as u16;
                                                 }
                                                 
                                                 println!("Sent fragmented initivexpand2 ({}) to {}: {}", total_chunks, addr, initivexpand2_cmd);
                                            } else {
                                                println!("ot=1: failed to parse solve puzzle payload!");
                                            }
                                        }
                                    }
                                    ObservedDesktopPacket::Ts3EncryptedPacket { mac, packet_id, client_id, flags, payload } => {
                                        if let Some(session) = sessions.get_mut(&addr) {
                                            if !session.is_active.load(Ordering::SeqCst) {
                                                sessions.remove(&addr);
                                                continue;
                                            }
                                            // Construct the header that was used for AAD
                                            let mut header = [0u8; 5];
                                            header[0..2].copy_from_slice(&packet_id.to_be_bytes());
                                            header[2..4].copy_from_slice(&client_id.to_be_bytes());
                                            header[4] = flags;
                                            
                                            // Try decrypting with session key first, fallback to dummy key
                                            let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                                &session.shared_secret, 
                                                &session.seed_client, 
                                                &session.seed_server
                                            );
                                            
                                             let decrypted_opt = if (flags & 0x80) != 0 {
                                                 Some((payload.clone(), "unencrypted"))
                                             } else {
                                                 let mut payload_with_mac = Vec::with_capacity(mac.len() + payload.len());
                                                 payload_with_mac.extend_from_slice(&mac);
                                                 payload_with_mac.extend_from_slice(&payload);
                                                 
                                                 crate::desktop_crypto::decrypt_with_session_key(
                                                     packet_id,
                                                     0, // generation_id
                                                     flags,
                                                     &header,
                                                     &payload_with_mac,
                                                     &iv_struct,
                                                     false
                                                 ).map(|d| (d, "session")).or_else(|| {
                                                     crate::desktop_crypto::decrypt_with_dummy_key(packet_id, &header, &mac, &payload)
                                                         .map(|d| (d, "dummy"))
                                                 })
                                             };

                                             if let Some((decrypted, key_type)) = decrypted_opt {
                                                 println!("teaspeak udp: decrypted packet from {} using {} key. packet_id: {}, flags: {:02X}, payload_hex: {:02X?}", addr, key_type, packet_id, flags, decrypted);
                                                 
                                                 // Update baseline runtime keepalive to prevent zombie housekeeping timeout
                                                 {
                                                     let mut rt = self.runtime.lock().unwrap();
                                                     rt.mark_client_seen(session.client_id);
                                                 }
                                                 
                                                 // Send UDP ACK/PONG if it is a COMMAND (type 2) or PING (type 4)
                                                 // Skip generic ACK for clientek packets - the clientek handler sends its own ACK
                                                 // after computing the shared secret with the correct iv_struct
                                                 let packet_type = flags & 0x0F;
                                                 let is_clientek_packet = key_type == "dummy" && decrypted.windows(8).any(|w| w == b"clientek");
                                                 if (packet_type == 0x02 || packet_type == 0x04) && !is_clientek_packet {
                                                     let ack_payload = packet_id.to_be_bytes();
                                                     let mut ack_header = [0u8; 3];
                                                     let is_new_protocol = (flags & 0x20) != 0;
                                                     
                                                     let resp_type = if packet_type == 0x04 { 0x05 } else { 0x06 };
                                                     // ACKs and PONGs are ALWAYS unencrypted (0x80 flag set)
                                                     let ack_flags = if is_new_protocol {
                                                         0x80 | 0x20 | resp_type // Unencrypted | NewProtocol | type
                                                     } else {
                                                         0x80 | resp_type // Unencrypted | type
                                                     };
                                                     ack_header[0..2].copy_from_slice(&session.server_ack_packet_id.to_be_bytes());
                                                     ack_header[2] = ack_flags;
                                                     
                                                     // Compute MAC: SHA1(iv_struct)[0..7] if shared secret is set, else zeros
                                                     let mac = if session.shared_secret.iter().any(|&b| b != 0) {
                                                         use sha1::{Sha1, Digest as _};
                                                         let hash = Sha1::digest(&iv_struct);
                                                         let mut m = [0u8; 8];
                                                         m.copy_from_slice(&hash[0..8]);
                                                         m
                                                     } else {
                                                         [0u8; 8]
                                                     };
                                                     
                                                     let mut final_ack = Vec::with_capacity(13);
                                                     final_ack.extend_from_slice(&mac);
                                                     final_ack.extend_from_slice(&ack_header);
                                                     final_ack.extend_from_slice(&ack_payload);
                                                     let _ = socket.send_to(&final_ack, addr).await;
                                                     
                                                     session.server_ack_packet_id = session.server_ack_packet_id.wrapping_add(1);
                                                     println!("Sent UDP {} {} for client packet_id {} (flags={:02X})", if resp_type == 0x05 { "PONG" } else { "ACK" }, session.server_ack_packet_id - 1, packet_id, ack_flags);
                                                 }

                                                if packet_type == 0x00 || packet_type == 0x01 {
                                                    let btea_type = if packet_type == 0x01 { 0x0B } else { 0x0A };
                                                    let is_whisper = packet_type == 0x01;
                                                    if decrypted.len() > 3 {
                                                        let raw_opus = &decrypted[3..];
                                                        let rt = self.runtime.lock().unwrap();
                                                        if !is_whisper && !rt.client_can_speak_in_channel(self.server_id, session.client_id) {
                                                            continue;
                                                        }
                                                        if let Some(c) = rt.online_client_snapshot(self.server_id, session.client_id) {
                                                            rt.broadcast_event(self.server_id, &crate::transport::TransportNotification::TalkStatus {
                                                                server_id: self.server_id,
                                                                channel_id: c.channel_id,
                                                                client_id: session.client_id,
                                                                is_talking: true,
                                                                is_whisper,
                                                                whisper_targets: c.whisper_targets.clone(),
                                                            });
                                                        }
                                                        rt.route_btea_media_to_desktop(self.server_id, session.client_id, btea_type, raw_opus);
                                                        rt.route_btea_media_to_webtransport(self.server_id, session.client_id, 1, raw_opus);
                                                    } else {
                                                        let rt = self.runtime.lock().unwrap();
                                                        if let Some(c) = rt.online_client_snapshot(self.server_id, session.client_id) {
                                                            rt.broadcast_event(self.server_id, &crate::transport::TransportNotification::TalkStatus {
                                                                server_id: self.server_id,
                                                                channel_id: c.channel_id,
                                                                client_id: session.client_id,
                                                                is_talking: false,
                                                                is_whisper,
                                                                whisper_targets: c.whisper_targets.clone(),
                                                            });
                                                        }
                                                        rt.route_btea_media_to_desktop(self.server_id, session.client_id, btea_type, &[]);
                                                        rt.route_btea_media_to_webtransport(self.server_id, session.client_id, 1, &[]);
                                                    }
                                                } else if packet_type == 0x02 || packet_type == 0x03 {
                                                    // TS3 command packets exceeding MTU are split across fragments (flag 0x10).
                                                    // The first and last packets in the sequence have the 0x10 flag set, while intermediate packets do not.
                                                    let is_fragmented = (flags & 0x10) != 0;
                                                    let mut defragmented_payload = None;
                                                    if is_fragmented {
                                                        if !session.command_fragment_active {
                                                            // First fragment: initiate a new reassembly buffer.
                                                            session.command_fragment_active = true;
                                                            session.command_fragment_buffer.clear();
                                                            session.command_fragment_buffer.extend_from_slice(&decrypted);
                                                            println!("teaspeak udp: received first fragment of command. packet_id: {}", packet_id);
                                                        } else {
                                                            // Last fragment: append data and finalize reassembled payload.
                                                            session.command_fragment_buffer.extend_from_slice(&decrypted);
                                                            session.command_fragment_active = false;
                                                            defragmented_payload = Some(session.command_fragment_buffer.clone());
                                                            println!("teaspeak udp: received last fragment of command. assembled len: {}", session.command_fragment_buffer.len());
                                                        }
                                                    } else {
                                                        if session.command_fragment_active {
                                                            // Intermediate fragment: append to active buffer.
                                                            session.command_fragment_buffer.extend_from_slice(&decrypted);
                                                            println!("teaspeak udp: received middle fragment of command. packet_id: {}", packet_id);
                                                        } else {
                                                            // Unfragmented payload: pass through directly.
                                                            defragmented_payload = Some(decrypted.clone());
                                                        }
                                                    }

                                                    if let Some(final_payload) = defragmented_payload {
                                                        let processed_decrypted = decompress_if_needed(&final_payload);
                                                        let payload_str = String::from_utf8_lossy(&processed_decrypted);
                                                        println!("teaspeak udp: payload_str: {}", payload_str);

                                                        if payload_str.starts_with("handshakebegin ") {
                                                            session.is_teaspeak_client = true;
                                                        } else if !session.is_teaspeak_client
                                                            && (payload_str.contains("client_version=6.")
                                                                || payload_str.contains("client_version=5.")
                                                                || payload_str.starts_with("setupstream")
                                                                || payload_str.starts_with("updatestream")
                                                                || payload_str.starts_with("stopstream")
                                                                || payload_str.starts_with("requeststreaminfo")
                                                                || payload_str.starts_with("joinstreamrequest")
                                                                || payload_str.starts_with("respondjoinstreamrequest")
                                                                || payload_str.starts_with("streamsignaling")
                                                                || payload_str.starts_with("removeclientfromstream")
                                                                || payload_str.starts_with("requestsfuaccessinfo"))
                                                        {
                                                            session.is_ts6_client = true;
                                                        }

                                                        if session.is_teaspeak_client {
                                                            // TeaSpeak-only pipeline (src/protocols/teaspeak/)
                                                            crate::protocols::teaspeak::webrtc_bridge::handle_incoming_webrtc_accumulate(
                                                                &payload_str,
                                                                &final_payload,
                                                                session,
                                                            );

                                                            let handled = crate::protocols::teaspeak::native_framing::handle_teaspeak_native(
                                                                &payload_str,
                                                                session,
                                                                addr,
                                                                &socket,
                                                                &iv_struct,
                                                            )
                                                            .await;

                                                            if !handled {
                                                                if payload_str.starts_with("musicbotplayerinfo") {
                                                                    pending_song_info_sync.clear();
                                                                }
                                                                let _ = crate::protocols::teaspeak::command_handler::handle_teaspeak_command(
                                                                    &payload_str,
                                                                    &processed_decrypted,
                                                                    session,
                                                                    addr,
                                                                    &socket,
                                                                    &iv_struct,
                                                                    &self.runtime,
                                                                    self.server_id,
                                                                    packet_id,
                                                                )
                                                                .await;
                                                            }

                                                            crate::protocols::teaspeak::webrtc_bridge::handle_webrtc_action(
                                                                &payload_str,
                                                                session,
                                                                addr,
                                                                &socket,
                                                                &self.rtc_manager,
                                                                &iv_struct,
                                                                &self.runtime,
                                                                self.server_id,
                                                            )
                                                            .await;
                                                        } else if session.is_ts6_client {
                                                            // Official TeamSpeak 6 pipeline (src/protocols/ts6/)
                                                            let _ = crate::protocols::ts6::command_handler::handle_ts6_command(
                                                                &payload_str,
                                                                &processed_decrypted,
                                                                session,
                                                                addr,
                                                                &socket,
                                                                &iv_struct,
                                                                &self.runtime,
                                                                self.server_id,
                                                                packet_id,
                                                            )
                                                            .await;
                                                        } else {
                                                            // Official TeamSpeak 3 pipeline (src/protocols/ts3/)
                                                            let _ = crate::protocols::ts3::command_handler::handle_ts3_command(
                                                                &payload_str,
                                                                &processed_decrypted,
                                                                session,
                                                                addr,
                                                                &socket,
                                                                &iv_struct,
                                                                &self.runtime,
                                                                self.server_id,
                                                                packet_id,
                                                            )
                                                            .await;
                                                        }

                                                        if !session.is_active.load(Ordering::SeqCst) {
                                                            println!("UDP session for {} disconnected cleanly; removing session.", addr);
                                                            sessions.remove(&addr);
                                                            continue;
                                                        }
                                                    }
                                                }
                                            } else {
                                                println!("teaspeak udp: failed to decrypt packet from {} (flags={:02X}, id={})", addr, flags, packet_id);
                                            }
                                        } else if let Some(secrets_arc) = &self.shared_secrets {
                                            let mut header = [0u8; 5];
                                            header[0..2].copy_from_slice(&packet_id.to_be_bytes());
                                            header[2..4].copy_from_slice(&client_id.to_be_bytes());
                                            header[4] = flags;
                                            let mut payload_with_mac = Vec::with_capacity(mac.len() + payload.len());
                                            payload_with_mac.extend_from_slice(&mac);
                                            payload_with_mac.extend_from_slice(&payload);

                                            let resolved = {
                                                let secrets_lock = secrets_arc.lock().unwrap();
                                                let candidate_cid = if client_id > 0 {
                                                    Some(client_id as u64)
                                                } else {
                                                    btea_addr_to_cid.get(&addr).copied()
                                                };
                                                if let Some(cid) = candidate_cid {
                                                    secrets_lock.get(&cid).and_then(|secret| {
                                                        crate::desktop_crypto::decrypt_btea_packet(
                                                            packet_id, 0, flags, &header, &payload_with_mac, secret, false
                                                        ).map(|d| (cid, d))
                                                    })
                                                } else {
                                                    let mut found = None;
                                                    for (&cid, secret) in secrets_lock.iter() {
                                                        if let Some(d) = crate::desktop_crypto::decrypt_btea_packet(
                                                            packet_id, 0, flags, &header, &payload_with_mac, secret, false
                                                        ) {
                                                            found = Some((cid, d));
                                                            break;
                                                        }
                                                    }
                                                    found
                                                }
                                            };

                                            if let Some((resolved_cid, decrypted)) = resolved {
                                                btea_addr_to_cid.insert(addr, resolved_cid);
                                                btea_udp_sessions.entry(resolved_cid).or_insert((addr, 0)).0 = addr;
                                                let packet_type = flags & 0x0F;
                                                let mut rt = self.runtime.lock().unwrap();
                                                rt.mark_client_seen(resolved_cid);
                                                if (packet_type == 0x0A || packet_type == 0x0B || packet_type == 0x01) && !decrypted.is_empty() {
                                                    let is_whisper = packet_type == 0x0B || packet_type == 0x01;
                                                    if !is_whisper && !rt.client_can_speak_in_channel(self.server_id, resolved_cid) {
                                                        continue;
                                                    }
                                                    if let Some(c) = rt.online_client_snapshot(self.server_id, resolved_cid) {
                                                        rt.broadcast_event(self.server_id, &crate::transport::TransportNotification::TalkStatus {
                                                            server_id: self.server_id,
                                                            channel_id: c.channel_id,
                                                            client_id: resolved_cid,
                                                            is_talking: true,
                                                            is_whisper,
                                                            whisper_targets: c.whisper_targets.clone(),
                                                        });
                                                    }
                                                    rt.route_btea_media_to_desktop(self.server_id, resolved_cid, packet_type, &decrypted);
                                                    rt.route_btea_media_to_webtransport(self.server_id, resolved_cid, 1, &decrypted);
                                                }
                                            }
                                        } else {
                                            println!("teaspeak udp: received encrypted packet from unknown session {}", addr);
                                        }
                                    }
                                    _ => {}
                                }
                            } else {
                                println!("teaspeak udp: received non-init packet from {} (len={})", addr, size);
                                println!("teaspeak udp: RAW NON-INIT = {:02X?}", packet);
                            }
                        }
                    }
                    _ = music_status_interval.tick() => {
                        let mut retry_payloads = Vec::new();
                        pending_song_info_sync.retain(|&bot_clid, (since, retries)| {
                            if *retries >= 2 {
                                return false;
                            }
                            if since.elapsed() >= Duration::from_millis(1050) {
                                *retries += 1;
                                *since = std::time::Instant::now();
                                if let Some(p) = self
                                    .runtime
                                    .lock()
                                    .unwrap()
                                    .music_bot_notify_payload_by_identifier(self.server_id, bot_clid)
                                {
                                    retry_payloads.push(p);
                                    return true;
                                }
                                return false;
                            }
                            true
                        });
                        let status_rows = self
                            .runtime
                            .lock()
                            .unwrap()
                            .playing_music_bots_status_rows(self.server_id);
                        if !status_rows.is_empty() || !retry_payloads.is_empty() {
                            for (peer_addr, peer_session) in sessions.iter() {
                                if !peer_session.is_teaspeak_client
                                    || peer_session.client_nickname.is_empty()
                                    || !peer_session.is_active.load(Ordering::SeqCst)
                                {
                                    continue;
                                }
                                let iv_struct = crate::desktop_crypto::derive_iv_struct(
                                    &peer_session.shared_secret,
                                    &peer_session.seed_client,
                                    &peer_session.seed_server,
                                );
                                for payload in &retry_payloads {
                                    let p_state = payload
                                        .client_updates
                                        .get("player_state")
                                        .map(String::as_str)
                                        .unwrap_or("4");
                                    if p_state == "2" {
                                        let p_vol = payload
                                            .client_updates
                                            .get("player_volume")
                                            .map(String::as_str)
                                            .unwrap_or("1");
                                        let p_plid = payload
                                            .client_updates
                                            .get("client_playlist_id")
                                            .map(String::as_str)
                                            .unwrap_or("0");
                                        let song_cmd = format!(
                                            "notifymusicplayersongchange {}",
                                            crate::protocols::teaspeak::wire_formatting::render_teaspeak_rows_only(std::slice::from_ref(&payload.song_change))
                                        );
                                        let client_upd_cmd = format!(
                                            "notifyclientupdated clid={} player_state={} player_volume={} client_playlist_id={}",
                                            payload.client_id,
                                            crate::protocols::teaspeak::wire_formatting::teaspeak_escape(p_state),
                                            crate::protocols::teaspeak::wire_formatting::teaspeak_escape(p_vol),
                                            crate::protocols::teaspeak::wire_formatting::teaspeak_escape(p_plid),
                                        );
                                        crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(
                                            &song_cmd,
                                            peer_session,
                                            *peer_addr,
                                            &socket,
                                            &iv_struct,
                                        )
                                        .await;
                                        crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(
                                            &client_upd_cmd,
                                            peer_session,
                                            *peer_addr,
                                            &socket,
                                            &iv_struct,
                                        )
                                        .await;
                                    }
                                }
                                for row in &status_rows {
                                    let status_cmd = format!(
                                        "notifymusicstatusupdate {}",
                                        crate::protocols::teaspeak::wire_formatting::render_teaspeak_rows_only(std::slice::from_ref(row))
                                    );
                                    crate::protocols::teaspeak::wire_formatting::send_teaspeak_command(
                                        &status_cmd,
                                        peer_session,
                                        *peer_addr,
                                        &socket,
                                        &iv_struct,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }
}
