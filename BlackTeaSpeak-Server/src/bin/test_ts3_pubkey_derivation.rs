use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::ops::Mul;

fn main() {
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let pbl_b64 = "0b7D7TrvqlnnyJh0uHI5YVu3t9GVHClGY3OCP+jvlU0=";
    
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let pbl_bytes = BASE64_STANDARD.decode(pbl_b64).unwrap();
    
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);
    
    // Method 1: Raw scalar
    {
        let scalar = Scalar::from_bytes_mod_order(prv_array);
        let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
        let pk_bytes = pk_point.compress().to_bytes();
        let mut flipped_pk = pk_bytes;
        flipped_pk[31] ^= 0x80;
        println!("Method 1 (Raw scalar) pubkey (unflipped): {}", BASE64_STANDARD.encode(&pk_bytes));
        println!("Method 1 (Raw scalar) pubkey (flipped):   {}", BASE64_STANDARD.encode(&flipped_pk));
        if pk_bytes == pbl_bytes.as_slice() {
            println!("MATCH Method 1 (Raw scalar) unflipped!");
        } else if flipped_pk == pbl_bytes.as_slice() {
            println!("MATCH Method 1 (Raw scalar) flipped!");
        }
    }
    
    // Method 2: Clamped raw scalar
    {
        let mut clamped = prv_array;
        clamped[0] &= 248;
        clamped[31] &= 127;
        clamped[31] |= 64;
        let scalar = Scalar::from_bytes_mod_order(clamped);
        let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
        let pk_bytes = pk_point.compress().to_bytes();
        let mut flipped_pk = pk_bytes;
        flipped_pk[31] ^= 0x80;
        println!("Method 2 (Clamped raw scalar) pubkey (unflipped): {}", BASE64_STANDARD.encode(&pk_bytes));
        println!("Method 2 (Clamped raw scalar) pubkey (flipped):   {}", BASE64_STANDARD.encode(&flipped_pk));
        if pk_bytes == pbl_bytes.as_slice() {
            println!("MATCH Method 2 (Clamped raw scalar) unflipped!");
        } else if flipped_pk == pbl_bytes.as_slice() {
            println!("MATCH Method 2 (Clamped raw scalar) flipped!");
        }
    }
    
    // Method 3: SHA-512 hashed + clamped (Standard Ed25519 seed)
    {
        use sha2::{Sha512, Digest};
        let mut hash = Sha512::digest(&prv_array);
        hash[0] &= 248;
        hash[31] &= 127;
        hash[31] |= 64;
        let mut scalar_bytes = [0u8; 32];
        scalar_bytes.copy_from_slice(&hash[0..32]);
        let scalar = Scalar::from_bytes_mod_order(scalar_bytes);
        let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
        let pk_bytes = pk_point.compress().to_bytes();
        let mut flipped_pk = pk_bytes;
        flipped_pk[31] ^= 0x80;
        println!("Method 3 (SHA-512 hashed + clamped) pubkey (unflipped): {}", BASE64_STANDARD.encode(&pk_bytes));
        println!("Method 3 (SHA-512 hashed + clamped) pubkey (flipped):   {}", BASE64_STANDARD.encode(&flipped_pk));
        if pk_bytes == pbl_bytes.as_slice() {
            println!("MATCH Method 3 (SHA-512 hashed + clamped seed) unflipped!");
        } else if flipped_pk == pbl_bytes.as_slice() {
            println!("MATCH Method 3 (SHA-512 hashed + clamped seed) flipped!");
        }
    }
    
    println!("Derivation tests completed.");
}
