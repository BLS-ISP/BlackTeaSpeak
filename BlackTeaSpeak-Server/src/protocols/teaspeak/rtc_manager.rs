use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, RwLock};
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp::header::Header as RtpHeader;
use webrtc::rtp::packet::Packet as RtpPacket;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocalWriter;

#[derive(Clone)]
pub struct ViewerVideoSlot {
    pub slot_index: usize,
    pub ssrc: u32,
    pub track: Arc<TrackLocalStaticRTP>,
    pub seq_num: Arc<AtomicU16>,
    pub assigned_broadcast: Option<(u64, u32)>,
}

pub struct RtcBroadcastManager {
    pub peer_connections: RwLock<HashMap<SocketAddr, Arc<RTCPeerConnection>>>,
    pub client_peer_connections: RwLock<HashMap<u64, Arc<RTCPeerConnection>>>,
    pub viewer_slots: RwLock<HashMap<u64, Vec<ViewerVideoSlot>>>,
    pub incoming_ssrc_types: RwLock<HashMap<(u64, u32), u32>>,
    pub incoming_track_ssrcs: RwLock<HashMap<(u64, u32), u32>>,
    pub web_rtp_clock: RwLock<HashMap<(u64, u32), (f64, u32)>>,
    pub web_stream_locks: std::sync::Mutex<HashMap<(u64, u32), Arc<tokio::sync::Mutex<(f64, u32)>>>>,
}

