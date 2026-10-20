use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::net::UdpSocket;
use webrtc::api::media_engine::{MIME_TYPE_OPUS, MIME_TYPE_VP8, MediaEngine};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::rtp_transceiver::RTCPFeedback;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};

use super::wire_formatting::{send_ts6_command_with_packet_id, ts6_escape};
use crate::protocols::teaspeak::rtc_manager::Vp8Depacketizer;
use crate::runtime::BaselineRuntime;

/// Virtual viewer `clid` used when the server auto-joins a TS6 Camera stream (`bt = 0`).
pub const VIRTUAL_BRIDGE_VIEWER_CAMERA_CLID: u64 = 65530;
/// Virtual viewer `clid` used when the server auto-joins a TS6 Screen/Window stream (`bt = 1`).
pub const VIRTUAL_BRIDGE_VIEWER_SCREEN_CLID: u64 = 65531;

pub fn virtual_bridge_viewer_clid_for_bt(bt: u32) -> u64 {
    if bt == 1 {
        VIRTUAL_BRIDGE_VIEWER_SCREEN_CLID
    } else {
        VIRTUAL_BRIDGE_VIEWER_CAMERA_CLID
    }
}

pub fn is_virtual_bridge_viewer_clid(clid: u64) -> bool {
    clid == VIRTUAL_BRIDGE_VIEWER_CAMERA_CLID || clid == VIRTUAL_BRIDGE_VIEWER_SCREEN_CLID
}

#[derive(Clone)]
pub struct Ts6InboundBridge {
    pub stream_id: String,
    pub server_id: u32,
    pub streamer_clid: u64,
    pub virtual_viewer_clid: u64,
    pub bt: u32,
    pub pc: Arc<RTCPeerConnection>,
    pub server_packet_id: Arc<Mutex<u16>>,
    pub addr: SocketAddr,
    pub socket: Arc<UdpSocket>,
    pub iv_struct: Vec<u8>,
}

#[derive(Clone)]
pub struct Ts6OutboundBridge {
    pub stream_id: String,
    pub server_id: u32,
    pub broadcaster_clid: u64,
    pub viewer_clid: u64,
    pub bt: u32,
    pub ssrc: u32,
    pub pc: Arc<RTCPeerConnection>,
}

#[derive(Default)]
pub struct Ts6WebRtcBridgeRegistry {
    /// Keyed by `stream_id` (for streams originated by a TS6 client)
    pub inbound_streams: HashMap<String, Ts6InboundBridge>,
    /// Keyed by `(stream_id, viewer_clid)` (for TS6 clients viewing non-TS6 broadcasts)
    pub outbound_viewers: HashMap<(String, u64), Ts6OutboundBridge>,
}

pub static TS6_WEBRTC_BRIDGE: LazyLock<Mutex<Ts6WebRtcBridgeRegistry>> =
    LazyLock::new(|| Mutex::new(Ts6WebRtcBridgeRegistry::default()));

fn extract_codec_payload_types(sdp: &str, codec_name_lower: &str) -> Vec<u8> {
    let mut pts = Vec::new();
    for raw_line in sdp.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("a=rtpmap:") {
            let mut parts = rest.split_whitespace();
            if let (Some(pt_str), Some(encoding)) = (parts.next(), parts.next()) {
                if encoding
                    .to_ascii_lowercase()
                    .starts_with(&format!("{}/", codec_name_lower))
                    && let Ok(pt) = pt_str.parse::<u8>()
                    && !pts.contains(&pt)
                {
                    pts.push(pt);
                }
            }
        }
    }
    pts
}

fn extract_offer_video_ssrc(sdp: &str) -> Option<u32> {
    let mut in_video = false;
    for raw_line in sdp.lines() {
        let line = raw_line.trim();
        if line.starts_with("m=") {
            in_video = line.starts_with("m=video");
        } else if in_video && let Some(rest) = line.strip_prefix("a=ssrc:") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(ssrc) = digits.parse::<u32>()
                && ssrc != 0
            {
                return Some(ssrc);
            }
        }
    }
    None
}

