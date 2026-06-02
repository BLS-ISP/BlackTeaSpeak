use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use p256::SecretKey;

fn main() {
    let prv_b64 = "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM=";
    let target_hex = "7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8";
    let target_bytes = hex::decode(target_hex).unwrap();
    
    let prv_bytes = base64_std.decode(prv_b64).unwrap();
    println!("Private Key Hex: {}", hex::encode(&prv_bytes));
    println!("Target X Hex:    {}", target_hex);
    
    // 1. Try with Big Endian private key
    if let Ok(sk) = SecretKey::from_slice(&prv_bytes) {
        let pk = sk.public_key();
        let pk_bytes = pk.to_sec1_bytes();
        let x_bytes = &pk_bytes[1..33];
        println!("Derived X (BE):  {}", hex::encode(x_bytes));
        if x_bytes == target_bytes.as_slice() {
            println!("  => MATCH BE!");
        }
    } else {
        println!("Failed to parse as BE SecretKey");
    }
    
    // 2. Try with Little Endian private key (reversed)
    let mut prv_le = prv_bytes.clone();
    prv_le.reverse();
    if let Ok(sk) = SecretKey::from_slice(&prv_le) {
        let pk = sk.public_key();
        let pk_bytes = pk.to_sec1_bytes();
        let x_bytes = &pk_bytes[1..33];
        println!("Derived X (LE):  {}", hex::encode(x_bytes));
        if x_bytes == target_bytes.as_slice() {
            println!("  => MATCH LE!");
        }
    } else {
        println!("Failed to parse as LE SecretKey");
    }
}