impl Default for RtcBroadcastManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RtcBroadcastManager {
    pub fn new() -> Self {
        Self {
            peer_connections: RwLock::new(HashMap::new()),
            client_peer_connections: RwLock::new(HashMap::new()),
            viewer_slots: RwLock::new(HashMap::new()),
            incoming_ssrc_types: RwLock::new(HashMap::new()),
            incoming_track_ssrcs: RwLock::new(HashMap::new()),
            web_rtp_clock: RwLock::new(HashMap::new()),
            web_stream_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn register_connection(
        &self,
        addr: SocketAddr,
        client_id: u64,
        pc: Arc<RTCPeerConnection>,
    ) {
        self.peer_connections
            .write()
            .unwrap()
            .insert(addr, Arc::clone(&pc));
        self.client_peer_connections
            .write()
            .unwrap()
            .insert(client_id, pc);
    }

    pub fn remove_connection(&self, addr: SocketAddr) {
        self.peer_connections.write().unwrap().remove(&addr);
    }

    pub fn get_connection(&self, addr: SocketAddr) -> Option<Arc<RTCPeerConnection>> {
        self.peer_connections.read().unwrap().get(&addr).cloned()
    }

    pub fn get_client_connection(&self, client_id: u64) -> Option<Arc<RTCPeerConnection>> {
        self.client_peer_connections
            .read()
            .unwrap()
            .get(&client_id)
            .cloned()
    }

    pub fn set_viewer_slots(&self, client_id: u64, mut slots: Vec<ViewerVideoSlot>) {
        let mut lock = self.viewer_slots.write().unwrap();
        if let Some(old_slots) = lock.get(&client_id) {
            for new_slot in &mut slots {
                if let Some(old_slot) = old_slots
                    .iter()
                    .find(|s| s.slot_index == new_slot.slot_index)
                {
                    if new_slot.assigned_broadcast.is_none() {
                        new_slot.assigned_broadcast = old_slot.assigned_broadcast;
                    }
                    new_slot.seq_num = Arc::clone(&old_slot.seq_num);
                }
            }
        }
        lock.insert(client_id, slots);
    }

    pub fn register_incoming_ssrc_type(&self, client_id: u64, ssrc: u32, bt: u32) {
        if ssrc == 0 {
            return;
        }
        let other_bt = if bt == 1 { 0 } else { 1 };
        let mut ssrc_types = self.incoming_ssrc_types.write().unwrap();
        let mut track_ssrcs = self.incoming_track_ssrcs.write().unwrap();

        // Find any other SSRC of this client that was previously mapped to `bt`
        // and swap it to `other_bt` so Camera (0) and Screen (1) never collide on the same `bt`.
        let displaced_ssrcs: Vec<u32> = ssrc_types
            .iter()
            .filter_map(|(&(cid, existing_ssrc), &existing_bt)| {
                if cid == client_id && existing_ssrc != ssrc && existing_bt == bt {
                    Some(existing_ssrc)
                } else {
                    None
                }
            })
            .collect();

        for other_ssrc in displaced_ssrcs {
            ssrc_types.insert((client_id, other_ssrc), other_bt);
            track_ssrcs.insert((client_id, other_bt), other_ssrc);
        }

        if track_ssrcs.get(&(client_id, other_bt)) == Some(&ssrc) {
            track_ssrcs.remove(&(client_id, other_bt));
        }

        ssrc_types.insert((client_id, ssrc), bt);
        track_ssrcs.insert((client_id, bt), ssrc);
    }

    pub fn resolve_or_assign_incoming_bt(&self, client_id: u64, ssrc: u32) -> u32 {
        if let Some(&bt) = self
            .incoming_ssrc_types
            .read()
            .unwrap()
            .get(&(client_id, ssrc))
        {
            self.incoming_track_ssrcs
                .write()
                .unwrap()
                .insert((client_id, bt), ssrc);
            return bt;
        }
        let mut track_lock = self.incoming_track_ssrcs.write().unwrap();
        let bt = if !track_lock.contains_key(&(client_id, 0)) {
            0
        } else if !track_lock.contains_key(&(client_id, 1)) {
            1
        } else {
            0
        };
        track_lock.insert((client_id, bt), ssrc);
        drop(track_lock);
        self.incoming_ssrc_types
            .write()
            .unwrap()
            .insert((client_id, ssrc), bt);
        bt
    }

    pub fn get_incoming_track_ssrc(&self, client_id: u64, bt: u32) -> Option<u32> {
        self.incoming_track_ssrcs
            .read()
            .unwrap()
            .get(&(client_id, bt))
            .copied()
    }

    pub fn assign_viewer_slot(
        &self,
        viewer_client_id: u64,
        broadcaster_client_id: u64,
        bt: u32,
    ) -> Option<(u32, Option<(u64, u32)>)> {
        let mut lock = self.viewer_slots.write().unwrap();
        let slots = lock.get_mut(&viewer_client_id)?;
        if slots.is_empty() {
            return None;
        }

        // 1. Reuse existing slot if already assigned to this exact (broadcaster, bt)
        if let Some(existing) = slots
            .iter()
            .find(|s| s.assigned_broadcast == Some((broadcaster_client_id, bt)))
        {
            return Some((existing.ssrc, None));
        }

        let is_self_preview = broadcaster_client_id == viewer_client_id;

        // 2. For self-preview, prefer higher slots (index >= 2) if available so slots 0..1 stay free for remote streams
        if is_self_preview && slots.len() > 2 {
            if let Some(high_slot) = slots
                .iter_mut()
                .skip(2)
                .find(|s| s.assigned_broadcast.is_none())
            {
                high_slot.assigned_broadcast = Some((broadcaster_client_id, bt));
                return Some((high_slot.ssrc, None));
            }
        }

        // 3. Prefer slot matching bt (0 = camera -> slot 0, 1 = screen -> slot 1) if free
        let preferred_idx = (bt as usize).min(slots.len() - 1);
        if slots[preferred_idx].assigned_broadcast.is_none() {
            slots[preferred_idx].assigned_broadcast = Some((broadcaster_client_id, bt));
            return Some((slots[preferred_idx].ssrc, None));
        }

        // 4. Otherwise pick any free slot
        if let Some(free_slot) = slots.iter_mut().find(|s| s.assigned_broadcast.is_none()) {
            free_slot.assigned_broadcast = Some((broadcaster_client_id, bt));
            return Some((free_slot.ssrc, None));
        }

        // 5. All slots are occupied:
        //    - A self-preview must NEVER evict a remote broadcaster's stream!
        if is_self_preview {
            if let Some(self_idx) = slots.iter().position(|s| {
                s.assigned_broadcast
                    .is_some_and(|(bid, _)| bid == viewer_client_id)
            }) {
                let evicted = slots[self_idx].assigned_broadcast;
                slots[self_idx].assigned_broadcast = Some((broadcaster_client_id, bt));
                return Some((slots[self_idx].ssrc, evicted));
            }
            return None;
        }

        //    - When subscribing to a remote broadcaster, prefer evicting a self-preview slot first,
        //      otherwise avoid evicting the same broadcaster's other stream (1 - bt).
        let evict_idx = slots
            .iter()
            .position(|s| {
                s.assigned_broadcast
                    .is_some_and(|(bid, _)| bid == viewer_client_id)
            })
            .or_else(|| {
                slots.iter().position(|s| {
                    s.assigned_broadcast
                        .is_some_and(|(bid, _)| bid != broadcaster_client_id)
                })
            })
            .unwrap_or(preferred_idx);

        let evicted = slots[evict_idx].assigned_broadcast;
        slots[evict_idx].assigned_broadcast = Some((broadcaster_client_id, bt));
        Some((slots[evict_idx].ssrc, evicted))
    }

    pub fn unassign_viewer_slot(
        &self,
        viewer_client_id: u64,
        broadcaster_client_id: Option<u64>,
        bt: Option<u32>,
    ) -> Vec<u32> {
        let mut cleared = Vec::new();
        let mut lock = self.viewer_slots.write().unwrap();
        if let Some(slots) = lock.get_mut(&viewer_client_id) {
            for slot in slots.iter_mut() {
                if let Some((bid, slot_bt)) = slot.assigned_broadcast {
                    if broadcaster_client_id.is_none_or(|target_bid| target_bid == bid)
                        && bt.is_none_or(|target_bt| target_bt == slot_bt)
                    {
                        slot.assigned_broadcast = None;
                        cleared.push(slot.ssrc);
                    }
                }
            }
        }
        cleared
    }

    pub fn unassign_broadcaster(&self, broadcaster_client_id: u64) {
        let mut lock = self.viewer_slots.write().unwrap();
        for slots in lock.values_mut() {
            for slot in slots.iter_mut() {
                if slot
                    .assigned_broadcast
                    .is_some_and(|(bid, _)| bid == broadcaster_client_id)
                {
                    slot.assigned_broadcast = None;
                }
            }
        }
    }

    pub fn get_subscribed_viewer_tracks(
        &self,
        broadcaster_client_id: u64,
        bt: u32,
    ) -> Vec<(u32, Arc<TrackLocalStaticRTP>, Arc<AtomicU16>)> {
        let lock = self.viewer_slots.read().unwrap();
        let mut out = Vec::new();
        for slots in lock.values() {
            for slot in slots {
                if slot.assigned_broadcast == Some((broadcaster_client_id, bt)) {
                    out.push((
                        slot.ssrc,
                        Arc::clone(&slot.track),
                        Arc::clone(&slot.seq_num),
                    ));
                }
            }
        }
        out
    }

    pub async fn request_keyframe(&self, broadcaster_client_id: u64, bt: u32) {
        let pc_opt = self.get_client_connection(broadcaster_client_id);
        let ssrc_opt = self.get_incoming_track_ssrc(broadcaster_client_id, bt);
        if let (Some(pc), Some(media_ssrc)) = (pc_opt, ssrc_opt) {
            let _ = pc
                .write_rtcp(&[Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc,
                })])
                .await;
        }
    }

