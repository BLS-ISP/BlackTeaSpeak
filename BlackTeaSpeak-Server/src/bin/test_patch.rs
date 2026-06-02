use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use ed25519_dalek::SigningKey;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use std::ops::Mul;

fn main() {
    let key_hex = "afc10cd303c007b90d4e8911ec461f8a69bc235782b2121ddbea91269f3072ad";
    let secret_bytes = hex::decode(key_hex).unwrap();
    
    let signing_key = SigningKey::from_bytes(secret_bytes[..].try_into().unwrap());
    let verifying_key = signing_key.verifying_key();
    let pk_bytes = verifying_key.to_bytes();
    
    let mut flipped_pk = pk_bytes;
    flipped_pk[31] ^= 0x80;
    
    println!("Derived Public Key (Unflipped): {}", hex::encode(&pk_bytes));
    println!("Derived Public Key (Flipped):   {}", hex::encode(&flipped_pk));
    println!("Derived Public Key (base64):    {}", base64_std.encode(&pk_bytes));
}
