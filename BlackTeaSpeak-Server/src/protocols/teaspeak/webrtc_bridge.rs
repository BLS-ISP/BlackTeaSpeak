use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use webrtc::api::media_engine::{MIME_TYPE_OPUS, MIME_TYPE_VP8, MediaEngine};
use webrtc::api::APIBuilder;
use webrtc::dtls_transport::dtls_role::DTLSRole;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::rtp_transceiver::RTCPFeedback;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};

use super::rtc_manager::{RtcBroadcastManager, ViewerVideoSlot, Vp8Depacketizer};
use crate::runtime::BaselineRuntime;
use crate::teaspeak_transport::UdpSession;

pub fn ts3_escape(s: &str) -> String {
    s.replace("\\", "\\\\")
        .replace(" ", "\\s")
        .replace("/", "\\/")
        .replace("|", "\\p")
        .replace("\x07", "\\a")
        .replace("\x08", "\\b")
        .replace("\x0c", "\\f")
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
        .replace("\x0b", "\\v")
}

pub fn ts3_unescape(s: &str) -> String {
    s.replace("\\\\", "\\")
        .replace("\\s", " ")
        .replace("\\/", "/")
        .replace("\\p", "|")
        .replace("\\a", "\x07")
        .replace("\\b", "\x08")
        .replace("\\f", "\x0c")
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace("\\v", "\x0b")
}

fn extract_offer_video_ssrcs(sdp: &str) -> Vec<u32> {
    let mut ssrcs = Vec::new();
    let mut in_video = false;
    let mut current_video_ssrc: Option<u32> = None;

    for raw_line in sdp.lines() {
        let line = raw_line.trim();
        if line.starts_with("m=") {
            if in_video {
                ssrcs.push(current_video_ssrc.unwrap_or(0));
            }
            in_video = line.starts_with("m=video");
            current_video_ssrc = None;
        } else if in_video && current_video_ssrc.is_none() && line.starts_with("a=ssrc:") {
            let rest = &line["a=ssrc:".len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(parsed) = digits.parse::<u32>() {
                current_video_ssrc = Some(parsed);
            }
        }
    }
    if in_video {
        ssrcs.push(current_video_ssrc.unwrap_or(0));
    }
    ssrcs
}

fn patch_sdp_and_sync_slots(
    sdp: &str,
    slots: &mut [ViewerVideoSlot],
    force_passive: bool,
) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    let mut in_video = false;
    let mut video_idx: usize = 0;
    let mut current_section_has_ssrc = false;

    for raw_line in sdp.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.starts_with("m=") {
            if in_video && !current_section_has_ssrc {
                if let Some(slot) = slots.get(video_idx.saturating_sub(1)) {
                    out_lines.push(format!("a=ssrc:{} cname:blackteaspeak-video", slot.ssrc));
                }
            }
            if line.starts_with("m=video") {
                in_video = true;
                current_section_has_ssrc = false;
                video_idx += 1;
            } else {
                in_video = false;
            }
            out_lines.push(line.to_string());
            continue;
        }

        if force_passive && line == "a=setup:active" {
            out_lines.push("a=setup:passive".to_string());
            continue;
        }

        if in_video {
            if line == "a=recvonly" || line == "a=sendonly" || line == "a=inactive" {
                out_lines.push("a=sendrecv".to_string());
                continue;
            }
            if line.starts_with("a=ssrc:") {
                let rest = &line["a=ssrc:".len()..];
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(parsed_ssrc) = digits.parse::<u32>() {
                    if let Some(slot) = slots.get_mut(video_idx.saturating_sub(1)) {
                        slot.ssrc = parsed_ssrc;
                    }
                    current_section_has_ssrc = true;
                }
            }
        }

        out_lines.push(line.to_string());
    }

    if in_video && !current_section_has_ssrc {
        if let Some(slot) = slots.get(video_idx.saturating_sub(1)) {
            out_lines.push(format!("a=ssrc:{} cname:blackteaspeak-video", slot.ssrc));
        }
    }

    let mut joined = out_lines.join("\r\n");
    joined.push_str("\r\n");
    joined
}

