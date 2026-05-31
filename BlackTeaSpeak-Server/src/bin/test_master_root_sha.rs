use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Digest};
use std::ops::Mul;

fn main() {
    let prv_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g=";
    let pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let pbl_bytes = BASE64_STANDARD.decode(pbl_b64).unwrap();
    
    // Standard Ed25519 private key to scalar derivation
    let mut hash = Sha512::digest(&prv_bytes);
    hash[0] &= 248;
    hash[31] &= 127;
    hash[31] |= 64;
    
    let mut scalar_bytes = [0u8; 32];
    scalar_bytes.copy_from_slice(&hash[0..32]);
    let scalar = Scalar::from_bytes_mod_order(scalar_bytes);
    
    let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
    let pk_bytes = pk_point.compress().to_bytes();
    
    println!("Public key in file:  {}", hex::encode(&pbl_bytes));
    println!("Derived via SHA-512: {}", hex::encode(&pk_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH!");
    } else {
        println!("NO MATCH!");
    }
}
