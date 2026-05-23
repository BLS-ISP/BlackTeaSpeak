use std::net::SocketAddr;
use tokio::time::Instant;
use crate::runtime::QuerySessionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopHandshakeStage {
    Initial,
    Connected,
}

pub struct DesktopClientSession {
    pub client_id: u64,
    pub remote_addr: SocketAddr,
    pub last_seen: Instant,
    pub stage: DesktopHandshakeStage,
    
    // Cryptography state
    pub shared_secret: Option<Vec<u8>>,
    pub iv: Option<Vec<u8>>,
    
    // Packet Tracking
    pub next_packet_id: u16,
    pub fragment_buffer: Vec<u8>,
    
    // Server Execution
    pub handler: crate::desktop_handler::DesktopSessionHandler,
}

impl DesktopClientSession {
    pub fn new(client_id: u64, remote_addr: SocketAddr) -> Self {
        
        Self {
            client_id,
            remote_addr,
            last_seen: Instant::now(),
            stage: DesktopHandshakeStage::Initial,
            shared_secret: None,
            iv: None,
            next_packet_id: 1, // Start sequence numbers at 1
            fragment_buffer: Vec::new(),
            handler: crate::desktop_handler::DesktopSessionHandler::new(client_id, remote_addr.ip().to_string()),
        }
    }
}
