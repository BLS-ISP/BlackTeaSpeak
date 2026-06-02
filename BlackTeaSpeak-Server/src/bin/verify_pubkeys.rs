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
    let d0_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g="; // Master Root prv
    let d0_bytes = BASE64_STANDARD.decode(d0_b64).unwrap();
    let d0 = Scalar::from_bytes_mod_order(d0_bytes.try_into().unwrap());
    let p_master = (&ED25519_BASEPOINT_TABLE).mul(&d0);

    // New Chain 1 Entries
    let entry0_bytes = hex::decode("00af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80000000255465616d537065616b2053797374656d7320476d624800").unwrap();
    let entry1_bytes = hex::decode("00d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d00189b68d53eaae569000000245465616d537065616b2073797374656d7320476d624800").unwrap();
    let entry2_bytes = hex::decode("007c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a80218a459801c49bf800600000000466c6f7269616e204d617468696173204265726b656d6569657200").unwrap();

    // Expected public keys at each level (Intermediate 1, Intermediate 2/Server stored, etc.)
    // Wait! Let's check which public key is at which level!
    // Stored keys:
    // Entry 0 stored key: af6c7153...
    // Entry 1 stored key: d1bec3ed...
    // Entry 2 stored key: 7c28498b...
    
    // In TS3 verification, the derived point is:
    // Level 1: P_int_0 = P_master + (base_pk_0 * h0)
    // Does P_int_0 match the stored key of Entry 1 (d1bec3ed...)?
    // Or is the parent public key for Entry 1 exactly P_int_0? Yes, and it verifies Entry 1!
    // But does Entry 1 contain base_pk_1, and derives P_int_1 = P_int_0 + (base_pk_1 * h1)?
    // And does Entry 2 contain base_pk_2, and derives P_int_2 = P_int_1 + (base_pk_2 * h2)?
    // Yes!
    
    let base_pk_0 = curve25519_dalek::edwards::CompressedEdwardsY(
        hex::decode("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429").unwrap().try_into().unwrap()
    ).decompress().unwrap();
    
    let expected_p_int_0 = hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap();

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

            // Try both signs
            for sign0 in &[-1f64, 1f64] {
                let p_int_0 = if *sign0 > 0.0 {
                    p_master + (base_pk_0 * h0)
                } else {
                    p_master - (base_pk_0 * h0)
                };
                let p_bytes = p_int_0.compress().to_bytes();
                let mut flipped_p = p_bytes;
                flipped_p[31] ^= 0x80;

                if p_bytes == expected_p_int_0.as_slice() {
                    println!("MATCH! Level 1 (Entry 0 -> Entry 1): sha256={}, skip0={}, sign0={}, unflipped", use_sha256, skip0, sign0);
                } else if flipped_p == expected_p_int_0.as_slice() {
                    println!("MATCH! Level 1 (Entry 0 -> Entry 1): sha256={}, skip0={}, sign0={}, flipped", use_sha256, skip0, sign0);
                }
            }
        }
    }
    println!("Step 1 verification done.");
}