fn parse_arg_u64(payload_str: &str, key: &str) -> Option<u64> {
    let prefix = format!("{}=", key);
    for token in payload_str.split(|c: char| c.is_whitespace() || c == '|') {
        if let Some(rest) = token.strip_prefix(&prefix) {
            let clean = rest.trim_matches('\0');
            if let Ok(v) = clean.parse::<u64>() {
                return Some(v);
            }
            if let Some(dot) = clean.find('.') {
                if let Ok(v) = clean[..dot].parse::<u64>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

pub fn handle_incoming_webrtc_accumulate(
    payload_str: &str,
    decrypted: &[u8],
    session: &mut UdpSession,
) {
    if payload_str.contains("rtcsessiondescribe") {
        session.pending_rtc_describe = true;
        session.pending_rtc_is_video =
            payload_str.contains("m=video") || payload_str.contains("video");
        session.rtc_describe_buffer.clear();
        session.rtc_describe_buffer.extend_from_slice(decrypted);
    } else if session.pending_rtc_describe {
        session.rtc_describe_buffer.extend_from_slice(decrypted);
    }
}

pub async fn handle_webrtc_action(
    payload_str: &str,
    session: &mut UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    rtc_manager: &Arc<RtcBroadcastManager>,
    iv_struct: &[u8],
    runtime: &Arc<Mutex<BaselineRuntime>>,
    server_id: u32,
) {
    let is_join = payload_str.starts_with("broadcastvideojoin");
    let is_leave =
        payload_str.starts_with("broadcastleave") || payload_str.starts_with("broadcastvideoleave");
    let is_broadcast_video = payload_str.starts_with("broadcastvideo ")
        || payload_str.starts_with("broadcastvideo\t")
        || payload_str == "broadcastvideo";
    let is_candidate = payload_str.contains("rtcicecandidate");
    let is_rtc = session.pending_rtc_describe;

    if is_rtc {
        session.pending_rtc_describe = false;
        let decompressed_buffer =
            crate::teaspeak_transport::decompress_if_needed(&session.rtc_describe_buffer);
        let full_sdp_offer = String::from_utf8_lossy(&decompressed_buffer).to_string();

        let rc = super::wire_formatting::get_teaspeak_return_code(&full_sdp_offer);
        let is_answer_mode = full_sdp_offer.contains("mode=answer");

        if let Some(sdp_idx) = full_sdp_offer.find("sdp=") {
            let sdp_substring = &full_sdp_offer[sdp_idx + 4..];
            let mut end_idx = 0;
            while end_idx < sdp_substring.len() {
                let c = sdp_substring.as_bytes()[end_idx];
                if c == b' ' {
                    break;
                }
                end_idx += 1;
            }
            let raw_sdp_val = &sdp_substring[..end_idx];
            let unescaped_sdp = ts3_unescape(raw_sdp_val);
            let client_id = session.client_id;

            if is_answer_mode {
                let rtc_manager = Arc::clone(rtc_manager);
                let socket_clone = Arc::clone(socket);
                let iv_struct_clone = iv_struct.to_vec();
                let addr_clone = addr;
                let server_packet_id_mutex = Arc::clone(&session.server_packet_id);
                tokio::spawn(async move {
                    if let Some(pc) = rtc_manager
                        .get_connection(addr_clone)
                        .or_else(|| rtc_manager.get_client_connection(client_id))
                    {
                        if let Ok(answer_desc) = RTCSessionDescription::answer(unescaped_sdp) {
                            let _ = pc.set_remote_description(answer_desc).await;
                        }
                    }
                    let ack_cmd = if let Some(rc_val) = &rc {
                        format!("error id=0 msg=ok return_code={}", rc_val)
                    } else {
                        "error id=0 msg=ok".to_string()
                    };
                    super::wire_formatting::send_teaspeak_command_with_packet_id(
                        &ack_cmd,
                        &server_packet_id_mutex,
                        addr_clone,
                        &socket_clone,
                        &iv_struct_clone,
                    )
                    .await;
                });
                session.rtc_describe_buffer.clear();
                return;
            }

            let offer_ssrcs = extract_offer_video_ssrcs(&unescaped_sdp);
            for (idx, &ssrc) in offer_ssrcs.iter().enumerate() {
                if ssrc != 0 {
                    rtc_manager.register_incoming_ssrc_type(client_id, ssrc, (idx as u32).min(1));
                }
            }
            let num_video_sections = unescaped_sdp
                .lines()
                .filter(|l| l.trim_start().starts_with("m=video"))
                .count()
                .max(2);

            let rtc_manager = Arc::clone(rtc_manager);
            let runtime_clone = Arc::clone(runtime);
            let socket_clone = Arc::clone(socket);
            let iv_struct_clone = iv_struct.to_vec();
            let addr_clone = addr;
            let server_packet_id_mutex = Arc::clone(&session.server_packet_id);

            tokio::spawn(async move {
                let mut m = MediaEngine::default();
                let _ = m.register_codec(
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_OPUS.to_owned(),
                            clock_rate: 48000,
                            channels: 2,
                            sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                            rtcp_feedback: vec![],
                        },
                        payload_type: 111,
                        ..Default::default()
                    },
                    RTPCodecType::Audio,
                );
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
                for pt in [120_u8, 96_u8] {
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

                let mut se = webrtc::api::setting_engine::SettingEngine::default();
                let _ = se.set_answering_dtls_role(DTLSRole::Server);

                let api = APIBuilder::new()
                    .with_media_engine(m)
                    .with_setting_engine(se)
                    .build();

                let config = RTCConfiguration {
                    ..Default::default()
                };

                if let Ok(pc) = api.new_peer_connection(config).await {
                    let pc = Arc::new(pc);

                    // Add local VP8 sender tracks BEFORE set_remote_description so the SDP Answer
                    // negotiates sendrecv video transceivers with valid outbound SSRCs for viewing.
                    let mut pending_slots = Vec::new();
                    for slot_idx in 0..num_video_sections {
                        let track = Arc::new(TrackLocalStaticRTP::new(
                            RTCRtpCodecCapability {
                                mime_type: "video/VP8".to_string(),
                                clock_rate: 90000,
                                ..Default::default()
                            },
                            format!("video-{}-{}", client_id, slot_idx),
                            format!("stream-{}-{}", client_id, slot_idx),
                        ));
                        if let Ok(rtp_sender) = pc
                            .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
                            .await
                        {
                            let sender_clone = Arc::clone(&rtp_sender);
                            tokio::spawn(async move {
                                let mut rtcp_buf = vec![0u8; 1500];
                                while sender_clone.read(&mut rtcp_buf).await.is_ok() {}
                            });
                            pending_slots.push((slot_idx, track, rtp_sender));
                        }
                    }

                    let addr_ice_state = addr_clone;
                    pc.on_ice_connection_state_change(Box::new(move |state| {
                        println!(
                            "WebRTC ICE Connection State Changed for {} (clid={}): {:?}",
                            addr_ice_state, client_id, state
                        );
                        Box::pin(async move {})
                    }));

                    let addr_pc_state = addr_clone;
                    let pc_for_reoffer = Arc::clone(&pc);
                    let rtc_mgr_for_reoffer = Arc::clone(&rtc_manager);
                    let socket_for_reoffer = Arc::clone(&socket_clone);
                    let iv_for_reoffer = iv_struct_clone.clone();
                    let pid_for_reoffer = Arc::clone(&server_packet_id_mutex);
                    let reoffered_extra_slots = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    pc.on_peer_connection_state_change(Box::new(move |state| {
                        println!(
                            "WebRTC Peer Connection State Changed for {} (clid={}): {:?}",
                            addr_pc_state, client_id, state
                        );
                        if state == webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Connected
                            && !reoffered_extra_slots.swap(true, Ordering::SeqCst)
                        {
                            let pc_re = Arc::clone(&pc_for_reoffer);
                            let rtc_mgr_re = Arc::clone(&rtc_mgr_for_reoffer);
                            let sock_re = Arc::clone(&socket_for_reoffer);
                            let iv_re = iv_for_reoffer.clone();
                            let pid_re = Arc::clone(&pid_for_reoffer);
                            tokio::spawn(async move {
                                const TARGET_TOTAL_SLOTS: usize = 8;
                                let existing_slots = rtc_mgr_re
                                    .viewer_slots
                                    .read()
                                    .unwrap()
                                    .get(&client_id)
                                    .cloned()
                                    .unwrap_or_default();
                                if existing_slots.len() >= TARGET_TOTAL_SLOTS {
                                    return;
                                }
                                let mut new_pending = Vec::new();
                                for slot_idx in existing_slots.len()..TARGET_TOTAL_SLOTS {
                                    let track = Arc::new(TrackLocalStaticRTP::new(
                                        RTCRtpCodecCapability {
                                            mime_type: "video/VP8".to_string(),
                                            clock_rate: 90000,
                                            ..Default::default()
                                        },
                                        format!("video-{}-{}", client_id, slot_idx),
                                        format!("stream-{}-{}", client_id, slot_idx),
                                    ));
                                    if let Ok(rtp_sender) = pc_re
                                        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
                                        .await
                                    {
                                        let sender_clone = Arc::clone(&rtp_sender);
                                        tokio::spawn(async move {
                                            let mut rtcp_buf = vec![0u8; 1500];
                                            while sender_clone.read(&mut rtcp_buf).await.is_ok() {}
                                        });
                                        new_pending.push((slot_idx, track, rtp_sender));
                                    }
                                }
                                if new_pending.is_empty() {
                                    return;
                                }
                                if let Ok(reoffer) = pc_re.create_offer(None).await {
                                    if pc_re.set_local_description(reoffer).await.is_ok() {
                                        let mut all_slots = existing_slots;
                                        for (slot_idx, track, rtp_sender) in new_pending {
                                            let params = rtp_sender.get_parameters().await;
                                            let ssrc = params
                                                .encodings
                                                .first()
                                                .map(|e| e.ssrc)
                                                .filter(|&s| s != 0)
                                                .unwrap_or(
                                                    0x5000_0000
                                                        + (client_id as u32) * 16
                                                        + (slot_idx as u32),
                                                );
                                            all_slots.push(ViewerVideoSlot {
                                                slot_index: slot_idx,
                                                ssrc,
                                                track,
                                                seq_num: Arc::new(AtomicU16::new(1)),
                                                assigned_broadcast: None,
                                            });
                                        }
                                        if let Some(local_sdp) = pc_re.local_description().await {
                                            let sdp_str = patch_sdp_and_sync_slots(
                                                &local_sdp.sdp,
                                                &mut all_slots,
                                                false,
                                            );
                                            rtc_mgr_re.set_viewer_slots(client_id, all_slots);
                                            let escaped_offer_sdp = ts3_escape(&sdp_str);
                                            let notify_cmd = format!(
                                                "notifyrtcsessiondescription mode=offer sdp={} compression=0",
                                                escaped_offer_sdp
                                            );
                                            super::wire_formatting::send_teaspeak_command_with_packet_id(
                                                &notify_cmd,
                                                &pid_re,
                                                addr_pc_state,
                                                &sock_re,
                                                &iv_re,
                                            )
                                            .await;
                                        }
                                    }
                                }
                            });
                        }
                        Box::pin(async move {})
                    }));

                    let rtc_mgr = Arc::clone(&rtc_manager);
                    let rt_for_track = Arc::clone(&runtime_clone);
                    let pc_for_track = Arc::clone(&pc);
                    pc.on_track(Box::new(move |track, _receiver, _transceiver| {
                        let rtc_mgr = Arc::clone(&rtc_mgr);
                        let rt_for_track = Arc::clone(&rt_for_track);
                        let pc_for_track = Arc::clone(&pc_for_track);
                        let track = Arc::clone(&track);
                        // IMPORTANT: Spawn a background task and return immediately so webrtc-rs's
                        // internal on_track_handler Mutex is not held across the RTP read loop!
                        tokio::spawn(async move {
                            let mime = track.codec().capability.mime_type.to_ascii_lowercase();
                            if !mime.contains("video") && !mime.contains("vp8") {
                                return;
                            }
                            let track_ssrc = track.ssrc();
                            let initial_bt =
                                rtc_mgr.resolve_or_assign_incoming_bt(client_id, track_ssrc);
                            println!(
                                "WebRTC video track ready for clid={} bt={} ssrc={}",
                                client_id, initial_bt, track_ssrc
                            );

                            let mut depacketizer = Vp8Depacketizer::new();
                            let mut was_active = false;
                            let mut last_pli = std::time::Instant::now();

                            while let Ok((packet, _)) = track.read_rtp().await {
                                let bt = rtc_mgr
                                    .incoming_ssrc_types
                                    .read()
                                    .unwrap()
                                    .get(&(client_id, track_ssrc))
                                    .copied()
                                    .unwrap_or(initial_bt);

                                let is_active = {
                                    let rt = rt_for_track.lock().unwrap();
                                    rt.store
                                        .active_broadcasts
                                        .contains_key(&(server_id, client_id, bt))
                                };
                                if !is_active {
                                    was_active = false;
                                    continue;
                                }

                                if !was_active
                                    || last_pli.elapsed() >= std::time::Duration::from_secs(2)
                                {
                                    was_active = true;
                                    last_pli = std::time::Instant::now();
                                    let _ = pc_for_track
                                        .write_rtcp(&[Box::new(PictureLossIndication {
                                            sender_ssrc: 0,
                                            media_ssrc: track_ssrc,
                                        })])
                                        .await;
                                }

                                // 1. Forward RTP packet to any subscribed TeaSpeak viewers
                                let viewers = rtc_mgr.get_subscribed_viewer_tracks(client_id, bt);
                                for (_slot_ssrc, viewer_track, seq_num) in &viewers {
                                    let mut out_pkt = packet.clone();
                                    out_pkt.header.payload_type = 120;
                                    out_pkt.header.sequence_number =
                                        seq_num.fetch_add(1, Ordering::Relaxed);
                                    // Strip incoming RTP header extensions (e.g. mid:0 / mid:1)
                                    // so BUNDLE demuxing on the viewer uses the outbound slot's SSRC/MID!
                                    out_pkt.header.extension = false;
                                    out_pkt.header.extension_profile = 0;
                                    out_pkt.header.extensions.clear();
                                    out_pkt.header.extensions_padding = 0;
                                    let _ = viewer_track.write_rtp(&out_pkt).await;
                                }

                                // 2. Depacketize RFC 7741 VP8 into BTEA frame and route to BlackTeaWeb
                                if let Some(btea_frame) = depacketizer.push_rtp(
                                    packet.header.timestamp,
                                    packet.header.marker,
                                    &packet.payload,
                                ) {
                                    let wt_packet_type = if bt == 1 { 4 } else { 2 };
                                    let rt = rt_for_track.lock().unwrap();
                                    rt.route_btea_media_to_webtransport(
                                        server_id,
                                        client_id,
                                        wt_packet_type,
                                        &btea_frame,
                                    );
                                }
                            }

                            println!(
                                "WebRTC video track ended for clid={} bt={} ssrc={}",
                                client_id, initial_bt, track_ssrc
                            );
                        });
                        Box::pin(async {})
                    }));

                    if let Ok(offer) = RTCSessionDescription::offer(unescaped_sdp) {
                        if pc.set_remote_description(offer).await.is_ok() {
                            if let Ok(answer) = pc.create_answer(None).await {
                                let (ice_tx, mut ice_rx) = tokio::sync::mpsc::channel::<()>(1);
                                pc.on_ice_gathering_state_change(Box::new(move |state| {
                                    if state == RTCIceGathererState::Complete {
                                        let _ = ice_tx.try_send(());
                                    }
                                    Box::pin(async move {})
                                }));

                                if pc.set_local_description(answer).await.is_ok() {
                                    let _ = tokio::time::timeout(
                                        std::time::Duration::from_millis(200),
                                        ice_rx.recv(),
                                    )
                                    .await;

                                    let mut slots = Vec::with_capacity(pending_slots.len());
                                    for (slot_idx, track, rtp_sender) in pending_slots {
                                        let params = rtp_sender.get_parameters().await;
                                        let ssrc = params
                                            .encodings
                                            .first()
                                            .map(|e| e.ssrc)
                                            .filter(|&s| s != 0)
                                            .unwrap_or(
                                                0x5000_0000
                                                    + (client_id as u32) * 16
                                                    + (slot_idx as u32),
                                            );
                                        slots.push(ViewerVideoSlot {
                                            slot_index: slot_idx,
                                            ssrc,
                                            track,
                                            seq_num: Arc::new(AtomicU16::new(1)),
                                            assigned_broadcast: None,
                                        });
                                    }

                                    if let Some(local_sdp) = pc.local_description().await {
                                        let sdp_str = patch_sdp_and_sync_slots(
                                            &local_sdp.sdp,
                                            &mut slots,
                                            true,
                                        );
                                        rtc_manager.set_viewer_slots(client_id, slots);
                                        rtc_manager.register_connection(
                                            addr_clone,
                                            client_id,
                                            Arc::clone(&pc),
                                        );

                                        let escaped_answer_sdp = ts3_escape(&sdp_str);
                                        let notify_cmd = format!(
                                            "notifyrtcsessiondescription mode=answer sdp={} compression=0",
                                            escaped_answer_sdp
                                        );

                                        super::wire_formatting::send_teaspeak_command_with_packet_id(
                                            &notify_cmd,
                                            &server_packet_id_mutex,
                                            addr_clone,
                                            &socket_clone,
                                            &iv_struct_clone,
                                        )
                                        .await;
                                    } else {
                                        rtc_manager.set_viewer_slots(client_id, slots);
                                        rtc_manager.register_connection(
                                            addr_clone,
                                            client_id,
                                            Arc::clone(&pc),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                let ack_cmd = if let Some(rc_val) = &rc {
                    format!("error id=0 msg=ok return_code={}", rc_val)
                } else {
                    "error id=0 msg=ok".to_string()
                };
                super::wire_formatting::send_teaspeak_command_with_packet_id(
                    &ack_cmd,
                    &server_packet_id_mutex,
                    addr_clone,
                    &socket_clone,
                    &iv_struct_clone,
                )
                .await;

                // Announce any already-active broadcasts in the TeaSpeak client's channel once WebRTC is ready
                let active_bcast_cmd = {
                    let rt = runtime_clone.lock().unwrap();
                    let client_cid = rt
                        .store
                        .online_clients
                        .get(&client_id)
                        .map(|c| c.channel_id)
                        .unwrap_or(1);
                    let parts: Vec<String> = rt
                        .store
                        .active_broadcasts
                        .values()
                        .filter(|b| {
                            b.server_id == server_id
                                && b.channel_id == client_cid
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
                        None
                    } else {
                        Some(format!("notifybroadcastvideo {}", parts.join("|")))
                    }
                };
                if let Some(bcast_cmd) = active_bcast_cmd {
                    super::wire_formatting::send_teaspeak_command_with_packet_id(
                        &bcast_cmd,
                        &server_packet_id_mutex,
                        addr_clone,
                        &socket_clone,
                        &iv_struct_clone,
                    )
                    .await;
                }
            });
        }
        session.rtc_describe_buffer.clear();
    }

    if is_candidate {
        let mut sdp_mline_index = None;
        if let Some(ml_idx) = payload_str.find("media_line=") {
            let ml_substring = &payload_str[ml_idx + 11..];
            let mut end_idx = 0;
            while end_idx < ml_substring.len() {
                let c = ml_substring.as_bytes()[end_idx];
                if c.is_ascii_digit() {
                    end_idx += 1;
                } else {
                    break;
                }
            }
            if let Ok(ml_val) = ml_substring[..end_idx].parse::<u16>() {
                sdp_mline_index = Some(ml_val);
            }
        }
        if let Some(cand_idx) = payload_str.find("candidate=") {
            let cand_substring = &payload_str[cand_idx + 10..];
            let mut end_idx = 0;
            while end_idx < cand_substring.len() {
                let c = cand_substring.as_bytes()[end_idx];
                if c == b' ' {
                    break;
                }
                end_idx += 1;
            }
            let raw_cand_val = &cand_substring[..end_idx];
            let unescaped_cand = ts3_unescape(raw_cand_val);

            let rtc_mgr = Arc::clone(rtc_manager);
            let addr_clone = addr;
            let cand_clone = unescaped_cand;
            tokio::spawn(async move {
                let mut found_pc = None;
                for _ in 0..15 {
                    if let Some(pc) = rtc_mgr.get_connection(addr_clone) {
                        found_pc = Some(pc);
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
                if let Some(pc) = found_pc {
                    let _ = pc
                        .add_ice_candidate(RTCIceCandidateInit {
                            candidate: cand_clone,
                            sdp_mline_index,
                            ..Default::default()
                        })
                        .await;
                }
            });
        }
    }

    if is_broadcast_video {
        let client_id = session.client_id;
        for bt in [0_u32, 1_u32] {
            let active_info = {
                let rt = runtime.lock().unwrap();
                rt.store
                    .active_broadcasts
                    .get(&(server_id, client_id, bt))
                    .map(|b| {
                        let (dbid, nick, uid) = rt
                            .store
                            .online_clients
                            .get(&client_id)
                            .map(|c| {
                                (
                                    c.database_id,
                                    c.nickname.clone(),
                                    c.unique_identifier.clone(),
                                )
                            })
                            .unwrap_or_else(|| {
                                (
                                    client_id + 1000,
                                    session.client_nickname.clone(),
                                    session.client_uid.clone(),
                                )
                            });
                        (b.ssrc, dbid, nick, uid)
                    })
            };
            if let Some((broadcaster_ssrc, dbid, nick, uid)) = active_info {
                if broadcaster_ssrc != 0 {
                    let escaped_nick = ts3_escape(&nick);
                    let escaped_uid = ts3_escape(&uid);
                    let joined_cmd = format!(
                        "notifystreamjoined streamid={} clid={} cluid={} cldbid={} clname={}",
                        broadcaster_ssrc, client_id, escaped_uid, dbid, escaped_nick
                    );
                    super::wire_formatting::send_teaspeak_command(
                        &joined_cmd,
                        session,
                        addr,
                        socket,
                        iv_struct,
                    )
                    .await;
                }
                rtc_manager.request_keyframe(client_id, bt).await;
            }
        }
    }

    if is_join {
        for segment in payload_str.split('|') {
            let requested_bid = parse_arg_u64(segment, "bid")
                .or_else(|| parse_arg_u64(segment, "clid"))
                .or_else(|| parse_arg_u64(segment, "scl"))
                .unwrap_or(0);
            let requested_bt = parse_arg_u64(segment, "bt")
                .or_else(|| parse_arg_u64(segment, "type"))
                .map(|v| v as u32);

            if requested_bid != 0 {
                let broadcast_info = {
                    let rt = runtime.lock().unwrap();
                    let target_bt = if let Some(bt) = requested_bt {
                        Some(bt)
                    } else {
                        [0_u32, 1_u32]
                            .into_iter()
                            .find(|&bt| {
                                rt.store
                                    .active_broadcasts
                                    .contains_key(&(server_id, requested_bid, bt))
                            })
                    };
                    target_bt.and_then(|bt| {
                        rt.store
                            .active_broadcasts
                            .get(&(server_id, requested_bid, bt))
                            .map(|b| {
                                let (dbid, nick, uid) = rt
                                    .store
                                    .online_clients
                                    .get(&b.broadcaster_client_id)
                                    .map(|c| {
                                        (
                                            c.database_id,
                                            c.nickname.clone(),
                                            c.unique_identifier.clone(),
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        (
                                            b.broadcaster_client_id + 1000,
                                            format!("Client {}", b.broadcaster_client_id),
                                            format!("uid-{}", b.broadcaster_client_id),
                                        )
                                    });
                                (
                                    b.broadcaster_client_id,
                                    b.broadcast_type,
                                    b.ssrc,
                                    dbid,
                                    nick,
                                    uid,
                                )
                            })
                    })
                };

                if let Some((broadcaster_id, bt, broadcaster_ssrc, dbid, nick, uid)) =
                    broadcast_info
                {
                    if let Some((stream_id, evicted_opt)) =
                        rtc_manager.assign_viewer_slot(session.client_id, broadcaster_id, bt)
                    {
                        if let Some((old_bid, _old_bt)) = evicted_opt {
                            let evict_state_cmd = format!(
                                "notifyrtcstreamstate streamid={} sclid={} state=0",
                                stream_id, old_bid
                            );
                            super::wire_formatting::send_teaspeak_command(
                                &evict_state_cmd,
                                session,
                                addr,
                                socket,
                                iv_struct,
                            )
                            .await;
                        }

                        // TeaSpeak media type: 2 = Camera ("video", bt=0), 3 = Screen ("video-screen", bt=1)
                        let media_type = if bt == 1 { 3 } else { 2 };
                        let escaped_nick = ts3_escape(&nick);
                        let escaped_uid = ts3_escape(&uid);

                        let assign_cmd = format!(
                            "notifyrtcstreamassignment streamid={} media={} sclid={} scldbid={} sclname={} scluid={}",
                            stream_id, media_type, broadcaster_id, dbid, escaped_nick, escaped_uid
                        );
                        super::wire_formatting::send_teaspeak_command(
                            &assign_cmd,
                            session,
                            addr,
                            socket,
                            iv_struct,
                        )
                        .await;

                        let state_cmd = format!(
                            "notifyrtcstreamstate streamid={} state=1 sclid={} scldbid={} sclname={} scluid={}",
                            stream_id, broadcaster_id, dbid, escaped_nick, escaped_uid
                        );
                        super::wire_formatting::send_teaspeak_command(
                            &state_cmd,
                            session,
                            addr,
                            socket,
                            iv_struct,
                        )
                        .await;

                        if broadcaster_id == session.client_id && broadcaster_ssrc != 0 {
                            let joined_cmd = format!(
                                "notifystreamjoined streamid={} clid={} cluid={} cldbid={} clname={}",
                                broadcaster_ssrc,
                                session.client_id,
                                escaped_uid,
                                dbid,
                                escaped_nick
                            );
                            super::wire_formatting::send_teaspeak_command(
                                &joined_cmd,
                                session,
                                addr,
                                socket,
                                iv_struct,
                            )
                            .await;
                        }

                        rtc_manager.request_keyframe(broadcaster_id, bt).await;
                    }
                }
            }
        }
    }

    if is_leave {
        for segment in payload_str.split('|') {
            let requested_bid = parse_arg_u64(segment, "bid")
                .or_else(|| parse_arg_u64(segment, "clid"))
                .or_else(|| parse_arg_u64(segment, "scl"));
            let requested_bt = parse_arg_u64(segment, "bt")
                .or_else(|| parse_arg_u64(segment, "type"))
                .map(|v| v as u32);
            let cleared_ssrcs =
                rtc_manager.unassign_viewer_slot(session.client_id, requested_bid, requested_bt);
            for stream_id in cleared_ssrcs {
                let state_cmd = format!("notifyrtcstreamstate streamid={} state=0", stream_id);
                super::wire_formatting::send_teaspeak_command(
                    &state_cmd, session, addr, socket, iv_struct,
                )
                .await;
                let assign_cmd = format!("notifyrtcstreamassignment streamid={} sclid=0", stream_id);
                super::wire_formatting::send_teaspeak_command(
                    &assign_cmd, session, addr, socket, iv_struct,
                )
                .await;
            }
        }
    }
}
