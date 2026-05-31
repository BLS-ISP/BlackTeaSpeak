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
    let chain_b64 = "AQBJKvuNOpCxwVJnIkcHUHwUZ+E6SgHvL6ATtFr1A5kPsgIYc5IAPhDeAAcAAAAgQW5vbnltb3VzAA==";
    let root_key_prv_b64 = "fJJ7Xbr9aOc8KMC2QKEmbB1uWtLJmKI2G9Ckb4fumgo=";
    let root_key_pbl_b64 = "a93rXNA2mwWLi9z8qYSPZeFa87kZWLvFKn/BpUwCN3k=";
    
    let crypto_chain = BASE64_STANDARD.decode(chain_b64).unwrap();
    let entry_bytes = &crypto_chain[1..];
    
    let ts3_master_pbl_b64 = "4Cnl/AqNKrLpYAfucskQMG0kH38YjDIq2bpg9PHnUAs=";
    let ts3_master_pbl_bytes = BASE64_STANDARD.decode(ts3_master_pbl_b64).unwrap();
    let mut master_pbl_compressed = curve25519_dalek::edwards::CompressedEdwardsY([0u8; 32]);
    master_pbl_compressed.0.copy_from_slice(&ts3_master_pbl_bytes);
    let master_pbl = master_pbl_compressed.decompress().unwrap();
    
    let mut base_pk_compressed = curve25519_dalek::edwards::CompressedEdwardsY([0u8; 32]);
    base_pk_compressed.0.copy_from_slice(&entry_bytes[1..33]);
    let base_pk = base_pk_compressed.decompress().unwrap();
    
    let mut hasher = Sha512::new();
    hasher.update(&entry_bytes[1..]);
    let hash = hasher.finalize();
    let hash_scalar = import_hash(&hash);
    
    let derived_point = master_pbl + (&base_pk * &hash_scalar);
    let derived_bytes = derived_point.compress().to_bytes();
    
    let expected_pbl_bytes = BASE64_STANDARD.decode(root_key_pbl_b64).unwrap();
    
    println!("Derived Public Key:  {}", hex::encode(&derived_bytes));
    println!("Expected Public Key: {}", hex::encode(&expected_pbl_bytes));
    
    if derived_bytes == expected_pbl_bytes.as_slice() {
        println!("VERIFICATION SUCCESSFUL! The new Anonymous chain is mathematically valid and fully correct!");
    } else {
        println!("VERIFICATION FAILED!");
    }
}
