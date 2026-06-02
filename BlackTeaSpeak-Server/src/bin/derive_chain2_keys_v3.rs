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
    // 1. Inputs
    // Root private key d0
    let d0_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo=";
    // Base private key d_base
    let d_base_b64 = "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM=";

    let d0_bytes = BASE64_STANDARD.decode(d0_b64).unwrap();
    let d_base_bytes = BASE64_STANDARD.decode(d_base_b64).unwrap();

    let d0 = Scalar::from_bytes_mod_order(d0_bytes.try_into().unwrap());
    let d_base = Scalar::from_bytes_mod_order(d_base_bytes.try_into().unwrap());

    // Public keys to match
    let pk0_expected = hex::decode("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429").unwrap();
    let pk1_expected = hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap();
    let pk2_expected = hex::decode("e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae").unwrap();

    // Raw Chain 2 Field 2 bytes
    let field2_hex = "0100af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80000000255465616d537065616b2053797374656d7320476d62480000d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d00189b68d53eaae569000000245465616d537065616b2073797374656d7320476d62480000e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae05189b68d53eaae569";
    let field2_bytes = hex::decode(field2_hex).unwrap();

    // 2. Prepare candidates for Entry 0
    let mut entry0_candidates = Vec::new();
    
    // Flat: index 1 to 70 of field2_data (69 bytes)
    let entry0_flat = field2_bytes[1..70].to_vec();
    entry0_candidates.push(("flat (69 bytes)", entry0_flat));

    // Nested: index 1 to 172 of field2_data (171 bytes)
    let entry0_nested = field2_bytes[1..172].to_vec();
    entry0_candidates.push(("nested (171 bytes)", entry0_nested));

    println!("=== Searching Level 1 (Root d0 -> Entry 0 pk0) ===");
    let mut d1_matched = None;
    let mut matched_lbl_0 = "";

    for (lbl, bytes) in &entry0_candidates {
        for &use_sha256 in &[false, true] {
            for &skip_prefix in &[false, true] {
                let test_bytes = if skip_prefix { &bytes[1..] } else { bytes };
                let hash = if use_sha256 {
                    let mut hasher = Sha256::new();
                    hasher.update(test_bytes);
                    hasher.finalize().to_vec()
                } else {
                    let mut hasher = Sha512::new();
                    hasher.update(test_bytes);
                    hasher.finalize().to_vec()
                };
                let h = import_hash(&hash);

                for &sign in &[-1f64, 1f64] {
                    let d1 = if sign > 0.0 { d0 + (d_base * h) } else { d0 - (d_base * h) };
                    let pk_derived = (&ED25519_BASEPOINT_TABLE).mul(&d1).compress().to_bytes();
                    let mut flipped = pk_derived;
                    flipped[31] ^= 0x80;

                    if pk_derived == pk0_expected.as_slice() || flipped == pk0_expected.as_slice() {
                        println!("MATCH FOUND for Level 1!");
                        println!("  Format:      {}", lbl);
                        println!("  SHA256:      {}", use_sha256);
                        println!("  Skip prefix: {}", skip_prefix);
                        println!("  Sign:        {}", sign);
                        println!("  Derived d1:  {}", BASE64_STANDARD.encode(d1.to_bytes()));
                        d1_matched = Some(d1);
                        matched_lbl_0 = lbl;
                        break;
                    }
                }
                if d1_matched.is_some() { break; }
            }
            if d1_matched.is_some() { break; }
        }
        if d1_matched.is_some() { break; }
    }

    if d1_matched.is_none() {
        println!("Level 1 derivation failed to match PK0.");
        return;
    }
    let d1 = d1_matched.unwrap();

    // 3. Prepare candidates for Entry 1
    let mut entry1_candidates = Vec::new();
    
    // Flat: index 70 to 139 (69 bytes)
    let entry1_flat = field2_bytes[70..139].to_vec();
    entry1_candidates.push(("flat (69 bytes)", entry1_flat));

    // Nested truncated: index 70 to 181 (111 bytes)
    let entry1_nested_trunc = field2_bytes[70..181].to_vec();
    entry1_candidates.push(("nested trunc (111 bytes)", entry1_nested_trunc.clone()));

    // Nested padded: index 70 to 181 padded with 00 bytes up to 148 bytes
    let mut entry1_nested_padded = entry1_nested_trunc.clone();
    while entry1_nested_padded.len() < 148 {
        entry1_nested_padded.push(0x00);
    }
    entry1_candidates.push(("nested padded 00 (148 bytes)", entry1_nested_padded.clone()));

    // Try other paddings (e.g. from 111 up to 148 with 00, just in case)
    for pad_len in 112..=148 {
        let mut entry1_nested_pad = entry1_nested_trunc.clone();
        while entry1_nested_pad.len() < pad_len {
            entry1_nested_pad.push(0x00);
        }
        entry1_candidates.push((Box::leak(format!("nested padded 00 ({} bytes)", pad_len).into_boxed_str()), entry1_nested_pad));
    }

    println!("\n=== Searching Level 2 (Entry 0 d1 -> Entry 1 pk1) ===");
    let mut d2_matched = None;
    let mut matched_lbl_1 = "";

    for (lbl, bytes) in &entry1_candidates {
        for &use_sha256 in &[false, true] {
            for &skip_prefix in &[false, true] {
                let test_bytes = if skip_prefix { &bytes[1..] } else { bytes };
                let hash = if use_sha256 {
                    let mut hasher = Sha256::new();
                    hasher.update(test_bytes);
                    hasher.finalize().to_vec()
                } else {
                    let mut hasher = Sha512::new();
                    hasher.update(test_bytes);
                    hasher.finalize().to_vec()
                };
                let h = import_hash(&hash);

                for &sign in &[-1f64, 1f64] {
                    let d2 = if sign > 0.0 { d1 + (d_base * h) } else { d1 - (h * d_base) };
                    let pk_derived = (&ED25519_BASEPOINT_TABLE).mul(&d2).compress().to_bytes();
                    let mut flipped = pk_derived;
                    flipped[31] ^= 0x80;

                    if pk_derived == pk1_expected.as_slice() || flipped == pk1_expected.as_slice() {
                        println!("MATCH FOUND for Level 2!");
                        println!("  Format:      {}", lbl);
                        println!("  SHA256:      {}", use_sha256);
                        println!("  Skip prefix: {}", skip_prefix);
                        println!("  Sign:        {}", sign);
                        println!("  Derived d2:  {}", BASE64_STANDARD.encode(d2.to_bytes()));
                        d2_matched = Some(d2);
                        matched_lbl_1 = lbl;
                        break;
                    }
                }
                if d2_matched.is_some() { break; }
            }
            if d2_matched.is_some() { break; }
        }
        if d2_matched.is_some() { break; }
    }

    if d2_matched.is_none() {
        println!("Level 2 derivation failed to match PK1.");
        return;
    }
    let d2 = d2_matched.unwrap();

    // 4. Prepare candidates for Entry 2 (License Sign)
    let mut entry2_candidates = Vec::new();
    
    // Flat: index 139 to 181 (42 bytes)
    let entry2_flat = field2_bytes[139..181].to_vec();
    entry2_candidates.push(("flat (42 bytes)", entry2_flat));

    println!("\n=== Searching Level 3 (Entry 1 d2 -> Entry 2 pk2 / License Sign Key) ===");
    let mut d3_matched = None;

    for (lbl, bytes) in &entry2_candidates {
        for &use_sha256 in &[false, true] {
            for &skip_prefix in &[false, true] {
                let test_bytes = if skip_prefix { &bytes[1..] } else { bytes };
                let hash = if use_sha256 {
                    let mut hasher = Sha256::new();
                    hasher.update(test_bytes);
                    hasher.finalize().to_vec()
                } else {
                    let mut hasher = Sha512::new();
                    hasher.update(test_bytes);
                    hasher.finalize().to_vec()
                };
                let h = import_hash(&hash);

                for &sign in &[-1f64, 1f64] {
                    let d3 = if sign > 0.0 { d2 + (d_base * h) } else { d2 - (d_base * h) };
                    let pk_derived = (&ED25519_BASEPOINT_TABLE).mul(&d3).compress().to_bytes();
                    let mut flipped = pk_derived;
                    flipped[31] ^= 0x80;

                    if pk_derived == pk2_expected.as_slice() || flipped == pk2_expected.as_slice() {
                        println!("MATCH FOUND for Level 3 (License Sign Key)!!!");
                        println!("  Format:      {}", lbl);
                        println!("  SHA256:      {}", use_sha256);
                        println!("  Skip prefix: {}", skip_prefix);
                        println!("  Sign:        {}", sign);
                        println!("  Derived d3 (License Sign prv): {}", BASE64_STANDARD.encode(d3.to_bytes()));
                        d3_matched = Some(d3);
                        break;
                    }
                }
                if d3_matched.is_some() { break; }
            }
            if d3_matched.is_some() { break; }
        }
        if d3_matched.is_some() { break; }
    }

    if d3_matched.is_none() {
        println!("Level 3 derivation failed to match PK2.");
    }
}