fn ensure_video_ssrc_in_sdp(sdp: &str, ssrc: u32, stream_id: &str, track_id: &str) -> String {
    let mut out_lines = Vec::new();
    let mut in_video = false;
    let mut video_has_ssrc = false;

    for raw_line in sdp.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.starts_with("m=") {
            if in_video && !video_has_ssrc {
                out_lines.push(format!("a=ssrc:{} cname:blackteaspeak-video", ssrc));
                out_lines.push(format!("a=ssrc:{} msid:{} {}", ssrc, stream_id, track_id));
            }
            in_video = line.starts_with("m=video");
            video_has_ssrc = false;
            out_lines.push(line.to_string());
            continue;
        }
        if in_video && line.starts_with("a=ssrc:") {
            video_has_ssrc = true;
        }
        out_lines.push(line.to_string());
    }

    if in_video && !video_has_ssrc {
        out_lines.push(format!("a=ssrc:{} cname:blackteaspeak-video", ssrc));
        out_lines.push(format!("a=ssrc:{} msid:{} {}", ssrc, stream_id, track_id));
    }

    let mut joined = out_lines.join("\r\n");
    joined.push_str("\r\n");
    joined
}

fn build_ts6_media_engine(offer_sdp: Option<&str>, prefer_pt_120_first: bool) -> MediaEngine {
    let mut m = MediaEngine::default();

    let mut opus_pts = offer_sdp
        .map(|sdp| extract_codec_payload_types(sdp, "opus"))
        .unwrap_or_default();
    if !opus_pts.contains(&111) {
        opus_pts.push(111);
    }
    for pt in opus_pts {
        let _ = m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: pt,
                ..Default::default()
            },
            RTPCodecType::Audio,
        );
    }

    let video_rtcp_feedback = vec![
        RTCPFeedback {
            typ: "goog-remb".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "ccm".to_owned(),
            parameter: "fir".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "pli".to_owned(),
        },
    ];

    let mut vp8_pts = offer_sdp
        .map(|sdp| extract_codec_payload_types(sdp, "vp8"))
        .unwrap_or_default();
    if prefer_pt_120_first {
        if !vp8_pts.contains(&120) {
            vp8_pts.push(120);
        }
        if !vp8_pts.contains(&96) {
            vp8_pts.push(96);
        }
    } else {
        if !vp8_pts.contains(&96) {
            vp8_pts.push(96);
        }
        if !vp8_pts.contains(&120) {
            vp8_pts.push(120);
        }
    }

    for pt in vp8_pts {
        let _ = m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: pt,
                ..Default::default()
            },
            RTPCodecType::Video,
        );
    }

    m
}

