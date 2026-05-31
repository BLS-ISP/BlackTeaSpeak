use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::ops::Mul;

fn main() {
    // Derived Anonymous key pair in protocol_key.txt:
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let pbl_b64 = "lV05ShflEHNMoGvfXTkPRSQsC2iarQ29Ru8WDzLxxTM=";
    
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let pbl_bytes = BASE64_STANDARD.decode(pbl_b64).unwrap();
    
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);
    
    let scalar = Scalar::from_bytes_mod_order(prv_array);
    
    let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
    let pk_bytes = pk_point.compress().to_bytes();
    
    println!("Public key in file:  {}", hex::encode(&pbl_bytes));
    println!("Derived from scalar: {}", hex::encode(&pk_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH! Scalar basepoint multiplication derives the public key correctly!");
    } else {
        println!("NO MATCH!");
    }
}
