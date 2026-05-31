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
    let chain_b64 = "AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAA==";
    let crypto_chain = BASE64_STANDARD.decode(chain_b64).unwrap();
    
    // Original Anonymous chain entry (skipping the first 0x01 ExportChain type)
    let entry_bytes = &crypto_chain[1..];
    
    // Original master root public key
    let ts3_master_pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    let ts3_master_pbl_bytes = BASE64_STANDARD.decode(ts3_master_pbl_b64).unwrap();
    let mut master_pbl_compressed = curve25519_dalek::edwards::CompressedEdwardsY([0u8; 32]);
    master_pbl_compressed.0.copy_from_slice(&ts3_master_pbl_bytes);
    let master_pbl = master_pbl_compressed.decompress().unwrap();
    
    // Original Anonymous base public key (stored at entry_bytes[1..33])
    let mut base_pk_compressed = curve25519_dalek::edwards::CompressedEdwardsY([0u8; 32]);
    base_pk_compressed.0.copy_from_slice(&entry_bytes[1..33]);
    let base_pk = base_pk_compressed.decompress().unwrap();
    
    // Original derived public key
    let ts3_anon_pbl_b64 = "lV05ShflEHNMoGvfXTkPRSQsC2iarQ29Ru8WDzLxxTM=";
    let ts3_anon_pbl_bytes = BASE64_STANDARD.decode(ts3_anon_pbl_b64).unwrap();
    
    // Case 1: Hashing with the first 0x00 byte (entire entry_bytes)
    {
        let mut hasher = Sha512::new();
        hasher.update(&entry_bytes);
        let hash = hasher.finalize();
        let hash_scalar = import_hash(&hash);
        let derived_point = master_pbl + (&base_pk * &hash_scalar);
        let derived_bytes = derived_point.compress().to_bytes();
        
        println!("Case 1 (entire entry): {}", hex::encode(&derived_bytes));
        if derived_bytes == ts3_anon_pbl_bytes.as_slice() {
            println!("MATCH Case 1!");
        }
    }
    
    // Case 2: Hashing without the first 0x00 byte (entry_bytes[1..])
    {
        let mut hasher = Sha512::new();
        hasher.update(&entry_bytes[1..]);
        let hash = hasher.finalize();
        let hash_scalar = import_hash(&hash);
        let derived_point = master_pbl + (&base_pk * &hash_scalar);
        let derived_bytes = derived_point.compress().to_bytes();
        
        println!("Case 2 (skipping 0x00): {}", hex::encode(&derived_bytes));
        if derived_bytes == ts3_anon_pbl_bytes.as_slice() {
            println!("MATCH Case 2!");
        }
    }
}
