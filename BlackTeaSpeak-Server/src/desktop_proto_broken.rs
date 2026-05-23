
    // Observed on-wire success path from a real BlackTeaSpeak server.
    packet[75] = 0x01;
    packet[139] = 0x01;
    packet[142] = 0x03;
    packet[143] = 0xE8;
    packet
}

pub fn build_encrypted_ack_packet(
    send_packet_id: u16,
    ack_packet_id: u16,
pub fn classify_observed_desktop_packet(bytes: &[u8]) -> Option<&'static str> {
    ObservedDesktopPacket::parse(bytes).map(|packet| packet.classification())
}

pub fn describe_observed_desktop_packet(bytes: &[u8]) -> Option<String> {
    ObservedDesktopPacket::parse(bytes).map(|packet| packet.describe())
}

pub fn parse_ts3init_solve_puzzle_payload(
    payload: &[u8],
) -> Option<Ts3InitSolvePuzzlePayloadModel> {
    let tail = printable_ascii_suffix(payload, TS3INIT_SOLVE_PUZZLE_CLIENTINITIV_MARKER.len())?;
    if !tail.starts_with(TS3INIT_SOLVE_PUZZLE_CLIENTINITIV_MARKER) {
        return None;
    }

    let trailing_marker = tail
        .split_ascii_whitespace()
        .last()
        .filter(|value| value.starts_with('-'))?
        .to_string();

    let mut alpha = None;
    let mut ot = None;
    let mut ip = None;
    let mut omega = None;

    for token in tail
        .split_ascii_whitespace()
        .skip(1)
        .take_while(|value| !value.starts_with('-'))
    {
        let (key, value) = token.split_once('=')?;
        match key {
            "alpha" => alpha = Some(value.to_string()),
            "ot" => ot = value.parse::<u32>().ok(),
            "ip" => ip = Some(value.to_string()),
            "omega" => omega = Some(value.to_string()),
            _ => {}
        }
    }

    let alpha = alpha?;
    let ot = ot?;
    let ip = ip?;
    let omega = omega?;
    
    // TS3 escapes '/' as '\/' in commands. Base64 strings can contain '/', so we must unescape it.
    let alpha_unescaped = alpha.replace("\\/", "/");
    let omega_unescaped = omega.replace("\\/", "/");
    
    let alpha_bytes = BASE64_STANDARD.decode(alpha_unescaped.as_bytes()).ok()?;
    let omega_bytes = BASE64_STANDARD.decode(omega_unescaped.as_bytes()).ok()?;
    let omega_der = parse_ts3init_omega_der(&omega_bytes)?;
    let ascii_offset = payload.len().checked_sub(tail.len())?;
    let observed_set_puzzle_prefix = observed_ts3init_set_puzzle_payload();
    let observed_set_puzzle_prefix_matches = payload.len() >= observed_set_puzzle_prefix.len()
        && payload[..observed_set_puzzle_prefix.len()] == observed_set_puzzle_prefix;
    let zero_gap_len = ascii_offset.saturating_sub(observed_set_puzzle_prefix.len());
    let zero_gap_is_all_zero = payload
        .get(observed_set_puzzle_prefix.len()..ascii_offset)
        .map(|gap| gap.iter().all(|byte| *byte == 0))
        .unwrap_or(false);

    Some(Ts3InitSolvePuzzlePayloadModel {
        observed_set_puzzle_prefix_matches,
        observed_set_puzzle_prefix_len: observed_set_puzzle_prefix.len(),
        zero_gap_len,
        zero_gap_is_all_zero,
        ascii_offset,
        tail,
        alpha,
        alpha_bytes,
        ot,
        ip,
        omega,
        omega_der,
        trailing_marker,
    })
}

pub fn build_ts3init_reset_packet() -> [u8; TS3INIT_RESET_PACKET_LENGTH] {
    [
        b'T', b'S', b'3', b'I', b'N', b'I', b'T', b'1', 0x00, 0x65, TS3INIT_FLAGS, 0x7F,
        0x00,
    ]
}

pub fn default_ts3init_set_cookie_random_seed() -> [u8; TS3INIT_RANDOM_SEED_LENGTH] {
    let mut seed = [0_u8; TS3INIT_RANDOM_SEED_LENGTH];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = index as u8;
    }
    seed
}