/// Starts a server-side WebRTC Viewer (`RTCPeerConnection`) that receives a TS6 streamer's P2P video
/// (`respondjoinstreamrequest` with `offer=`) and bridges it to:
/// 1. TeaSpeak clients (via `RtcBroadcastManager` `viewer_slots`)
/// 2. BTEA Desktop & Web clients (via `Vp8Depacketizer` -> `route_btea_media_to_webtransport`)
#[allow(clippy::too_many_arguments)]
pub fn start_ts6_inbound_bridge(
    stream_id: String,
    server_id: u32,
    streamer_clid: u64,
    virtual_viewer_clid: u64,
    bt: u32,
    offer_sdp: String,
    server_packet_id: Arc<Mutex<u16>>,
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    iv_struct: Vec<u8>,
    runtime: Arc<Mutex<BaselineRuntime>>,
) {
    let rtc_manager = {
        let rt = runtime.lock().unwrap();
        Arc::clone(&rt.rtc_manager)
    };

    if let Some(offer_ssrc) = extract_offer_video_ssrc(&offer_sdp) {
        rtc_manager.register_incoming_ssrc_type(streamer_clid, offer_ssrc, bt);
    }

    tokio::spawn(async move {
        let m = build_ts6_media_engine(Some(&offer_sdp), false);
        let api = APIBuilder::new().with_media_engine(m).build();
        let Ok(pc) = api.new_peer_connection(RTCConfiguration::default()).await else {
            return;
        };
        let pc = Arc::new(pc);

        {
            let mut reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
            if let Some(old) = reg.inbound_streams.insert(
                stream_id.clone(),
                Ts6InboundBridge {
                    stream_id: stream_id.clone(),
                    server_id,
                    streamer_clid,
                    virtual_viewer_clid,
                    bt,
                    pc: Arc::clone(&pc),
                    server_packet_id: Arc::clone(&server_packet_id),
                    addr,
                    socket: Arc::clone(&socket),
                    iv_struct: iv_struct.clone(),
                },
            ) {
                let old_pc = old.pc;
                tokio::spawn(async move {
                    let _ = old_pc.close().await;
                });
            }
        }
        rtc_manager.register_stream_connection(streamer_clid, bt, Arc::clone(&pc));

        let answer_sent = Arc::new(AtomicBool::new(false));
        let answer_sent_ice = Arc::clone(&answer_sent);
        let stream_id_ice = stream_id.clone();
        let pid_ice = Arc::clone(&server_packet_id);
        let sock_ice = Arc::clone(&socket);
        let iv_ice = iv_struct.clone();
        pc.on_ice_candidate(Box::new(move |c| {
            let answer_sent_ice = Arc::clone(&answer_sent_ice);
            let stream_id_ice = stream_id_ice.clone();
            let pid_ice = Arc::clone(&pid_ice);
            let sock_ice = Arc::clone(&sock_ice);
            let iv_ice = iv_ice.clone();
            Box::pin(async move {
                if let Some(c) = c
                    && let Ok(init) = c.to_json()
                    && !init.candidate.is_empty()
                {
                    for _ in 0..25 {
                        if answer_sent_ice.load(Ordering::SeqCst) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    let mid = init
                        .sdp_mid
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| String::from("0"));
                    let mline = init.sdp_mline_index.unwrap_or(0);
                    let json_str = serde_json::json!({
                        "cmd": "iceCandidate",
                        "args": {
                            "sdp": init.candidate,
                            "mid": mid,
                            "mLine": mline,
                        }
                    })
                    .to_string();
                    let cmd = format!(
                        "notifystreamsignaling clid={} id={} json={}",
                        virtual_viewer_clid,
                        ts6_escape(&stream_id_ice),
                        ts6_escape(&json_str)
                    );
                    send_ts6_command_with_packet_id(&cmd, &pid_ice, addr, &sock_ice, &iv_ice).await;
                }
            })
        }));

        let rtc_mgr_track = Arc::clone(&rtc_manager);
        let rt_track = Arc::clone(&runtime);
        let pc_track = Arc::clone(&pc);
        let stream_id_track = stream_id.clone();
        pc.on_track(Box::new(move |track, _receiver, _transceiver| {
            let rtc_mgr = Arc::clone(&rtc_mgr_track);
            let rt_for_track = Arc::clone(&rt_track);
            let pc_for_track = Arc::clone(&pc_track);
            let stream_id_for_track = stream_id_track.clone();
            let track = Arc::clone(&track);
            tokio::spawn(async move {
                let mime = track.codec().capability.mime_type.to_ascii_lowercase();
                if !mime.contains("video") && !mime.contains("vp8") {
                    while track.read_rtp().await.is_ok() {}
                    return;
                }

                let track_ssrc = track.ssrc();
                rtc_mgr.register_incoming_ssrc_type(streamer_clid, track_ssrc, bt);
                println!(
                    "TS6 inbound WebRTC video track ready: stream_id={} clid={} bt={} ssrc={}",
                    stream_id_for_track, streamer_clid, bt, track_ssrc
                );

                let mut depacketizer = Vp8Depacketizer::new();
                let mut first_packet = true;
                let mut last_pli = std::time::Instant::now();

                while let Ok((packet, _)) = track.read_rtp().await {
                    if first_packet || last_pli.elapsed() >= std::time::Duration::from_secs(2) {
                        first_packet = false;
                        last_pli = std::time::Instant::now();
                        let _ = pc_for_track
                            .write_rtcp(&[Box::new(PictureLossIndication {
                                sender_ssrc: 0,
                                media_ssrc: track_ssrc,
                            })])
                            .await;
                    }

                    // 1. Forward RTP packet to any subscribed TeaSpeak or TS6 viewers
                    let viewers = rtc_mgr.get_subscribed_viewer_tracks(streamer_clid, bt);
                    for (_slot_ssrc, viewer_track, seq_num) in &viewers {
                        let mut out_pkt = packet.clone();
                        out_pkt.header.payload_type = 120;
                        out_pkt.header.sequence_number = seq_num.fetch_add(1, Ordering::Relaxed);
                        out_pkt.header.extension = false;
                        out_pkt.header.extension_profile = 0;
                        out_pkt.header.extensions.clear();
                        out_pkt.header.extensions_padding = 0;
                        let _ = viewer_track.write_rtp(&out_pkt).await;
                    }

                    // 2. Depacketize RFC 7741 VP8 into 13-byte BTEA frame and route to BTEA Desktop & Web clients
                    if let Some(btea_frame) = depacketizer.push_rtp(
                        packet.header.timestamp,
                        packet.header.marker,
                        &packet.payload,
                    ) {
                        let wt_packet_type = if bt == 1 { 4 } else { 2 };
                        let rt = rt_for_track.lock().unwrap();
                        rt.route_btea_media_to_webtransport(
                            server_id,
                            streamer_clid,
                            wt_packet_type,
                            &btea_frame,
                        );
                    }
                }

                println!(
                    "TS6 inbound WebRTC video track ended: stream_id={} clid={} bt={} ssrc={}",
                    stream_id_for_track, streamer_clid, bt, track_ssrc
                );
            });
            Box::pin(async {})
        }));

        let Ok(offer) = RTCSessionDescription::offer(offer_sdp) else {
            return;
        };
        if pc.set_remote_description(offer).await.is_err() {
            return;
        }
        let Ok(answer) = pc.create_answer(None).await else {
            return;
        };

        let (ice_tx, mut ice_rx) = tokio::sync::mpsc::channel::<()>(1);
        pc.on_ice_gathering_state_change(Box::new(move |state| {
            if state == RTCIceGathererState::Complete {
                let _ = ice_tx.try_send(());
            }
            Box::pin(async move {})
        }));

        if pc.set_local_description(answer).await.is_err() {
            return;
        }
        let _ = tokio::time::timeout(std::time::Duration::from_millis(150), ice_rx.recv()).await;

        if let Some(local_sdp) = pc.local_description().await {
            let answer_json = serde_json::json!({
                "cmd": "answer",
                "args": {
                    "answer": local_sdp.sdp,
                }
            })
            .to_string();
            let sig_cmd = format!(
                "notifystreamsignaling clid={} id={} json={}",
                virtual_viewer_clid,
                ts6_escape(&stream_id),
                ts6_escape(&answer_json)
            );
            send_ts6_command_with_packet_id(&sig_cmd, &server_packet_id, addr, &socket, &iv_struct)
                .await;
            answer_sent.store(true, Ordering::SeqCst);
            println!(
                "Sent TS6 inbound bridge WebRTC SDP answer for stream_id={} to streamer clid={}",
                stream_id, streamer_clid
            );
        }
    });
}

