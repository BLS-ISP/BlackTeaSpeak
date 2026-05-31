use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::ops::Mul;

fn main() {
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let pbl_bytes = BASE64_STANDARD.decode(pbl_b64).unwrap();
    
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);
    
    let scalar = Scalar::from_bytes_mod_order(prv_array);
    let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
    let mut pk_bytes = pk_point.compress().to_bytes();
    
    println!("Standard Edwards PK: {}", hex::encode(&pk_bytes));
    
    // TeamSpeak flips the sign bit (bit 7 of the last byte)
    pk_bytes[31] ^= 0x80;
    println!("TeamSpeak Flipping PK: {}", hex::encode(&pk_bytes));
    println!("Expected PK in file:  {}", hex::encode(&pbl_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH! TeamSpeak key derivation flips the public key's sign bit!");
    } else {
        println!("NO MATCH!");
    }
}
