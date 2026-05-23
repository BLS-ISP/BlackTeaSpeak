use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use ed25519_dalek::{SecretKey, SigningKey};

fn main() {
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    
    let prv_bytes = base64_std.decode(prv_b64).unwrap();
    let pbl_bytes = base64_std.decode(pbl_b64).unwrap();
    
    println!("Private Key: {}", hex::encode(&prv_bytes));
    println!("Public Key: {}", hex::encode(&pbl_bytes));
    
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&prv_bytes);
    
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let verifying_key = signing_key.verifying_key();
    let pk_bytes = verifying_key.to_bytes();
    
    println!("Derived PK: {}", hex::encode(&pk_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH! It is indeed Ed25519!");
    } else {
        println!("NO MATCH");
    }
}