/// Starts a server-side WebRTC Streamer (`RTCPeerConnection`) when a TS6 client sends `joinstreamrequest`
/// to watch a non-TS6 stream (TeaSpeak client, BTEA Desktop/Web client, or MusicBot).
#[allow(clippy::too_many_arguments)]
pub fn start_ts6_outbound_bridge(
    stream_id: String,
    server_id: u32,
    broadcaster_clid: u64,
    viewer_clid: u64,
    bt: u32,
    server_packet_id: Arc<Mutex<u16>>,
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    iv_struct: Vec<u8>,
    runtime: Arc<Mutex<BaselineRuntime>>,
) {
    let rtc_manager = {
        let rt = runtime.lock().unwrap();
        Arc::clone(&rt.rtc_manager)
    };

    tokio::spawn(async move {
        let m = build_ts6_media_engine(None, true);
        let api = APIBuilder::new().with_media_engine(m).build();
        let Ok(pc) = api.new_peer_connection(RTCConfiguration::default()).await else {
            return;
        };
        let pc = Arc::new(pc);

        let track_id = format!("video-{}-{}", broadcaster_clid, bt);
        let msid = format!("stream-{}-{}", broadcaster_clid, bt);
        let video_track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            track_id.clone(),
            msid.clone(),
        ));

        let Ok(rtp_sender) = pc
            .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await
        else {
            return;
        };

        // Drain incoming RTCP packets on the outbound video sender and forward PLI/FIR keyframe requests
        let rtc_mgr_rtcp = Arc::clone(&rtc_manager);
        let sender_for_rtcp = Arc::clone(&rtp_sender);
        tokio::spawn(async move {
            while let Ok((rtcp_packets, _)) = sender_for_rtcp.read_rtcp().await {
                for pkt in rtcp_packets {
                    if pkt.as_any().is::<PictureLossIndication>()
                        || pkt.as_any().is::<FullIntraRequest>()
                    {
                        rtc_mgr_rtcp.request_keyframe(broadcaster_clid, bt).await;
                    }
                }
            }
        });

        let offer_sent = Arc::new(AtomicBool::new(false));
        let offer_sent_ice = Arc::clone(&offer_sent);
        let stream_id_ice = stream_id.clone();
        let pid_ice = Arc::clone(&server_packet_id);
        let sock_ice = Arc::clone(&socket);
        let iv_ice = iv_struct.clone();
        pc.on_ice_candidate(Box::new(move |c| {
            let offer_sent_ice = Arc::clone(&offer_sent_ice);
            let stream_id_ice = stream_id_ice.clone();
            let pid_ice = Arc::clone(&pid_ice);
            let sock_ice = Arc::clone(&sock_ice);
            let iv_ice = iv_ice.clone();
            Box::pin(async move {
                if let Some(c) = c
                    && let Ok(init) = c.to_json()
                    && !init.candidate.is_empty()
                {
                    for _ in 0..25 {
                        if offer_sent_ice.load(Ordering::SeqCst) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    let mid = init
                        .sdp_mid
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| String::from("0"));
                    let mline = init.sdp_mline_index.unwrap_or(0);
                    let json_str = serde_json::json!({
                        "cmd": "iceCandidate",
                        "args": {
                            "sdp": init.candidate,
                            "mid": mid,
                            "mLine": mline,
                        }
                    })
                    .to_string();
                    let cmd = format!(
                        "notifystreamsignaling clid={} id={} json={}",
                        broadcaster_clid,
                        ts6_escape(&stream_id_ice),
                        ts6_escape(&json_str)
                    );
                    send_ts6_command_with_packet_id(&cmd, &pid_ice, addr, &sock_ice, &iv_ice).await;
                }
            })
        }));

        let Ok(offer) = pc.create_offer(None).await else {
            return;
        };

        let (ice_tx, mut ice_rx) = tokio::sync::mpsc::channel::<()>(1);
        pc.on_ice_gathering_state_change(Box::new(move |state| {
            if state == RTCIceGathererState::Complete {
                let _ = ice_tx.try_send(());
            }
            Box::pin(async move {})
        }));

        if pc.set_local_description(offer).await.is_err() {
            return;
        }
        let _ = tokio::time::timeout(std::time::Duration::from_millis(150), ice_rx.recv()).await;

        let params = rtp_sender.get_parameters().await;
        let ssrc = params
            .encodings
            .first()
            .map(|e| e.ssrc)
            .filter(|&s| s != 0)
            .unwrap_or(0x6000_0000 + (broadcaster_clid as u32) * 16 + bt);

        {
            let mut reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
            if let Some(old) = reg.outbound_viewers.insert(
                (stream_id.clone(), viewer_clid),
                Ts6OutboundBridge {
                    stream_id: stream_id.clone(),
                    server_id,
                    broadcaster_clid,
                    viewer_clid,
                    bt,
                    ssrc,
                    pc: Arc::clone(&pc),
                },
            ) {
                let old_pc = old.pc;
                tokio::spawn(async move {
                    let _ = old_pc.close().await;
                });
            }
        }

        rtc_manager.upsert_ts6_viewer_slot(viewer_clid, broadcaster_clid, bt, ssrc, video_track);

        if let Some(local_sdp) = pc.local_description().await {
            let patched_sdp =
                ensure_video_ssrc_in_sdp(&local_sdp.sdp, ssrc, &msid, &track_id);
            let respond_cmd = format!(
                "notifyrespondjoinstreamrequest clid={} id={} msg= offer={} decision=1",
                broadcaster_clid,
                ts6_escape(&stream_id),
                ts6_escape(&patched_sdp),
            );
            send_ts6_command_with_packet_id(
                &respond_cmd,
                &server_packet_id,
                addr,
                &socket,
                &iv_struct,
            )
            .await;

            let joined_cmd = format!(
                "notifystreamclientjoined clid={} id={}",
                viewer_clid,
                ts6_escape(&stream_id)
            );
            send_ts6_command_with_packet_id(
                &joined_cmd,
                &server_packet_id,
                addr,
                &socket,
                &iv_struct,
            )
            .await;

            offer_sent.store(true, Ordering::SeqCst);
            rtc_manager.request_keyframe(broadcaster_clid, bt).await;
            println!(
                "Sent TS6 outbound bridge WebRTC SDP offer for stream_id={} (broadcaster={}) to viewer clid={}",
                stream_id, broadcaster_clid, viewer_clid
            );
        }
    });
}

