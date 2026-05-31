use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use p256::SecretKey;

fn main() {
    let prv_b64 = "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s=";
    let prv_bytes = base64_std.decode(prv_b64).unwrap();
    
    let mut prv_le = prv_bytes.clone();
    prv_le.reverse();
    let sk = SecretKey::from_slice(&prv_le).unwrap();
    let pk = sk.public_key();
    let pk_bytes = pk.to_sec1_bytes();
    
    println!("Full SEC1 public key (len {}): {:02X?}", pk_bytes.len(), pk_bytes);
    let y_parity = pk_bytes[64] % 2;
    println!("Y parity: {}", y_parity);
}