pub fn build_ts3init_set_puzzle_observed_packet() -> [u8; TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH] {
    let mut packet = [0_u8; TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH];
    packet[..8].copy_from_slice(TS3INIT1_MAGIC);
    packet[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
    packet[10] = TS3INIT_FLAGS;
    packet[11] = 0x03;

    // Use the mathematically valid trivial puzzle (x=1, n=1, level=1000)
    packet[75] = 0x01;
    packet[139] = 0x01;
    packet[142] = 0x03;
    packet[143] = 0xE8;
    
    packet
}

pub fn build_ts3init_set_cookie_packet(
    request_bytes: &[u8],
    remote_addr: SocketAddr,
    local_addr: SocketAddr,
    random_seed: &[u8; TS3INIT_RANDOM_SEED_LENGTH],
    current_unix_time: u32,
) -> Option<[u8; TS3INIT_SET_COOKIE_PACKET_LENGTH]> {
    let random_sequence = match ObservedDesktopPacket::parse(request_bytes)? {
        ObservedDesktopPacket::Ts3InitGetCookie {
            random_sequence,
            ..
        } => random_sequence,
        _ => return None,
    };

    let packet_index = (current_unix_time % 8) as u8;
    let cookie = calculate_ts3init_cookie(
        remote_addr,
        local_addr,
        random_seed,
        current_unix_time,
        packet_index,
    )?;

    let mut packet = [0_u8; TS3INIT_SET_COOKIE_PACKET_LENGTH];
    packet[..8].copy_from_slice(TS3INIT1_MAGIC);
    packet[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
    packet[10] = TS3INIT_FLAGS;
    packet[11] = 0x01;
    packet[12..20].copy_from_slice(&cookie.to_le_bytes());
    packet[20] = packet_index;
    packet[21..28].fill(0);
    packet[28..32].copy_from_slice(&random_sequence.to_le_bytes());
    Some(packet)
}

pub fn build_ts3init_set_cookie_compact_packet(
    request_bytes: &[u8],
    remote_addr: SocketAddr,
    local_addr: SocketAddr,
    random_seed: &[u8; TS3INIT_RANDOM_SEED_LENGTH],
    current_unix_time: u32,
) -> Option<[u8; TS3INIT_SET_COOKIE_COMPACT_PACKET_LENGTH]> {
    let packet = build_ts3init_set_cookie_packet(
        request_bytes,
        remote_addr,
        local_addr,
        random_seed,
        current_unix_time,
    )?;
    let mut compact_packet = [0_u8; TS3INIT_SET_COOKIE_COMPACT_PACKET_LENGTH];
    compact_packet.copy_from_slice(&packet[11..]);
    Some(compact_packet)
}

pub fn build_ts3init_set_cookie_error_packet(error_code: u32) -> [u8; TS3INIT_SET_COOKIE_ERROR_PACKET_LENGTH] {
    let mut packet = [0_u8; TS3INIT_SET_COOKIE_ERROR_PACKET_LENGTH];
    packet[0] = 0x01;
    packet[1..5].copy_from_slice(&error_code.to_le_bytes());
    packet
}



pub fn build_ts3init_set_puzzle_packet(
) -> [u8; TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH] {
    let mut packet = [0_u8; TS3INIT_SET_PUZZLE_OBSERVED_PACKET_LENGTH];
    packet[..8].copy_from_slice(TS3INIT1_MAGIC);
    packet[8..10].copy_from_slice(&TS3INIT_PACKET_ID.to_be_bytes());
    packet[10] = TS3INIT_FLAGS;
    packet[11] = 0x03;

    // Observed on-wire success path from a real BlackTeaSpeak server.
    packet[75] = 0x01;
    packet[139] = 0x01;
    packet[142] = 0x03;
    packet[143] = 0xE8;
    packet
}

pub fn build_encrypted_ack_packet(
    send_packet_id: u16,
    ack_packet_id: u16,
) -> Vec<u8> {
    use crate::desktop_crypto::encrypt_with_dummy_key;

    let payload = ack_packet_id.to_be_bytes();

    let mut header = [0u8; 3];
    header[0..2].copy_from_slice(&send_packet_id.to_be_bytes());
    header[2] = 0x86; // NewProtocol | Ack

    encrypt_with_dummy_key(send_packet_id, &header, &payload)
}

pub fn build_unencrypted_ack_packet(
    send_packet_id: u16,
