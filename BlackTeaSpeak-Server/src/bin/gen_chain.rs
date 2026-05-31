use std::time::{SystemTime, UNIX_EPOCH};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;

fn main() {
    let chain_b64 = "AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAA==";
    let root_key_prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let root_key_pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";

    let mut crypto_chain = BASE64.decode(chain_b64).unwrap();
    let new_chain_b64 = BASE64.encode(&crypto_chain);
    let new_root_key_prv = root_key_prv_b64.to_string();
    let new_root_key_pbl = root_key_pbl_b64.to_string();

    println!("chain: {}", new_chain_b64);
    println!("root_key_prv: {}", new_root_key_prv);
    println!("root_key_pbl: {}", new_root_key_pbl);
}
