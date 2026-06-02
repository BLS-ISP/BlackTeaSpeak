use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use std::ops::Mul;
use sha2::{Sha512, Digest};

fn import_hash(hash: &[u8]) -> Scalar {
    let mut buffer = [0u8; 64];
    buffer[0..32].copy_from_slice(&hash[0..32]);
    buffer[0] &= 0xF8;
    buffer[31] &= 0x3F;
    buffer[31] |= 0x40;
    Scalar::from_bytes_mod_order_wide(&buffer)
}

fn main() {
    let d1_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo="; // root_key_prv (parent)
    let d_base_b64 = "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="; // Server identity base key

    let d1_bytes = BASE64_STANDARD.decode(d1_b64).unwrap();
    let d_base_bytes = BASE64_STANDARD.decode(d_base_b64).unwrap();

    let d1 = Scalar::from_bytes_mod_order(d1_bytes.try_into().unwrap());
    let d_base = Scalar::from_bytes_mod_order(d_base_bytes.try_into().unwrap());

    let target_pub_hex = "7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8";
    let target_bytes = hex::decode(target_pub_hex).unwrap();

    // Raw bytes of new Entry 2 (excluding the 0x00 separator)
    let entry2_bytes = hex::decode("7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a80218a459801c49bf800600000000466c6f7269616e204d617468696173204265726b656d6569657200").unwrap();

    let mut hasher = Sha512::new();
    hasher.update(&entry2_bytes);
    let entry_hash = hasher.finalize();

    let hash_scalar = import_hash(&entry_hash);

    let candidates = vec![
        ("d1 + d_base * hash", d1 + (d_base * hash_scalar)),
        ("d1 - d_base * hash", d1 - (d_base * hash_scalar)),
        ("-d1 + d_base * hash", -d1 + (d_base * hash_scalar)),
        ("-d1 - d_base * hash", -d1 - (d_base * hash_scalar)),
        ("d_base * hash - d1", (d_base * hash_scalar) - d1),
    ];

    for (name, d2) in candidates {
        let d2_bytes = d2.to_bytes();
        let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&d2);
        let pk_bytes = pk_point.compress().to_bytes();
        
        let mut flipped_pk = pk_bytes;
        flipped_pk[31] ^= 0x80;
        
        if pk_bytes == target_bytes.as_slice() {
            println!("MATCH for {}: perfect match! (unflipped)", name);
            println!("Private key (b64): {}", BASE64_STANDARD.encode(&d2_bytes));
        } else if flipped_pk == target_bytes.as_slice() {
            println!("MATCH for {}: matches with flipped sign bit!", name);
            println!("Private key (b64): {}", BASE64_STANDARD.encode(&d2_bytes));
        }
    }
    println!("Parity check completed.");
}

