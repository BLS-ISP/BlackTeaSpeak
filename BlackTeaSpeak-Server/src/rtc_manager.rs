use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::net::SocketAddr;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::peer_connection::RTCPeerConnection;

pub struct RtcBroadcastManager {
    // Map from broadcaster's client ID to their TrackLocalStaticRTP
    pub tracks: RwLock<HashMap<u32, Arc<TrackLocalStaticRTP>>>,
    
    // Map from client's SocketAddr to their RTCPeerConnection
    pub peer_connections: RwLock<HashMap<SocketAddr, Arc<RTCPeerConnection>>>,
}

impl RtcBroadcastManager {
    pub fn new() -> Self {
        Self {
            tracks: RwLock::new(HashMap::new()),
            peer_connections: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_broadcast(&self, broadcaster_id: u32, mime_type: String) -> Arc<TrackLocalStaticRTP> {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type,
                ..Default::default()
            },
            format!("video-{}", broadcaster_id),
            format!("stream-{}", broadcaster_id),
        ));
        
        let mut lock = self.tracks.write().unwrap();
        lock.insert(broadcaster_id, Arc::clone(&track));
        track
    }

    pub fn remove_broadcast(&self, broadcaster_id: u32) {
        let mut lock = self.tracks.write().unwrap();
        lock.remove(&broadcaster_id);
    }

    pub fn get_broadcast(&self, broadcaster_id: u32) -> Option<Arc<TrackLocalStaticRTP>> {
        let lock = self.tracks.read().unwrap();
        lock.get(&broadcaster_id).cloned()
    }

    pub fn register_connection(&self, addr: SocketAddr, pc: Arc<RTCPeerConnection>) {
        let mut lock = self.peer_connections.write().unwrap();
        lock.insert(addr, pc);
    }

    pub fn remove_connection(&self, addr: SocketAddr) {
        let mut lock = self.peer_connections.write().unwrap();
        lock.remove(&addr);
    }

    pub fn get_connection(&self, addr: SocketAddr) -> Option<Arc<RTCPeerConnection>> {
        let lock = self.peer_connections.read().unwrap();
        lock.get(&addr).cloned()
    }
}