    pub async fn forward_web_vp8_frame(
        &self,
        broadcaster_client_id: u64,
        bt: u32,
        btea_payload: &[u8],
    ) {
        if btea_payload.len() <= 13 {
            return;
        }
        let viewers = self.get_subscribed_viewer_tracks(broadcaster_client_id, bt);
        if viewers.is_empty() {
            return;
        }

        let is_keyframe = (btea_payload[0] & 0x01) != 0;
        let ts_us = f64::from_le_bytes(btea_payload[5..13].try_into().unwrap_or([0u8; 8]));
        let raw_vp8 = &btea_payload[13..];
        if raw_vp8.is_empty() {
            return;
        }

        let stream_mutex = {
            let mut map = self.web_stream_locks.lock().unwrap();
            Arc::clone(
                map.entry((broadcaster_client_id, bt))
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new((0.0, 10_000)))),
            )
        };

        let mut state = stream_mutex.lock().await;
        if !is_keyframe && state.0 > 0.0 && ts_us <= state.0 {
            // Drop out-of-order delta frame so we never corrupt the VP8 reference frame
            return;
        }

        let delta_us = if state.0 > 0.0 && ts_us > state.0 {
            (ts_us - state.0).clamp(5_000.0, 200_000.0)
        } else {
            33_333.0
        };
        let delta_ticks = ((delta_us * 90_000.0) / 1_000_000.0).round() as u32;
        state.0 = ts_us;
        state.1 = state.1.wrapping_add(delta_ticks.max(900));
        let rtp_timestamp = state.1;

        const MAX_VP8_CHUNK: usize = 1100;
        let chunks: Vec<&[u8]> = raw_vp8.chunks(MAX_VP8_CHUNK).collect();
        let num_chunks = chunks.len();

        for (idx, chunk) in chunks.into_iter().enumerate() {
            let mut rtp_payload = Vec::with_capacity(1 + chunk.len());
            // RFC 7741 VP8 Payload Descriptor: S=1 (0x10) on first fragment of frame, 0x00 on continuation
            rtp_payload.push(if idx == 0 { 0x10 } else { 0x00 });
            rtp_payload.extend_from_slice(chunk);
            let payload_bytes = bytes::Bytes::from(rtp_payload);
            let is_last = idx + 1 == num_chunks;

            for (_slot_ssrc, track, seq_num) in &viewers {
                let seq = seq_num.fetch_add(1, Ordering::Relaxed);
                let pkt = RtpPacket {
                    header: RtpHeader {
                        version: 2,
                        padding: false,
                        extension: false,
                        marker: is_last,
                        payload_type: 120,
                        sequence_number: seq,
                        timestamp: rtp_timestamp,
                        ssrc: 0,
                        csrc: Vec::new(),
                        extension_profile: 0,
                        extensions: Vec::new(),
                        extensions_padding: 0,
                    },
                    payload: payload_bytes.clone(),
                };
                let _ = track.write_rtp(&pkt).await;
            }
        }
    }

    pub fn forward_web_vp8_frame_blocking(
        &self,
        broadcaster_client_id: u64,
        bt: u32,
        btea_payload: &[u8],
    ) {
        if self
            .get_subscribed_viewer_tracks(broadcaster_client_id, bt)
            .is_empty()
        {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.block_on(self.forward_web_vp8_frame(broadcaster_client_id, bt, btea_payload));
        } else {
            static BOT_RTP_RT: std::sync::OnceLock<tokio::runtime::Runtime> =
                std::sync::OnceLock::new();
            let rt = BOT_RTP_RT.get_or_init(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build bot RTP runtime")
            });
            rt.block_on(self.forward_web_vp8_frame(broadcaster_client_id, bt, btea_payload));
        }
    }
}

