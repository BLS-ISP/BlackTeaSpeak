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
    let d0_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g="; // Master Root
    let d_base_b64 = "0UHWCIfqReCJJ1yruDGBaW3TymM5pe0soijb1By2EVc="; // Base key

    let d0_bytes = BASE64_STANDARD.decode(d0_b64).unwrap();
    let d_base_bytes = BASE64_STANDARD.decode(d_base_b64).unwrap();

    let d0 = Scalar::from_bytes_mod_order(d0_bytes.try_into().unwrap());
    let d_base = Scalar::from_bytes_mod_order(d_base_bytes.try_into().unwrap());

    // Raw entry bytes from new Chain 1
    let entry0_bytes = hex::decode("00af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80000000255465616d537065616b2053797374656d7320476d624800").unwrap();
    let entry1_bytes = hex::decode("00d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d00189b68d53eaae569000000245465616d537065616b2073797374656d7320476d624800").unwrap();
    let entry2_bytes = hex::decode("007c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a80218a459801c49bf800600000000466c6f7269616e204d617468696173204265726b656d6569657200").unwrap();

    let target_pub_hex = "7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8";
    let target_bytes = hex::decode(target_pub_hex).unwrap();

    // Test both SHA-512 and SHA-256
    for &use_sha256 in &[false, true] {
        println!("Testing with use_sha256 = {}", use_sha256);
        // Systematically test all configurations of skipping first byte (skip vs keep)
        for skip0 in &[false, true] {
            for skip1 in &[false, true] {
                for skip2 in &[false, true] {
                    // Level 1 (Entry 0) hash
                    let hash0 = if use_sha256 {
                        let mut hasher = Sha256::new();
                        if *skip0 { hasher.update(&entry0_bytes[1..]); } else { hasher.update(&entry0_bytes); }
                        hasher.finalize().to_vec()
                    } else {
                        let mut hasher = Sha512::new();
                        if *skip0 { hasher.update(&entry0_bytes[1..]); } else { hasher.update(&entry0_bytes); }
                        hasher.finalize().to_vec()
                    };
                    let h0 = import_hash(&hash0);

                    // Level 2 (Entry 1) hash
                    let hash1 = if use_sha256 {
                        let mut hasher = Sha256::new();
                        if *skip1 { hasher.update(&entry1_bytes[1..]); } else { hasher.update(&entry1_bytes); }
                        hasher.finalize().to_vec()
                    } else {
                        let mut hasher = Sha512::new();
                        if *skip1 { hasher.update(&entry1_bytes[1..]); } else { hasher.update(&entry1_bytes); }
                        hasher.finalize().to_vec()
                    };
                    let h1 = import_hash(&hash1);

                    // Level 3 (Entry 2) hash
                    let hash2 = if use_sha256 {
                        let mut hasher = Sha256::new();
                        if *skip2 { hasher.update(&entry2_bytes[1..]); } else { hasher.update(&entry2_bytes); }
                        hasher.finalize().to_vec()
                    } else {
                        let mut hasher = Sha512::new();
                        if *skip2 { hasher.update(&entry2_bytes[1..]); } else { hasher.update(&entry2_bytes); }
                        hasher.finalize().to_vec()
                    };
                    let h2 = import_hash(&hash2);

                    // Try derivation signs combinations
                    for sign0 in &[-1f64, 1f64] {
                        for sign1 in &[-1f64, 1f64] {
                            for sign2 in &[-1f64, 1f64] {
                                let d1 = if *sign0 > 0.0 { d0 + (d_base * h0) } else { d0 - (d_base * h0) };
                                let d2 = if *sign1 > 0.0 { d1 + (d_base * h1) } else { d1 - (d_base * h1) };
                                let d3 = if *sign2 > 0.0 { d2 + (d_base * h2) } else { d2 - (d_base * h2) };

                                let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&d3);
                                let pk_bytes = pk_point.compress().to_bytes();
                                let mut flipped_pk = pk_bytes;
                                flipped_pk[31] ^= 0x80;

                                if pk_bytes == target_bytes.as_slice() || flipped_pk == target_bytes.as_slice() {
                                    println!("MATCH FOUND!!!");
                                    println!("  SHA256: {}", use_sha256);
                                    println!("  Skips: skip0={}, skip1={}, skip2={}", skip0, skip1, skip2);
                                    println!("  Signs: sign0={}, sign1={}, sign2={}", sign0, sign1, sign2);
                                    println!("  Derived Private Key (base64): {}", BASE64_STANDARD.encode(d3.to_bytes()));
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!("Derivation failed. No matches found.");
}
