use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Sha256, Digest};
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
    // 1. Load root_key_prv from protocol_key.txt
    let root_key_prv = {
        let content = std::fs::read_to_string("protocol_key.txt").unwrap();
        let mut root_key_prv = Vec::new();
        for line in content.lines() {
            if line.starts_with("root_key_prv:") {
                let prv_b64 = line.trim_start_matches("root_key_prv:").trim();
                root_key_prv = BASE64_STANDARD.decode(prv_b64).unwrap();
            }
        }
        root_key_prv
    };

    // d1 is identity_sec loaded from root_key_prv
    let d1 = Scalar::from_bytes_mod_order(root_key_prv.try_into().unwrap());
    
    // Base private key d_base
    let d_base_b64 = "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM=";
    let d_base = Scalar::from_bytes_mod_order(BASE64_STANDARD.decode(d_base_b64).unwrap().try_into().unwrap());

    // Target public key: Entry 2 (License Sign entry) pubkey:
    // e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae
    let target_pub = hex::decode("e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae").unwrap();

    // Raw bytes of Entry 2
    let entry2_bytes = hex::decode("00e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae05189b68d53eaae569").unwrap();
    println!("Entry 2 bytes len: {}", entry2_bytes.len());

    for &use_sha256 in &[false, true] {
        for &skip_zero in &[false, true] {
            let hash = if use_sha256 {
                let mut hasher = Sha256::new();
                if skip_zero {
                    hasher.update(&entry2_bytes[1..]);
                } else {
                    hasher.update(&entry2_bytes);
                }
                hasher.finalize().to_vec()
            } else {
                let mut hasher = Sha512::new();
                if skip_zero {
                    hasher.update(&entry2_bytes[1..]);
                } else {
                    hasher.update(&entry2_bytes);
                }
                hasher.finalize().to_vec()
            };
            let h = import_hash(&hash);

            let candidates = vec![
                ("d1 + (d_base * h)", d1 + (d_base * h)),
                ("d1 - (d_base * h)", d1 - (d_base * h)),
                ("(d_base * h) - d1", (d_base * h) - d1),
                ("-d1 - (d_base * h)", -d1 - (d_base * h)),
            ];

            for (name, d2) in candidates {
                let pk = (&ED25519_BASEPOINT_TABLE).mul(&d2).compress().to_bytes();
                let mut flipped_pk = pk;
                flipped_pk[31] ^= 0x80;

                if pk == target_pub.as_slice() {
                    println!("MATCH (unflipped)!!! sha256={}, skip_zero={}, formulation={}", use_sha256, skip_zero, name);
                    println!("  d2 private key (b64): {}", BASE64_STANDARD.encode(d2.to_bytes()));
                } else if flipped_pk == target_pub.as_slice() {
                    println!("MATCH (flipped)!!! sha256={}, skip_zero={}, formulation={}", use_sha256, skip_zero, name);
                    println!("  d2 private key (b64): {}", BASE64_STANDARD.encode(d2.to_bytes()));
                }
            }
        }
    }
}
