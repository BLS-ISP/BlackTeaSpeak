use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use curve25519_dalek::constants::ED25519_BASEPOINT_POINT;
use curve25519_dalek::scalar::Scalar;

fn main() {
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    
    let prv_bytes = base64_std.decode(prv_b64).unwrap();
    let pbl_bytes = base64_std.decode(pbl_b64).unwrap();
    
    println!("Private Key: {}", hex::encode(&prv_bytes));
    println!("Public Key: {}", hex::encode(&pbl_bytes));
    
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&prv_array);
    let pk_bytes = signing_key.verifying_key().to_bytes();
    
    println!("Derived PK: {}", hex::encode(&pk_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH! The private key is exactly the Ed25519 scalar!");
    } else {
        println!("NO MATCH");
    }
}
