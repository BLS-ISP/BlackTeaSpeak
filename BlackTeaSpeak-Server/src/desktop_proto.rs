use std::net::{IpAddr, SocketAddr};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Digest, Sha512};

const TS3INIT1_MAGIC: &[u8; 8] = b"TS3INIT1";
const TS3INIT_PACKET_ID: u16 = 101;
const TS3INIT_FLAGS: u8 = 0x88;
const TS3INIT_CLIENT_HEADER_LENGTH: usize = 18;
const TS3INIT_SERVER_HEADER_LENGTH: usize = 12;
const TS3INIT_GET_COOKIE_PACKET_LENGTH: usize = 34;
const TS3INIT_SET_COOKIE_PACKET_LENGTH: usize = 32;
const TS3INIT_SET_COOKIE_COMPACT_PACKET_LENGTH: usize = 21;
const TS3INIT_SET_COOKIE_ERROR_PACKET_LENGTH: usize = 5;
const TS3INIT_GET_PUZZLE_PACKET_LENGTH: usize = 38;
const TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH: usize = 244;
const TS3INIT_SET_PUZZLE_OBSERVED_PAYLOAD_LENGTH: usize =
    TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH - TS3INIT_SERVER_HEADER_LENGTH;
const TS3INIT_RESET_PACKET_LENGTH: usize = 13;

pub const TS3INIT_RANDOM_SEED_LENGTH: usize = 60;
const TS3INIT_SOLVE_PUZZLE_CLIENTINITIV_MARKER: &str = "clientinitiv ";
const BINARY_BOOTSTRAP_MIN_PACKET_LENGTH: usize = 185;
const BINARY_BOOTSTRAP_MAX_PACKET_LENGTH: usize = 186;
const BINARY_BOOTSTRAP_PREFIX_LENGTH: usize = 8;
const BINARY_BOOTSTRAP_MARKER_LENGTH: usize = 8;
const BINARY_BOOTSTRAP_BLOCK_LENGTH: usize = 16;
const BINARY_BOOTSTRAP_FIXED_SECTION_LENGTH: usize = BINARY_BOOTSTRAP_PREFIX_LENGTH
    + BINARY_BOOTSTRAP_MARKER_LENGTH
    + BINARY_BOOTSTRAP_BLOCK_LENGTH
    + BINARY_BOOTSTRAP_BLOCK_LENGTH;
const BINARY_FOLLOWUP_39_PACKET_LENGTH: usize = 39;
const BINARY_FOLLOWUP_39_PREFIX_LENGTH: usize = 8;
const BINARY_FOLLOWUP_39_FIXED_LENGTH: usize = 4;
const BINARY_FOLLOWUP_39_MARKER_LENGTH: usize = 4;
const BINARY_FOLLOWUP_39_BODY_LENGTH: usize = BINARY_FOLLOWUP_39_PACKET_LENGTH
    - BINARY_FOLLOWUP_39_PREFIX_LENGTH
    - BINARY_FOLLOWUP_39_FIXED_LENGTH
    - BINARY_FOLLOWUP_39_MARKER_LENGTH;
const BINARY_ACK_13_PACKET_LENGTH: usize = 13;
const BINARY_ACK_13_PREFIX_LENGTH: usize = 8;
const BINARY_ACK_15_PACKET_LENGTH: usize = 15;
const BINARY_ACK_15_PREFIX_LENGTH: usize = 8;
const OBSERVED_BINARY_BOOTSTRAP_MARKER_AT_8: [u8; 8] = [
    0x00, 0x00, 0x00, 0x00, 0x22, 0x9D, 0x74, 0x8B,
];
const OBSERVED_BINARY_FOLLOWUP_39_FIXED_AT_8: [u8; 4] = [0x00, 0x01, 0x00, 0x00];
const OBSERVED_BINARY_FOLLOWUP_39_MARKER_AT_12: [u8; 4] = [0x22, 0x9D, 0x74, 0x8B];
const OBSERVED_BINARY_ACK_13_MARKER: u8 = 0xA6;
const OBSERVED_BINARY_ACK_15_MARKER: u8 = 0x26;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ts3InitCommand {
    GetCookie,
    SetCookie,
    GetPuzzle,
    SetPuzzle,
    SolvePuzzle,
    ResetPuzzle,
    Reset,
    Unknown(u8),
}

impl Ts3InitCommand {
    fn from_byte(value: u8) -> Self {
        match value {
            0 => Self::GetCookie,
            1 => Self::SetCookie,
            2 => Self::GetPuzzle,
            3 => Self::SetPuzzle,
            4 => Self::SolvePuzzle,
            5 => Self::ResetPuzzle,
            127 => Self::Reset,
            _ => Self::Unknown(value),
        }
    }

