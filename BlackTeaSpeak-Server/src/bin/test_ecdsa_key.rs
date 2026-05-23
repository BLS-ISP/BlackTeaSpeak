use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use p256::SecretKey;

fn main() {
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    
    let prv_bytes = base64_std.decode(prv_b64).unwrap();
    let pbl_bytes = base64_std.decode(pbl_b64).unwrap();
    
    println!("Private Key: {}", hex::encode(&prv_bytes));
    println!("Public Key X: {}", hex::encode(&pbl_bytes));
    
    let mut prv_le = prv_bytes.clone();
    prv_le.reverse();
    let sk = SecretKey::from_slice(&prv_le).unwrap();
    let pk = sk.public_key();
    let pk_bytes = pk.to_sec1_bytes();
    
    // In uncompressed format (0x04), bytes 1..33 are X, 33..65 are Y
    let x_bytes = &pk_bytes[1..33];
    println!("Derived X: {}", hex::encode(x_bytes));
    
    if x_bytes == pbl_bytes.as_slice() {
        println!("MATCH! It is indeed NIST P-256 (secp256r1)");
    } else {
        println!("NO MATCH");
    }
}
