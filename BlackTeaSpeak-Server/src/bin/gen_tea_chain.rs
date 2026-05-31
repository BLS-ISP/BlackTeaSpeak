use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use sha2::{Sha512, Digest};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

fn import_hash(hash: &[u8]) -> Scalar {
    let mut buffer = [0u8; 64];
    buffer[0..32].copy_from_slice(&hash[0..32]);
    buffer[0] &= 0xF8;
    buffer[31] &= 0x3F;
    buffer[31] |= 0x40;
    Scalar::from_bytes_mod_order_wide(&buffer)
}

fn main() {
    let mut rng = rand::thread_rng();

    // 1. TeamSpeak Root Private Key (scalar)
    let ts3_root_prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let ts3_root_pbl_b64 = "lV05ShflEHNMoGvfXTkPRSQsC2iarQ29Ru8WDzLxxTM=";
    
    let private_ts3_root = BASE64_STANDARD.decode(ts3_root_prv_b64).unwrap();
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&private_ts3_root);
    let root_prv = Scalar::from_bytes_mod_order(prv_array);

    let public_ts3_root = BASE64_STANDARD.decode(ts3_root_pbl_b64).unwrap();

    // 2. Generate Ephemeral Base Private Key
    let mut rand_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rng, &mut rand_bytes);
    let ephem_base_prv = Scalar::from_bytes_mod_order(rand_bytes);
    
    // 3. Derive Ephemeral Public Key
    use std::ops::Mul;
    let ephem_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&ephem_base_prv);
    let ephem_pk_bytes = ephem_pk_point.compress().to_bytes();

    // 4. Construct entry bytes
    let mut entry_bytes = Vec::new();
    entry_bytes.push(0x00);
    entry_bytes.extend_from_slice(&ephem_pk_bytes);
    entry_bytes.push(0x20); // Type Ephemeral
    
    // Realistic timestamps: current time minus 1 month, and plus 5 years
    let begin: u32 = 1714500000; // ~May 1, 2024
    let end: u32 = 1872200000;   // ~April 2029
    entry_bytes.extend_from_slice(&begin.to_be_bytes());
    entry_bytes.extend_from_slice(&end.to_be_bytes());

    // 5. Hash entry
    let mut hasher = Sha512::new();
    hasher.update(&entry_bytes);
    let entry_hash = hasher.finalize();

    // 6. importHash
    let hash_scalar = import_hash(&entry_hash);

    // 7. Derive Ephemeral Derived Private Key
    // sc_muladd(buffer, this->entries[index]->key.privateKeyData, (uint8_t*) importHash(this->entries[index]->hash()).data(), buffer);
    // Which means: derived = (base_prv * hash_scalar + parent_prv) mod l
    let derived_prv = (ephem_base_prv * hash_scalar) + root_prv;

    // 8. Construct Exported Chain
    let mut exported_chain = Vec::new();
    exported_chain.push(0x01); // ExportChain type
    exported_chain.extend_from_slice(&entry_bytes);

    println!("chain: {}", BASE64_STANDARD.encode(&exported_chain));
    println!("root_key_pbl: {}", BASE64_STANDARD.encode(&public_ts3_root));
    println!("ephemeral_prv: {}", BASE64_STANDARD.encode(derived_prv.as_bytes()));
    println!("ephemeral_pk: {}", BASE64_STANDARD.encode(&ephem_pk_bytes));
}