    fn describe(self) -> String {
        match self {
            Self::GetCookie => String::from("GET_COOKIE"),
            Self::SetCookie => String::from("SET_COOKIE"),
            Self::GetPuzzle => String::from("GET_PUZZLE"),
            Self::SetPuzzle => String::from("SET_PUZZLE"),
            Self::SolvePuzzle => String::from("SOLVE_PUZZLE"),
            Self::ResetPuzzle => String::from("RESET_PUZZLE"),
            Self::Reset => String::from("RESET"),
            Self::Unknown(value) => format!("UNKNOWN({value})"),
        }
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ts3InitGetPuzzleValidation {
    pub cookie: u64,
    pub expected_cookie: u64,
    pub packet_index: u8,
    pub cookie_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ts3InitSolvePuzzlePayloadModel {
    pub alpha: String,
    pub alpha_bytes: Vec<u8>,
    pub ot: u32,
    pub ip: Option<String>,
    pub omega: String,
    pub omega_bytes: Vec<u8>,
    pub teaspeak: bool,
}

pub fn parse_ts3init_solve_puzzle_payload(
    payload: &[u8],
) -> Option<Ts3InitSolvePuzzlePayloadModel> {
    // Find "clientinitiv " in the payload
    let marker = TS3INIT_SOLVE_PUZZLE_CLIENTINITIV_MARKER.as_bytes();
    let start_idx = payload.windows(marker.len()).position(|w| w == marker)?;
    let tail_bytes = &payload[start_idx..];
    
    // Convert to string up to the first null byte or non-printable character (excluding \n\r)
    let end_idx = tail_bytes.iter().position(|&b| b == 0).unwrap_or(tail_bytes.len());
    let tail = std::str::from_utf8(&tail_bytes[..end_idx]).ok()?;

    let mut alpha = None;
    let mut ot = None;
    let mut ip = None;
    let mut omega = None;
    let mut teaspeak = false;

    for token in tail
        .split_ascii_whitespace()
        .skip(1)
    {
        if let Some((key, value)) = token.split_once('=') {
            match key {
                "alpha" => alpha = Some(value.to_string()),
                "ot" => ot = value.parse::<u32>().ok(),
                "ip" => ip = Some(value.to_string()),
                "omega" => omega = Some(value.to_string()),
                "teaspeak" => teaspeak = value == "1",
                _ => {}
            }
        } else {
            // Some parameters don't have an equal sign (e.g. `ip` in TeaSpeak client)
            if token == "ip" {
                ip = Some(String::new());
            } else if token == "teaspeak" || token == "-teaspeak" {
                teaspeak = true;
            }
        }
    }

    let alpha = alpha?;
    let ot = ot?;
    let omega = omega?;
    
    // TS3 escapes '/' as '\/' in commands. Base64 strings can contain '/', so we must unescape it.
    let alpha_unescaped = alpha.replace("\\/", "/");
    let omega_unescaped = omega.replace("\\/", "/");
    
    let alpha_bytes = BASE64_STANDARD.decode(alpha_unescaped.as_bytes()).ok()?;
    let omega_bytes = BASE64_STANDARD.decode(omega_unescaped.as_bytes()).ok()?;

    Some(Ts3InitSolvePuzzlePayloadModel {
        alpha,
        alpha_bytes,
        ot,
        ip,
        omega,
        omega_bytes,
        teaspeak,
    })
}



#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedDesktopPacket {
    BtInitRequest {
        client_public_key: [u8; 32],
    },
    BtInitResponse {
        server_public_key: [u8; 32],
    },
    BinaryBootstrap {
        prefix8: [u8; BINARY_BOOTSTRAP_PREFIX_LENGTH],
        marker8: [u8; BINARY_BOOTSTRAP_MARKER_LENGTH],
        block_a: [u8; BINARY_BOOTSTRAP_BLOCK_LENGTH],
        block_b: [u8; BINARY_BOOTSTRAP_BLOCK_LENGTH],
        tail: Vec<u8>,
    },
    BinaryFollowup39 {
        prefix8: [u8; BINARY_FOLLOWUP_39_PREFIX_LENGTH],
        fixed4: [u8; BINARY_FOLLOWUP_39_FIXED_LENGTH],
        marker4: [u8; BINARY_FOLLOWUP_39_MARKER_LENGTH],
        body: [u8; BINARY_FOLLOWUP_39_BODY_LENGTH],
    },
    BinaryAck13 {
        prefix8: [u8; BINARY_ACK_13_PREFIX_LENGTH],
        sequence: u16,
        ack_marker: u8,
        next_sequence: u16,
    },
    BinaryAck15 {
        prefix8: [u8; BINARY_ACK_15_PREFIX_LENGTH],
        sequence: u16,
        reserved: u16,
        ack_marker: u8,
        ack_tail: [u8; 2],
    },
    BinarySegmentFrame {
        prefix8: [u8; 8],
        sequence: u16,
        tag: u16,
        payload: Vec<u8>,
    },
    Ts3EncryptedPacket {
        mac: [u8; 8],
        packet_id: u16,
        client_id: u16,
        flags: u8,
        payload: Vec<u8>,
    },
    Ts3InitGetCookie {
        random_sequence: u32,
    },
    Ts3InitGetPuzzle {
        cookie: u64,
        packet_index: u8,
    },
    Ts3InitSolvePuzzle {
        payload: Vec<u8>,
    },
}

impl ObservedDesktopPacket {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(TS3INIT1_MAGIC) {
            // TS3 packets with TS3INIT1_MAGIC have a 13-byte header:
            // 8 bytes magic + 2 bytes packet ID + 2 bytes client ID + 1 byte flags
            if bytes.len() >= 13 {
                let packet_id = u16::from_be_bytes(bytes[8..10].try_into().unwrap());
                let flags = bytes[12];
                
                if packet_id == TS3INIT_PACKET_ID && flags == TS3INIT_FLAGS {
                    let payload = &bytes[13..];
                    if payload.len() >= 5 {
                        let command = payload[4];
                        match command {
                            0x00 => { // GetCookie
                                if payload.len() >= 13 {
                                    let mut seq_bytes = [0u8; 4];
                                    seq_bytes.copy_from_slice(&payload[9..13]);
                                    return Some(Self::Ts3InitGetCookie {
                                        random_sequence: u32::from_le_bytes(seq_bytes),
                                    });
                                }
                            }
                            0x02 => { // GetPuzzle
                                if payload.len() >= 21 {
                                    let mut cookie_bytes = [0u8; 8];
                                    cookie_bytes.copy_from_slice(&payload[5..13]);
                                    return Some(Self::Ts3InitGetPuzzle {
                                        cookie: u64::from_le_bytes(cookie_bytes),
                                        packet_index: payload[13],
                                    });
                                }
                            }
                            0x04 => { // SolvePuzzle
                                return Some(Self::Ts3InitSolvePuzzle {
                                    payload: payload.to_vec(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if bytes.starts_with(b"BTEAINIT") {
            if bytes.len() >= 41 {
                let packet_type = bytes[8];
                if packet_type == 0x01 {
                    let mut client_public_key = [0u8; 32];
                    client_public_key.copy_from_slice(&bytes[9..41]);
                    return Some(Self::BtInitRequest { client_public_key });
                } else if packet_type == 0x02 {
                    let mut server_public_key = [0u8; 32];
                    server_public_key.copy_from_slice(&bytes[9..41]);
                    return Some(Self::BtInitResponse { server_public_key });
                }
            }
            return None;
        }

        if (BINARY_BOOTSTRAP_MIN_PACKET_LENGTH..=BINARY_BOOTSTRAP_MAX_PACKET_LENGTH).contains(&bytes.len())
            && bytes.get(8..16) == Some(&OBSERVED_BINARY_BOOTSTRAP_MARKER_AT_8)
        {
            let prefix8 = bytes[..BINARY_BOOTSTRAP_PREFIX_LENGTH].try_into().ok()?;
            let marker8 = bytes[BINARY_BOOTSTRAP_PREFIX_LENGTH..BINARY_BOOTSTRAP_PREFIX_LENGTH + BINARY_BOOTSTRAP_MARKER_LENGTH].try_into().ok()?;
            let block_a = bytes[16..16 + BINARY_BOOTSTRAP_BLOCK_LENGTH].try_into().ok()?;
            let block_b = bytes[32..32 + BINARY_BOOTSTRAP_BLOCK_LENGTH].try_into().ok()?;
            let tail = bytes[BINARY_BOOTSTRAP_FIXED_SECTION_LENGTH..].to_vec();
            return Some(Self::BinaryBootstrap { prefix8, marker8, block_a, block_b, tail });
        }

        if bytes.len() > 11 {
            let mut mac = [0u8; 8];
            mac.copy_from_slice(&bytes[0..8]);
            let packet_id = u16::from_be_bytes(bytes[8..10].try_into().ok()?);
            let client_id = u16::from_be_bytes(bytes[10..12].try_into().ok()?);
            let flags = bytes[12];
            let payload = bytes[13..].to_vec();
            return Some(Self::Ts3EncryptedPacket { mac, packet_id, client_id, flags, payload });
        }
        
        None
    }

    pub fn describe(&self) -> String {
        match self {
            Self::BtInitRequest { .. } => "BtInitRequest".to_string(),
            Self::BtInitResponse { .. } => "BtInitResponse".to_string(),
            Self::BinaryBootstrap { .. } => "BinaryBootstrap".to_string(),
            Self::BinaryFollowup39 { .. } => "BinaryFollowup39".to_string(),
            Self::BinaryAck13 { .. } => "BinaryAck13".to_string(),
            Self::BinaryAck15 { .. } => "BinaryAck15".to_string(),
            Self::BinarySegmentFrame { .. } => "BinarySegmentFrame".to_string(),
            Self::Ts3EncryptedPacket { packet_id, flags, .. } => format!("Ts3EncryptedPacket(id={}, flags={})", packet_id, flags),
            Self::Ts3InitGetCookie { .. } => "Ts3InitGetCookie".to_string(),
            Self::Ts3InitGetPuzzle { .. } => "Ts3InitGetPuzzle".to_string(),
            Self::Ts3InitSolvePuzzle { .. } => "Ts3InitSolvePuzzle".to_string(),
        }
    }
}
