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
    let d0_candidates = vec![
        ("QCUIVtjak...", "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s="),
        ("oCPmMAvfk...", "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g="),
        ("fJJ7Xbr9a...", "fJJ7Xbr9aOc8KMC2QKEmbB1uWtLJmKI2G9Ckb4fumgo="),
        ("YARwqypuX...", "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo="),
    ];

    let d_base_candidates = vec![
        ("6BNOfdZZ...", "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="),
        ("0UHWCIfq...", "0UHWCIfqReCJJ1yruDGBaW3TymM5pe0soijb1By2EVc="),
        ("YBNOfdZZ...", "YBNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="),
    ];

    // Raw entry bytes from Chain 2
    let entry0_bytes = hex::decode("00af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80000000255465616d537065616b2053797374656d7320476d624800").unwrap();
    let entry1_bytes = hex::decode("00d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d00189b68d53eaae569000000245465616d537065616b2073797374656d7320476d624800").unwrap();
    let entry2_bytes = hex::decode("00e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae05189b68d53eaae569").unwrap();

    let pk0_expected = hex::decode("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429").unwrap();
    let pk1_expected = hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap();
    let pk2_expected = hex::decode("e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae").unwrap();

    for &(d0_name, d0_b64) in &d0_candidates {
        let d0_bytes = BASE64_STANDARD.decode(d0_b64).unwrap();
        
        // Try d0 both as raw scalar and hashed/clamped seed
        for &hash_d0 in &[false, true] {
            let d0 = if hash_d0 {
                let mut hasher = Sha512::new();
                hasher.update(&d0_bytes);
                let seed_hash = hasher.finalize();
                let mut scalar_bytes = [0u8; 32];
                scalar_bytes.copy_from_slice(&seed_hash[0..32]);
                scalar_bytes[0] &= 248;
                scalar_bytes[31] &= 127;
                scalar_bytes[31] |= 64;
                Scalar::from_bytes_mod_order(scalar_bytes)
            } else {
                Scalar::from_bytes_mod_order(d0_bytes.clone().try_into().unwrap())
            };

            for &(db_name, db_b64) in &d_base_candidates {
                let db_bytes = BASE64_STANDARD.decode(db_b64).unwrap();
                let d_base = Scalar::from_bytes_mod_order(db_bytes.try_into().unwrap());

                // Test SHA-512 vs SHA-256
                for &use_sha256 in &[false, true] {
                    for skip0 in &[false, true] {
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

                        for sign0 in &[-1f64, 1f64] {
                            let d1 = if *sign0 > 0.0 { d0 + (d_base * h0) } else { d0 - (d_base * h0) };
                            let pk1_derived = (&ED25519_BASEPOINT_TABLE).mul(&d1).compress().to_bytes();
                            let mut flipped_pk1 = pk1_derived;
                            flipped_pk1[31] ^= 0x80;

                            if pk1_derived == pk0_expected.as_slice() || flipped_pk1 == pk0_expected.as_slice() {
                                println!("MATCH Level 1 (Root -> Entry 0)!!! d0={}, hash_d0={}, d_base={}", d0_name, hash_d0, db_name);
                                println!("  SHA256: {}", use_sha256);
                                println!("  skip0: {}", skip0);
                                println!("  sign0: {}", sign0);
                                println!("  Derived Entry 0 private key (b64): {}", BASE64_STANDARD.encode(d1.to_bytes()));

                                // Level 2
                                for skip1 in &[false, true] {
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

                                    for sign1 in &[-1f64, 1f64] {
                                        let d2 = if *sign1 > 0.0 { d1 + (d_base * h1) } else { d1 - (d_base * h1) };
                                        let pk2_derived = (&ED25519_BASEPOINT_TABLE).mul(&d2).compress().to_bytes();
                                        let mut flipped_pk2 = pk2_derived;
                                        flipped_pk2[31] ^= 0x80;

                                        if pk2_derived == pk1_expected.as_slice() || flipped_pk2 == pk1_expected.as_slice() {
                                            println!("  MATCH Level 2 (Entry 0 -> Entry 1 / Server Identity)!!!");
                                            println!("    skip1: {}", skip1);
                                            println!("    sign1: {}", sign1);
                                            println!("    Derived Entry 1 private key (b64): {}", BASE64_STANDARD.encode(d2.to_bytes()));

                                            // Level 3
                                            for skip2 in &[false, true] {
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

                                                for sign2 in &[-1f64, 1f64] {
                                                    let d3 = if *sign2 > 0.0 { d2 + (d_base * h2) } else { d2 - (d_base * h2) };
                                                    let pk3_derived = (&ED25519_BASEPOINT_TABLE).mul(&d3).compress().to_bytes();
                                                    let mut flipped_pk3 = pk3_derived;
                                                    flipped_pk3[31] ^= 0x80;

                                                    if pk3_derived == pk2_expected.as_slice() || flipped_pk3 == pk2_expected.as_slice() {
                                                        println!("    MATCH Level 3 (Entry 1 -> Entry 2 / License Sign Key)!!!");
                                                        println!("      skip2: {}", skip2);
                                                        println!("      sign2: {}", sign2);
                                                        println!("      Derived Entry 2 Private Key (base64): {}", BASE64_STANDARD.encode(d3.to_bytes()));
                                                        return;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!("Derivation search completed.");
}
