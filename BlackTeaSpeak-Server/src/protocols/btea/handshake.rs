pub fn encrypt_btea_packet(
    packet_id: u16,
    generation_id: u32,
    packet_type_raw: u8,
    header: &[u8],
    payload: &[u8],
    session_secret: &[u8],
    is_server_to_client: bool,
) -> Vec<u8> {
    use sha2::{Sha256, Digest};
    use aes::Aes128;
    use eax::aead::{Aead, Payload};
    use eax::aead::generic_array::GenericArray;
    use eax::{Eax, NewAead};

    let mut tmp_to_hash = Vec::with_capacity(6 + session_secret.len());
    tmp_to_hash.push(if is_server_to_client { 0x30 } else { 0x31 });
    tmp_to_hash.push(packet_type_raw & 0x0F);
    tmp_to_hash.extend_from_slice(&generation_id.to_be_bytes());
    tmp_to_hash.extend_from_slice(session_secret);

    let hash_result = Sha256::digest(&tmp_to_hash);
    let mut key = [0u8; 16];
    let mut nonce_bytes = [0u8; 16];
    key.copy_from_slice(&hash_result[0..16]);
    nonce_bytes.copy_from_slice(&hash_result[16..32]);

    key[0] ^= (packet_id >> 8) as u8;
    key[1] ^= (packet_id & 0xFF) as u8;

    let cipher = Eax::<Aes128>::new(GenericArray::from_slice(&key));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let aead_payload = Payload {
        msg: payload,
        aad: header,
    };

    let encrypted = cipher.encrypt(nonce, aead_payload).unwrap_or_default();
    
    let mac = &encrypted[encrypted.len() - 16..encrypted.len() - 8];
    
    let mut result = Vec::with_capacity(8 + payload.len());
    result.extend_from_slice(mac);
    result.extend_from_slice(&encrypted[..encrypted.len() - 16]);
    
    result
}

pub fn decrypt_btea_packet(
    packet_id: u16,
    generation_id: u32,
    packet_type_raw: u8,
    header: &[u8],
    payload_with_mac: &[u8],
    session_secret: &[u8],
    is_server_to_client: bool,
) -> Option<Vec<u8>> {
    decrypt_with_session_key(
        packet_id,
        generation_id,
        packet_type_raw,
        header,
        payload_with_mac,
        session_secret,
        is_server_to_client,
    )
}

pub fn decrypt_with_session_key(
    packet_id: u16,
    generation_id: u32,
    packet_type_raw: u8,
    header: &[u8],
    payload_with_mac: &[u8],
    shared_secret: &[u8],
    is_server_to_client: bool,
) -> Option<Vec<u8>> {
    if payload_with_mac.len() < 8 {
        return None;
    }
    use sha2::{Sha256, Digest};
    use aes::Aes128;
    use eax::aead::{Aead, Payload};
    use eax::aead::generic_array::GenericArray;
    use eax::{Eax, NewAead};

    let mut tmp_to_hash = Vec::with_capacity(6 + shared_secret.len());
    tmp_to_hash.push(if is_server_to_client { 0x30 } else { 0x31 });
    tmp_to_hash.push(packet_type_raw & 0x0F);
    tmp_to_hash.extend_from_slice(&generation_id.to_be_bytes());
    tmp_to_hash.extend_from_slice(shared_secret);

    let hash_result = Sha256::digest(&tmp_to_hash);
    let mut key = [0u8; 16];
    let mut nonce_bytes = [0u8; 16];
    key.copy_from_slice(&hash_result[0..16]);
    nonce_bytes.copy_from_slice(&hash_result[16..32]);

    key[0] ^= (packet_id >> 8) as u8;
    key[1] ^= (packet_id & 0xFF) as u8;

    let cipher = Eax::<Aes128>::new(GenericArray::from_slice(&key));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let client_mac = &payload_with_mac[0..8];
    let ciphertext = &payload_with_mac[8..];

    let zeroes = vec![0u8; ciphertext.len()];
    let encrypted_zeroes = cipher.encrypt(nonce, Payload { msg: &zeroes, aad: b"" }).unwrap_or_default();
    if encrypted_zeroes.len() < 16 {
        return None;
    }
    let keystream = &encrypted_zeroes[..encrypted_zeroes.len() - 16];

    let mut decrypted = vec![0u8; ciphertext.len()];
    for i in 0..ciphertext.len() {
        decrypted[i] = ciphertext[i] ^ keystream[i];
    }

    let re_encrypted = cipher.encrypt(nonce, Payload { msg: &decrypted, aad: header }).unwrap_or_default();
    if re_encrypted.len() < 16 {
        return None;
    }
    let computed_mac = &re_encrypted[re_encrypted.len() - 16..re_encrypted.len() - 8];

    if computed_mac != client_mac {
        return None;
    }

    Some(decrypted)
}
