use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Digest};
use std::ops::Mul;

fn import_hash(hash: &[u8]) -> Scalar {
    let mut buffer = [0u8; 64];
    buffer[0..32].copy_from_slice(&hash[0..32]);
    buffer[0] &= 0xF8;
    buffer[31] &= 0x3F;
    buffer[31] |= 0x40;
    Scalar::from_bytes_mod_order_wide(&buffer)
}

fn main() {
    let d0_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo="; // Master Root
    let d_base_b64 = "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="; // Base key

    let d0_bytes = BASE64_STANDARD.decode(d0_b64).unwrap();
    let d_base_bytes = BASE64_STANDARD.decode(d_base_b64).unwrap();

    let d0 = Scalar::from_bytes_mod_order(d0_bytes.try_into().unwrap());
    let d_base = Scalar::from_bytes_mod_order(d_base_bytes.try_into().unwrap());

    // Entry 0 raw bytes
    let entry0_bytes = hex::decode("00af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80000000255465616d537065616b2053797374656d7320476d624800").unwrap();
    let target_pub_hex = "af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429";
    let target_bytes = hex::decode(target_pub_hex).unwrap();

    for skip0 in &[false, true] {
        let mut hasher = Sha512::new();
        if *skip0 {
            hasher.update(&entry0_bytes[1..]);
        } else {
            hasher.update(&entry0_bytes);
        }
        let hash0 = hasher.finalize();
        let h0 = import_hash(&hash0);

        for sign0 in &[-1f64, 1f64] {
            let d1 = if *sign0 > 0.0 { d0 + (d_base * h0) } else { d0 - (d_base * h0) };
            let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&d1);
            let pk_bytes = pk_point.compress().to_bytes();
            let mut flipped_pk = pk_bytes;
            flipped_pk[31] ^= 0x80;

            if pk_bytes == target_bytes.as_slice() {
                println!("MATCH for Entry 0 (unflipped): skip0={}, sign0={}", skip0, sign0);
            } else if flipped_pk == target_bytes.as_slice() {
                println!("MATCH for Entry 0 (flipped): skip0={}, sign0={}", skip0, sign0);
            }
        }
    }
    println!("Level 1 test completed.");
}
