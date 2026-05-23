use p256::ecdsa::{SigningKey, VerifyingKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::SecretKey;
use rand::rngs::OsRng;
use sha1::{Digest, Sha1};

pub fn generate_server_keypair() -> (SecretKey, VerifyingKey) {
    let secret_key = SecretKey::random(&mut OsRng);
    let public_key = secret_key.public_key();
    (secret_key, public_key.into())
}

pub fn export_public_key_der(verifying_key: &VerifyingKey) -> Vec<u8> {
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    let encoded_point = verifying_key.to_encoded_point(false);
    
    let x_bytes = encoded_point.x().unwrap();
    let y_bytes = encoded_point.y().unwrap();
    
    // TS3 format is exactly 65 bytes:
    // [0x20 (pLength), X (32 bytes), Y (32 bytes)]
    let mut ts3_key = Vec::with_capacity(65);
    ts3_key.push(0x20);
    
    // Pad X to 32 bytes just in case
    let mut x_padded = [0u8; 32];
    x_padded[32 - x_bytes.len()..].copy_from_slice(x_bytes);
    ts3_key.extend_from_slice(&x_padded);
    
    // Pad Y to 32 bytes just in case
    let mut y_padded = [0u8; 32];
    y_padded[32 - y_bytes.len()..].copy_from_slice(y_bytes);
    ts3_key.extend_from_slice(&y_padded);
    
    ts3_key
}

pub fn calculate_shared_secret(client_pub_bytes: &[u8], server_sec: &SecretKey) -> Option<[u8; 20]> {
    use p256::PublicKey;
    let client_pub = PublicKey::from_sec1_bytes(client_pub_bytes).ok()?;
    
    // Perform ECDH: ServerPrivateKey * ClientPublicKey
    let shared_secret = p256::elliptic_curve::ecdh::diffie_hellman(
        server_sec.to_nonzero_scalar(),
        client_pub.as_affine(),
    );

    let x_bytes = shared_secret.raw_secret_bytes();
    
    let mut hasher = Sha1::new();
    hasher.update(x_bytes);
    let result = hasher.finalize();
    
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&result);
    Some(hash)
}

pub fn get_shared_secret2(public_key: &[u8], private_key: &[u8]) -> Option<[u8; 64]> {
    use curve25519_dalek::edwards::CompressedEdwardsY;
    use curve25519_dalek::scalar::Scalar;
    use sha2::{Sha512, Digest};

    if public_key.len() != 32 || private_key.len() != 32 {
        return None;
    }

    // 1. Copy and mask private key (like TS3AudioBot: privateKeyCpy[31] &= 0x7F)
    let mut private_key_cpy = [0u8; 32];
    private_key_cpy.copy_from_slice(private_key);
    private_key_cpy[31] &= 0x7F;
    
    let scalar = Scalar::from_bytes_mod_order(private_key_cpy);

    // 2. Load public key and negate (like ge_frombytes_negate_vartime)
    let mut pub_bytes = [0u8; 32];
    pub_bytes.copy_from_slice(public_key);
    pub_bytes[31] ^= 0x80; // Negate by flipping the X-sign bit

    let compressed_pub = CompressedEdwardsY(pub_bytes);
    let point = compressed_pub.decompress()?;

    // 3. Scalar multiplication
    let shared_point = point * scalar;

    // 4. Compress and negate the shared point (like sharedTmp[31] ^= 0x80)
    let mut shared_bytes = shared_point.compress().to_bytes();
    shared_bytes[31] ^= 0x80;

    // 5. SHA512 hash
    let mut hasher = Sha512::new();
    hasher.update(&shared_bytes);
    let result = hasher.finalize();

    let mut hash = [0u8; 64];
    hash.copy_from_slice(&result);
    Some(hash)
}

pub fn derive_iv_struct(shared_secret: &[u8], alpha: &[u8], beta: &[u8]) -> Vec<u8> {
    let mut iv_struct = vec![0u8; 10 + beta.len()];
    
    // First step: XOR shared_secret and alpha into the start of iv_struct
    // For V1, alpha is 65 bytes. For V2, alpha is 32 bytes.
    let len1 = alpha.len().min(shared_secret.len()).min(iv_struct.len());
    for i in 0..len1 {
        iv_struct[i] = shared_secret[i] ^ alpha[i];
    }
    
    // Second step: XOR shared_secret[10..] and beta into iv_struct[10..]
    for i in 0..beta.len() {
        if i + 10 < shared_secret.len() {
            iv_struct[10 + i] = shared_secret[10 + i] ^ beta[i];
        } else {
            // For V1 fallback where shared_secret is only 20 bytes
            iv_struct[10 + i] = shared_secret[10 + (i % 10)] ^ beta[i]; 
        }
    }
    
    iv_struct
}