/// Processes `streamsignaling` messages addressed to an active inbound or outbound TS6 WebRTC bridge.
/// Returns `true` if this signaling message belongs to a bridged stream and was handled by the server.
pub fn handle_ts6_bridge_signaling(
    stream_id: &str,
    sender_clid: u64,
    target_clid: u64,
    json_payload: &str,
    runtime: &Arc<Mutex<BaselineRuntime>>,
) -> bool {
    let is_inbound_target = is_virtual_bridge_viewer_clid(target_clid);
    let is_bridge_stream = stream_id.starts_with("ts6-bridge-") || is_inbound_target || {
        let reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
        reg.inbound_streams.contains_key(stream_id)
            || reg
                .outbound_viewers
                .contains_key(&(stream_id.to_string(), sender_clid))
    };

    if !is_bridge_stream {
        return false;
    }

    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_payload) else {
        return true;
    };
    let cmd = parsed
        .get("cmd")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let args = parsed.get("args").cloned().unwrap_or(serde_json::Value::Null);

    let stream_id_owned = stream_id.to_string();
    let rt_clone = Arc::clone(runtime);

    tokio::spawn(async move {
        if cmd == "answer" {
            let Some(answer_sdp) = args.get("answer").and_then(|v| v.as_str()) else {
                return;
            };
            let mut found = None;
            for _ in 0..25 {
                {
                    let reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
                    if let Some(ob) = reg
                        .outbound_viewers
                        .get(&(stream_id_owned.clone(), sender_clid))
                    {
                        found = Some(ob.clone());
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if let Some(ob) = found
                && let Ok(desc) = RTCSessionDescription::answer(answer_sdp.to_string())
                && ob.pc.set_remote_description(desc).await.is_ok()
            {
                let rtc_mgr = {
                    let rt = rt_clone.lock().unwrap();
                    Arc::clone(&rt.rtc_manager)
                };
                rtc_mgr.request_keyframe(ob.broadcaster_clid, ob.bt).await;
                println!(
                    "Applied TS6 viewer WebRTC SDP answer for stream_id={} viewer={}",
                    stream_id_owned, sender_clid
                );
            }
        } else if cmd == "offer" || cmd == "reconnectOffer" {
            let Some(offer_sdp) = args.get("offer").and_then(|v| v.as_str()) else {
                return;
            };
            let mut found = None;
            for _ in 0..25 {
                {
                    let reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
                    if let Some(ib) = reg.inbound_streams.get(&stream_id_owned) {
                        found = Some(ib.clone());
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if let Some(ib) = found
                && let Ok(offer) = RTCSessionDescription::offer(offer_sdp.to_string())
                && ib.pc.set_remote_description(offer).await.is_ok()
                && let Ok(answer) = ib.pc.create_answer(None).await
                && ib.pc.set_local_description(answer).await.is_ok()
                && let Some(local_sdp) = ib.pc.local_description().await
            {
                let answer_json = serde_json::json!({
                    "cmd": "answer",
                    "args": {
                        "answer": local_sdp.sdp,
                    }
                })
                .to_string();
                let sig_cmd = format!(
                    "notifystreamsignaling clid={} id={} json={}",
                    ib.virtual_viewer_clid,
                    ts6_escape(&ib.stream_id),
                    ts6_escape(&answer_json)
                );
                send_ts6_command_with_packet_id(
                    &sig_cmd,
                    &ib.server_packet_id,
                    ib.addr,
                    &ib.socket,
                    &ib.iv_struct,
                )
                .await;
            }
        } else if cmd == "iceCandidate" {
            let cand_str = args
                .get("sdp")
                .or_else(|| args.get("candidate"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if cand_str.is_empty() {
                return;
            }
            let sdp_mid = args
                .get("mid")
                .or_else(|| args.get("sdpMid"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let sdp_mline_index = args
                .get("mLine")
                .or_else(|| args.get("sdpMLineIndex"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u16);

            let mut target_pc: Option<Arc<RTCPeerConnection>> = None;
            for _ in 0..50 {
                let pc_opt = {
                    let reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
                    if is_inbound_target {
                        reg.inbound_streams
                            .get(&stream_id_owned)
                            .map(|ib| Arc::clone(&ib.pc))
                    } else {
                        reg.outbound_viewers
                            .get(&(stream_id_owned.clone(), sender_clid))
                            .map(|ob| Arc::clone(&ob.pc))
                            .or_else(|| {
                                reg.inbound_streams
                                    .get(&stream_id_owned)
                                    .map(|ib| Arc::clone(&ib.pc))
                            })
                    }
                };
                if let Some(pc) = pc_opt {
                    if pc.remote_description().await.is_some() {
                        target_pc = Some(pc);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }

            if let Some(pc) = target_pc {
                let _ = pc
                    .add_ice_candidate(RTCIceCandidateInit {
                        candidate: cand_str,
                        sdp_mid,
                        sdp_mline_index,
                        ..Default::default()
                    })
                    .await;
            }
        }
    });

    true
}

pub fn close_ts6_outbound_viewer(
    stream_id: &str,
    viewer_clid: u64,
    runtime: &Arc<Mutex<BaselineRuntime>>,
) {
    let removed = {
        let mut reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
        reg.outbound_viewers
            .remove(&(stream_id.to_string(), viewer_clid))
    };
    if let Some(ob) = removed {
        let rtc_mgr = {
            let rt = runtime.lock().unwrap();
            Arc::clone(&rt.rtc_manager)
        };
        rtc_mgr.remove_ts6_viewer_slot(viewer_clid, Some(ob.broadcaster_clid), Some(ob.bt));
        tokio::spawn(async move {
            let _ = ob.pc.close().await;
        });
    }
}

pub fn close_all_ts6_bridges_for_stream(
    stream_id: &str,
    runtime: &Arc<Mutex<BaselineRuntime>>,
) {
    let (inbound, outbound_list) = {
        let mut reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
        let ib = reg.inbound_streams.remove(stream_id);
        let keys: Vec<(String, u64)> = reg
            .outbound_viewers
            .keys()
            .filter(|(sid, _)| sid == stream_id)
            .cloned()
            .collect();
        let mut obs = Vec::new();
        for k in keys {
            if let Some(ob) = reg.outbound_viewers.remove(&k) {
                obs.push(ob);
            }
        }
        (ib, obs)
    };

    if inbound.is_none() && outbound_list.is_empty() {
        return;
    }

    let rtc_mgr = {
        let rt = runtime.lock().unwrap();
        Arc::clone(&rt.rtc_manager)
    };

    if let Some(ib) = inbound {
        rtc_mgr.remove_stream_connection(ib.streamer_clid, ib.bt);
        tokio::spawn(async move {
            let _ = ib.pc.close().await;
        });
    }

    for ob in outbound_list {
        rtc_mgr.remove_ts6_viewer_slot(ob.viewer_clid, Some(ob.broadcaster_clid), Some(ob.bt));
        tokio::spawn(async move {
            let _ = ob.pc.close().await;
        });
    }
}

pub fn close_all_ts6_bridges_for_client(clid: u64, runtime: &Arc<Mutex<BaselineRuntime>>) {
    let (inbounds, outbounds) = {
        let mut reg = TS6_WEBRTC_BRIDGE.lock().unwrap();
        let ib_keys: Vec<String> = reg
            .inbound_streams
            .iter()
            .filter_map(|(k, v)| (v.streamer_clid == clid).then(|| k.clone()))
            .collect();
        let mut ibs = Vec::new();
        for k in ib_keys {
            if let Some(ib) = reg.inbound_streams.remove(&k) {
                ibs.push(ib);
            }
        }

        let ob_keys: Vec<(String, u64)> = reg
            .outbound_viewers
            .iter()
            .filter_map(|(k, v)| {
                (v.viewer_clid == clid || v.broadcaster_clid == clid).then(|| k.clone())
            })
            .collect();
        let mut obs = Vec::new();
        for k in ob_keys {
            if let Some(ob) = reg.outbound_viewers.remove(&k) {
                obs.push(ob);
            }
        }
        (ibs, obs)
    };

    let rtc_mgr = {
        let rt = runtime.lock().unwrap();
        Arc::clone(&rt.rtc_manager)
    };
    rtc_mgr.remove_ts6_viewer_slot(clid, None, None);

    for ib in inbounds {
        rtc_mgr.remove_stream_connection(ib.streamer_clid, ib.bt);
        tokio::spawn(async move {
            let _ = ib.pc.close().await;
        });
    }

    for ob in outbounds {
        rtc_mgr.remove_ts6_viewer_slot(ob.viewer_clid, Some(ob.broadcaster_clid), Some(ob.bt));
        tokio::spawn(async move {
            let _ = ob.pc.close().await;
        });
    }
}
