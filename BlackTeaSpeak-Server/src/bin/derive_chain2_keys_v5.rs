use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Digest};
use std::ops::Mul;

struct HierarchyEntry {
    entry_type: u8,
    pubkey: [u8; 32],
    ts_begin: [u8; 4],
    ts_end: [u8; 4],
    body: Vec<u8>,
}

impl HierarchyEntry {
    fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.entry_type);
        data.extend_from_slice(&self.pubkey);
        data.extend_from_slice(&self.ts_begin);
        data.extend_from_slice(&self.ts_end);
        let body_len = self.body.len() as u16;
        data.extend_from_slice(&body_len.to_le_bytes());
        data.extend_from_slice(&self.body);
        data
    }
}

fn import_hash(sha_hash: &[u8]) -> Scalar {
    let mut buffer = [0u8; 64];
    buffer[0..32].copy_from_slice(&sha_hash[0..32]);
    buffer[0] &= 0xF8;
    buffer[31] &= 0x3F;
    buffer[31] |= 0x40;
    Scalar::from_bytes_mod_order_wide(&buffer)
}

fn main() {
    // 1. Target Public Keys
    let target_pub0 = hex::decode("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429").unwrap();
    let target_pub1 = hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap();
    let target_pub2 = hex::decode("e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae").unwrap();

    let target_pub0_arr: [u8; 32] = target_pub0.clone().try_into().unwrap();
    let target_pub1_arr: [u8; 32] = target_pub1.clone().try_into().unwrap();
    let target_pub2_arr: [u8; 32] = target_pub2.clone().try_into().unwrap();

    // 2. Private Keys
    let d0_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo=";
    let d0 = Scalar::from_bytes_mod_order(BASE64_STANDARD.decode(d0_b64).unwrap().try_into().unwrap());

    let d_base_candidates = vec![
        ("6BNOfdZZ...", "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="),
        ("0UHWCIfq...", "0UHWCIfqReCJJ1yruDGBaW3TymM5pe0soijb1By2EVc="),
        ("YBNOfdZZ...", "YBNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="),
    ];

    println!("Target Pub0:      {}", hex::encode(&target_pub0_arr));
    println!("Target Pub1:      {}", hex::encode(&target_pub1_arr));
    println!("Target Pub2:      {}", hex::encode(&target_pub2_arr));

    // A. Reconstruct Entry 2
    let entry2 = HierarchyEntry {
        entry_type: 5,
        pubkey: target_pub2_arr,
        ts_begin: [0x18, 0x9b, 0x68, 0xd5],
        ts_end: [0x3e, 0xaa, 0xe5, 0x69],
        body: Vec::new(),
    };
    let entry2_bytes = entry2.serialize();

    // B. Reconstruct Entry 1
    // Body of Entry 1 contains: Dummy (4) + Issuer (23) + Entry 2 (43) + Padding (35) = 105 bytes
    let mut entry1_body = Vec::new();
    entry1_body.extend_from_slice(&[0x00, 0x00, 0x00, 0x24]); // Dummy for Entry 1
    entry1_body.extend_from_slice(b"TeamSpeak systems GmbH\0"); // Issuer for Entry 1 (23 bytes)
    entry1_body.extend_from_slice(&entry2_bytes); // Entry 2 (43 bytes)
    entry1_body.extend_from_slice(&vec![0u8; 35]); // Padding (35 bytes)

    let entry1 = HierarchyEntry {
        entry_type: 0,
        pubkey: target_pub1_arr,
        ts_begin: [0x18, 0x9b, 0x68, 0xd5],
        ts_end: [0x3e, 0xaa, 0xe5, 0x69],
        body: entry1_body,
    };
    let entry1_bytes = entry1.serialize();

    // C. Reconstruct Entry 0 (Try both variants of Dummy/Issuer)
    let variants = vec![
        (
            "Variant A: Dummy 80 00 00 00, Issuer %TeamSpeak Systems GmbH\\0",
            vec![0x80, 0x00, 0x00, 0x00],
            b"%TeamSpeak Systems GmbH\0".to_vec()
        ),
        (
            "Variant B: Dummy 00 00 00 25, Issuer TeamSpeak Systems GmbH\\0",
            vec![0x00, 0x00, 0x00, 0x25],
            b"TeamSpeak Systems GmbH\0".to_vec()
        ),
    ];

    for (var_name, dummy, issuer) in variants {
        println!("\nTesting {}", var_name);
        
        let mut entry0_body = Vec::new();
        entry0_body.extend_from_slice(&dummy);
        entry0_body.extend_from_slice(&issuer);
        
        // Truncate Entry 1 to fit the remaining bytes of Entry 0's 128-byte body
        let remaining_len = 128 - entry0_body.len();
        entry0_body.extend_from_slice(&entry1_bytes[0..remaining_len]);

        let entry0 = HierarchyEntry {
            entry_type: 0,
            pubkey: target_pub0_arr,
            ts_begin: [0x00, 0x09, 0x16, 0xe7],
            ts_end: [0x80, 0x6b, 0x36, 0xec],
            body: entry0_body,
        };
        let entry0_bytes = entry0.serialize();

        // Let's compute SHA-512 hashes
        let h0_hash = Sha512::digest(&entry0_bytes);
        let h1_hash = Sha512::digest(&entry1_bytes);
        let h2_hash = Sha512::digest(&entry2_bytes);

        let h0 = import_hash(&h0_hash);
        let h1 = import_hash(&h1_hash);
        let h2 = import_hash(&h2_hash);

        println!("  h0 scalar: {:?}", hex::encode(h0.to_bytes()));
        println!("  h1 scalar: {:?}", hex::encode(h1.to_bytes()));
        println!("  h2 scalar: {:?}", hex::encode(h2.to_bytes()));

        for &(db_name, db_b64) in &d_base_candidates {
            let db_bytes = BASE64_STANDARD.decode(db_b64).unwrap();
            let d_base = Scalar::from_bytes_mod_order(db_bytes.try_into().unwrap());

            // Check derivation of d1 from d0:
            // Candidates for d1:
            let d1_candidates = vec![
                ("d0 + d_base * h0", d0 + (d_base * h0)),
                ("d0 - d_base * h0", d0 - (d_base * h0)),
                ("d_base * h0 - d0", (d_base * h0) - d0),
                ("-d0 - d_base * h0", -d0 - (d_base * h0)),
            ];

            for (d1_name, d1_val) in d1_candidates {
                let p1_derived = (&ED25519_BASEPOINT_TABLE).mul(&d1_val).compress().to_bytes();
                let mut flipped_p1 = p1_derived;
                flipped_p1[31] ^= 0x80;

                let mut d1_match = false;
                let mut derived_is_flipped = false;
                if p1_derived == target_pub0_arr {
                    d1_match = true;
                    derived_is_flipped = false;
                } else if flipped_p1 == target_pub0_arr {
                    d1_match = true;
                    derived_is_flipped = true;
                }

                if d1_match {
                    println!("    --> MATCH for d1! BaseKey: {}, Formulation: {}, Flipped: {}", db_name, d1_name, derived_is_flipped);
                    println!("        Derived d1 (b64): {}", BASE64_STANDARD.encode(d1_val.to_bytes()));

                    // Now derive d2 from d1!
                    let d2_candidates = vec![
                        ("d1 + d_base * h1", d1_val + (d_base * h1)),
                        ("d1 - d_base * h1", d1_val - (d_base * h1)),
                        ("d_base * h1 - d1", (d_base * h1) - d1_val),
                        ("-d1 - d_base * h1", -d1_val - (d_base * h1)),
                    ];

                    for (d2_name, d2_val) in d2_candidates {
                        let p2_derived = (&ED25519_BASEPOINT_TABLE).mul(&d2_val).compress().to_bytes();
                        let mut flipped_p2 = p2_derived;
                        flipped_p2[31] ^= 0x80;

                        let mut d2_match = false;
                        let mut d2_flipped = false;
                        if p2_derived == target_pub1_arr {
                            d2_match = true;
                            d2_flipped = false;
                        } else if flipped_p2 == target_pub1_arr {
                            d2_match = true;
                            d2_flipped = true;
                        }

                        if d2_match {
                            println!("        ====> MATCH for d2! Formulation: {}, Flipped: {}", d2_name, d2_flipped);
                            println!("              Derived d2 private key (b64): {}", BASE64_STANDARD.encode(d2_val.to_bytes()));

                            // Now derive d3 from d2!
                            let d3_candidates = vec![
                                ("d2 + d_base * h2", d2_val + (d_base * h2)),
                                ("d2 - d_base * h2", d2_val - (d_base * h2)),
                                ("d_base * h2 - d2", (d_base * h2) - d2_val),
                                ("-d2 - d_base * h2", -d2_val - (d_base * h2)),
                            ];

                            for (d3_name, d3_val) in d3_candidates {
                                let p3_derived = (&ED25519_BASEPOINT_TABLE).mul(&d3_val).compress().to_bytes();
                                let mut flipped_p3 = p3_derived;
                                flipped_p3[31] ^= 0x80;

                                let mut d3_match = false;
                                let mut d3_flipped = false;
                                if p3_derived == target_pub2_arr {
                                    d3_match = true;
                                    d3_flipped = false;
                                } else if flipped_p3 == target_pub2_arr {
                                    d3_match = true;
                                    d3_flipped = true;
                                }

                                if d3_match {
                                    println!("              ========> PERFECT MATCH for d3! Formulation: {}, Flipped: {}", d3_name, d3_flipped);
                                    println!("                        Derived d3 private key (b64): {}", BASE64_STANDARD.encode(d3_val.to_bytes()));
                                    println!("                        Derived d3 private key (hex): {}", hex::encode(d3_val.to_bytes()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