pub fn reconstruct_client_pub_key(x_bytes: &[u8], y_bytes: &[u8]) -> Option<Vec<u8>> {
    if x_bytes.len() > 32 || y_bytes.len() > 32 {
        return None;
    }
    
    let mut sec1 = vec![0x04]; // Uncompressed point tag
    
    // Pad X to 32 bytes
    let mut x_padded = vec![0u8; 32];
    x_padded[32 - x_bytes.len()..].copy_from_slice(x_bytes);
    sec1.extend_from_slice(&x_padded);
    
    // Pad Y to 32 bytes
    let mut y_padded = vec![0u8; 32];
    y_padded[32 - y_bytes.len()..].copy_from_slice(y_bytes);
    sec1.extend_from_slice(&y_padded);
    
    Some(sec1)
}

const DUMMY_KEY: &[u8; 16] = b"c:\\windows\\syste";
const DUMMY_NONCE: &[u8; 16] = b"m\\firewall32.cpl";

pub fn encrypt_with_dummy_key(packet_id: u16, header: &[u8], payload: &[u8]) -> Vec<u8> {
    use aes::Aes128;
    use eax::aead::{Aead, Payload};
    use eax::aead::generic_array::GenericArray;
    use eax::{Eax, NewAead};

    let key = DUMMY_KEY.to_vec();

    let cipher = Eax::<Aes128>::new(GenericArray::from_slice(&key));
    let nonce = GenericArray::from_slice(DUMMY_NONCE);

    let aead_payload = Payload {
        msg: payload,
        aad: header,
    };

    let encrypted = cipher.encrypt(nonce, aead_payload).unwrap_or_default();
    
    let mac = &encrypted[encrypted.len() - 16..encrypted.len() - 8];
    
    let mut result = Vec::with_capacity(8 + header.len() + payload.len());
    result.extend_from_slice(mac);
    result.extend_from_slice(header);
    result.extend_from_slice(&encrypted[..encrypted.len() - 16]);
    
    result
}

pub fn decrypt_with_dummy_key(packet_id: u16, header: &[u8], mac: &[u8; 8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    use aes::Aes128;
    use eax::aead::{Aead, Payload};
    use eax::aead::generic_array::GenericArray;
    use eax::{Eax, NewAead};

    let key = DUMMY_KEY.to_vec();

    let cipher = Eax::<Aes128>::new(GenericArray::from_slice(&key));
    let nonce = GenericArray::from_slice(DUMMY_NONCE);

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

    if computed_mac != mac {
        return None;
    }

    Some(decrypted)
}

pub fn encrypt_with_session_key(
    packet_id: u16,
    generation_id: u32,
    packet_type_raw: u8,
    header: &[u8],
    payload: &[u8],
    shared_secret: &[u8],
    client_alpha: &[u8],
    server_beta: &[u8],
    is_server_to_client: bool,
) -> Vec<u8> {
    use sha2::{Sha256, Digest};
    use aes::Aes128;
    use eax::aead::{Aead, Payload};
    use eax::aead::generic_array::GenericArray;
    use eax::{Eax, NewAead};

    let mut iv_struct = Vec::with_capacity(client_alpha.len() + server_beta.len());
    for i in 0..client_alpha.len() {
        iv_struct.push(shared_secret[i] ^ client_alpha[i]);
    }
    iv_struct.extend_from_slice(server_beta);

    let mut tmp_to_hash = Vec::with_capacity(6 + iv_struct.len());
    tmp_to_hash.push(if is_server_to_client { 0x30 } else { 0x31 });
    tmp_to_hash.push(packet_type_raw & 0x0F);
    tmp_to_hash.extend_from_slice(&generation_id.to_be_bytes());
    tmp_to_hash.extend_from_slice(&iv_struct);

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

pub fn generate_server_proof(private_key_bytes: &[u8], license_bytes: &[u8]) -> Option<Vec<u8>> {
    use p256::ecdsa::{SigningKey, signature::Signer};
    let signing_key = SigningKey::from_slice(private_key_bytes).ok()?;
    let signature: p256::ecdsa::Signature = signing_key.sign(license_bytes);
    Some(signature.to_der().to_bytes().into())
}

pub fn load_protocol_key() -> Option<(String, Vec<u8>)> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    let file = File::open("protocol_key.txt").ok()?;
    let reader = BufReader::new(file);

    let mut chain_b64 = String::new();
    let mut root_key_prv = Vec::new();

    for line in reader.lines() {
        let line = line.ok()?;
        if line.starts_with("chain:") {
            chain_b64 = line.trim_start_matches("chain:").trim().to_string();
        } else if line.starts_with("root_key_prv:") {
            let prv_b64 = line.trim_start_matches("root_key_prv:").trim();
            root_key_prv = BASE64.decode(prv_b64).ok()?;
        }
    }

    if chain_b64.is_empty() || root_key_prv.len() != 32 {
        return None;
    }

    Some((chain_b64, root_key_prv))
}