pub struct Vp8Depacketizer {
    frame_buf: Vec<u8>,
    current_timestamp: Option<u32>,
    in_frame: bool,
    current_is_keyframe: bool,
    last_width: u16,
    last_height: u16,
}

impl Default for Vp8Depacketizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Vp8Depacketizer {
    pub fn new() -> Self {
        Self {
            frame_buf: Vec::with_capacity(65536),
            current_timestamp: None,
            in_frame: false,
            current_is_keyframe: false,
            last_width: 1280,
            last_height: 720,
        }
    }

    /// Processes an incoming RFC 7741 VP8 RTP packet and returns a complete 13-byte-prefixed
    /// BlackTeaWeb video frame when the RTP marker bit completes a valid frame.
    pub fn push_rtp(&mut self, rtp_timestamp: u32, marker: bool, payload: &[u8]) -> Option<Vec<u8>> {
        if payload.is_empty() {
            return None;
        }
        let b0 = payload[0];
        let x_bit = (b0 & 0x80) != 0;
        let s_bit = (b0 & 0x10) != 0;
        let pid = b0 & 0x07;
        let mut offset = 1usize;

        if x_bit {
            if offset >= payload.len() {
                return None;
            }
            let ext = payload[offset];
            offset += 1;
            let i_bit = (ext & 0x80) != 0;
            let l_bit = (ext & 0x40) != 0;
            let t_bit = (ext & 0x20) != 0;
            let k_bit = (ext & 0x10) != 0;

            if i_bit {
                if offset >= payload.len() {
                    return None;
                }
                if (payload[offset] & 0x80) != 0 {
                    offset += 2;
                } else {
                    offset += 1;
                }
            }
            if l_bit {
                offset += 1;
            }
            if t_bit || k_bit {
                offset += 1;
            }
        }

        if offset >= payload.len() {
            return None;
        }

        let vp8_chunk = &payload[offset..];

        if s_bit && pid == 0 {
            self.frame_buf.clear();
            self.current_timestamp = Some(rtp_timestamp);
            self.in_frame = true;
            // RFC 6386 VP8 uncompressed 3-byte frame tag: bit 0 is 0 for keyframe, 1 for interframe
            self.current_is_keyframe = (vp8_chunk[0] & 0x01) == 0;
            if self.current_is_keyframe && vp8_chunk.len() >= 10 {
                if vp8_chunk[3] == 0x9D && vp8_chunk[4] == 0x01 && vp8_chunk[5] == 0x2A {
                    let w = u16::from_le_bytes([vp8_chunk[6], vp8_chunk[7]]) & 0x3FFF;
                    let h = u16::from_le_bytes([vp8_chunk[8], vp8_chunk[9]]) & 0x3FFF;
                    if w > 0 && h > 0 {
                        self.last_width = w;
                        self.last_height = h;
                    }
                }
            }
        } else if !self.in_frame || self.current_timestamp != Some(rtp_timestamp) {
            self.frame_buf.clear();
            self.in_frame = false;
            return None;
        }

        self.frame_buf.extend_from_slice(vp8_chunk);

        if marker && self.in_frame && !self.frame_buf.is_empty() {
            self.in_frame = false;
            let timestamp_us = ((rtp_timestamp as u64 * 1_000_000) / 90_000) as f64;
            let mut btea_frame = Vec::with_capacity(13 + self.frame_buf.len());
            btea_frame.push(if self.current_is_keyframe { 1 } else { 0 });
            btea_frame.extend_from_slice(&self.last_width.to_le_bytes());
            btea_frame.extend_from_slice(&self.last_height.to_le_bytes());
            btea_frame.extend_from_slice(&timestamp_us.to_le_bytes());
            btea_frame.extend_from_slice(&self.frame_buf);
            self.frame_buf.clear();
            return Some(btea_frame);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;

    #[test]
    fn vp8_depacketizer_reassembles_keyframe_and_extracts_dimensions() {
        let mut depack = Vp8Depacketizer::new();
        // Fragment 1: X=1, S=1, PID=0 (0x90), ext I=1 (0x80), 15-bit PictureID (0x80, 0x01)
        // Followed by VP8 keyframe tag (0x10, 0x00, 0x00), start code (0x9D, 0x01, 0x2A),
        // width=1920 (0x80, 0x07), height=1080 (0x38, 0x04)
        let frag1 = [
            0x90, 0x80, 0x80, 0x01, // 4-byte RFC 7741 descriptor
            0x10, 0x00, 0x00, 0x9D, 0x01, 0x2A, 0x80, 0x07, 0x38, 0x04,
        ];
        assert!(depack.push_rtp(90_000, false, &frag1).is_none());

        // Fragment 2: X=1, S=0, PID=0 (0x80), ext I=1 (0x80), 15-bit PictureID (0x80, 0x01)
        // Followed by 3 continuation payload bytes, with marker=true
        let frag2 = [0x80, 0x80, 0x80, 0x01, 0xAA, 0xBB, 0xCC];
        let frame = depack
            .push_rtp(90_000, true, &frag2)
            .expect("marker packet should complete BTEA VP8 frame");

        // 13-byte BTEA header + 10 bytes from frag1 + 3 bytes from frag2 = 26 bytes
        assert_eq!(frame.len(), 26);
        assert_eq!(frame[0], 1); // keyframe flag
        assert_eq!(u16::from_le_bytes([frame[1], frame[2]]), 1920);
        assert_eq!(u16::from_le_bytes([frame[3], frame[4]]), 1080);
        let ts_us = f64::from_le_bytes(frame[5..13].try_into().unwrap());
        assert!((ts_us - 1_000_000.0).abs() < 1.0);
        assert_eq!(&frame[23..26], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn assign_viewer_slot_evicts_self_preview_before_same_broadcaster_other_stream() {
        let mgr = RtcBroadcastManager::new();
        let make_track = |idx: usize| {
            Arc::new(TrackLocalStaticRTP::new(
                RTCRtpCodecCapability {
                    mime_type: "video/VP8".to_string(),
                    clock_rate: 90000,
                    ..Default::default()
                },
                format!("v-{}", idx),
                format!("s-{}", idx),
            ))
        };
        mgr.set_viewer_slots(
            101,
            vec![
                ViewerVideoSlot {
                    slot_index: 0,
                    ssrc: 1000,
                    track: make_track(0),
                    seq_num: Arc::new(AtomicU16::new(1)),
                    assigned_broadcast: Some((101, 0)),
                },
                ViewerVideoSlot {
                    slot_index: 1,
                    ssrc: 1001,
                    track: make_track(1),
                    seq_num: Arc::new(AtomicU16::new(1)),
                    assigned_broadcast: None,
                },
            ],
        );

        // Joining remote broadcaster 20001 bt=0 takes free slot 1 (ssrc 1001)
        let res0 = mgr.assign_viewer_slot(101, 20001, 0);
        assert_eq!(res0, Some((1001, None)));

        // Immediately joining remote broadcaster 20001 bt=1 must evict self-preview (101, 0) on slot 0 (ssrc 1000)
        // instead of overwriting (20001, 0) on slot 1 (ssrc 1001)
        let res1 = mgr.assign_viewer_slot(101, 20001, 1);
        assert_eq!(res1, Some((1000, Some((101, 0)))));
    }

    #[test]
    fn register_incoming_ssrc_type_swaps_displaced_ssrc_when_screen_starts_first() {
        let mgr = RtcBroadcastManager::new();
        // SDP offer tentatively maps SSRC 500 -> bt=0 and SSRC 501 -> bt=1
        mgr.register_incoming_ssrc_type(42, 500, 0);
        mgr.register_incoming_ssrc_type(42, 501, 1);

        // User starts Screen Share (bt=1) first -> TeaSpeak assigns its first free sender SSRC 500 to bt=1
        mgr.register_incoming_ssrc_type(42, 500, 1);

        // SSRC 500 must be bt=1, and SSRC 501 must be automatically swapped to bt=0!
        assert_eq!(mgr.incoming_ssrc_types.read().unwrap().get(&(42, 500)), Some(&1));
        assert_eq!(mgr.incoming_ssrc_types.read().unwrap().get(&(42, 501)), Some(&0));
        assert_eq!(mgr.get_incoming_track_ssrc(42, 1), Some(500));
        assert_eq!(mgr.get_incoming_track_ssrc(42, 0), Some(501));
    }
}

