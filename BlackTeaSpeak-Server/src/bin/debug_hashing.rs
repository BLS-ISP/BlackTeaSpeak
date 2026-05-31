use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Digest};
use std::ops::Mul;

fn import_hash(hash: &[u8]) -> Scalar {
    let mut buffer = [0u8; 64];
    buffer[0..32].copy_from_slice(&hash[0..32]);
    buffer[0] &= 0xF8;
    buffer[31] &= 0x3F;
    buffer[31] |= 0x40;
    Scalar::from_bytes_mod_order_wide(&buffer)
}

fn main() {
    // Let's replicate gen_new_anonymous_chain logic in memory:
    let ts3_master_prv_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g=";
    let ts3_master_prv_bytes = BASE64_STANDARD.decode(ts3_master_prv_b64).unwrap();
    let mut master_prv_array = [0u8; 32];
    master_prv_array.copy_from_slice(&ts3_master_prv_bytes);
    let master_prv = Scalar::from_bytes_mod_order(master_prv_array);
    
    let master_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&master_prv);
    let master_pk_bytes = master_pk_point.compress().to_bytes();

    let anon_base_prv = Scalar::from_bytes_mod_order([1u8; 32]); // Fixed for debugging
    let anon_base_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&anon_base_prv);
    let anon_base_pk_bytes = anon_base_pk_point.compress().to_bytes();

    let mut entry_bytes = Vec::new();
    entry_bytes.push(0x00);
    entry_bytes.extend_from_slice(&anon_base_pk_bytes);
    entry_bytes.push(0x02);
    
    const TIMESTAMP_OFFSET: u64 = 1356998400;
    let begin_time: u64 = 1767225600;
    let end_time: u64 = 2398291200;
    
    let begin = (begin_time - TIMESTAMP_OFFSET) as u32;
    let end = (end_time - TIMESTAMP_OFFSET) as u32;
    entry_bytes.extend_from_slice(&begin.to_be_bytes());
    entry_bytes.extend_from_slice(&end.to_be_bytes());
    entry_bytes.push(0x07);
    let slots: u32 = 32;
    entry_bytes.extend_from_slice(&slots.to_be_bytes());
    entry_bytes.extend_from_slice(b"Anonymous\0");

    // Generator hashing
    let mut hasher = Sha512::new();
    hasher.update(&entry_bytes);
    let entry_hash = hasher.finalize();
    let hash_scalar = import_hash(&entry_hash);

    let anon_derived_prv = (anon_base_prv * hash_scalar) + master_prv;
    let anon_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&anon_derived_prv);
    let anon_pk_bytes = anon_pk_point.compress().to_bytes();

    // Verifier hashing (simulating verify_new_anonymous_chain)
    let mut base_pk_compressed = curve25519_dalek::edwards::CompressedEdwardsY([0u8; 32]);
    base_pk_compressed.0.copy_from_slice(&entry_bytes[1..33]);
    let base_pk = base_pk_compressed.decompress().unwrap();
    
    let mut master_pbl_compressed = curve25519_dalek::edwards::CompressedEdwardsY([0u8; 32]);
    master_pbl_compressed.0.copy_from_slice(&master_pk_bytes);
    let master_pbl = master_pbl_compressed.decompress().unwrap();

    let verifier_point = master_pbl + (&base_pk * &hash_scalar);
    let verifier_bytes = verifier_point.compress().to_bytes();

    println!("anon_base_pk_bytes: {}", hex::encode(&anon_base_pk_bytes));
    println!("base_pk in verifier: {}", hex::encode(&entry_bytes[1..33]));
    println!("hash_scalar: {}", hex::encode(hash_scalar.as_bytes()));
    println!("anon_pk_bytes (generator): {}", hex::encode(&anon_pk_bytes));
    println!("verifier_bytes (verifier):  {}", hex::encode(&verifier_bytes));
}
