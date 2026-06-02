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
    // Authentic Root public key corresponding to d0
    let root_pbl_bytes = hex::decode("99025e9cfa517c375f94edb32f10b8104825ed1bf389495ee341c97be5c981b9").unwrap();
    let root_pbl_compressed = curve25519_dalek::edwards::CompressedEdwardsY(root_pbl_bytes.try_into().unwrap());
    let p_root = root_pbl_compressed.decompress().unwrap();

    // Entry 0 public key (base_pk_0)
    let base_pk_0_bytes = hex::decode("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429").unwrap();
    let base_pk_0_compressed = curve25519_dalek::edwards::CompressedEdwardsY(base_pk_0_bytes.try_into().unwrap());
    let base_pk_0 = base_pk_0_compressed.decompress().unwrap();

    // Expected derived Entry 1 public key
    let expected_pk1_bytes = hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap();

    // Field 2 bytes
    let field2_hex = "0100af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80000000255465616d537065616b2053797374656d7320476d62480000d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d00189b68d53eaae569000000245465616d537065616b2073797374656d7320476d62480000e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae05189b68d53eaae569";
    let field2_bytes = hex::decode(field2_hex).unwrap();

    // Try both flat and nested Entry 0 bytes
    let entry0_flat = field2_bytes[1..70].to_vec();
    let entry0_nested = field2_bytes[1..172].to_vec();

    let candidates = vec![
        ("flat (69 bytes)", entry0_flat),
        ("nested (171 bytes)", entry0_nested),
    ];

    println!("=== Verifying Level 1 Public Key Derivation ===");
    for (lbl, bytes) in &candidates {
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
                    let p_derived = if sign > 0.0 {
                        p_root + (base_pk_0 * h)
                    } else {
                        p_root - (base_pk_0 * h)
                    };
                    let p_bytes = p_derived.compress().to_bytes();
                    let mut flipped = p_bytes;
                    flipped[31] ^= 0x80;

                    if p_bytes == expected_pk1_bytes.as_slice() {
                        println!("MATCH FOUND! lbl={}, sha256={}, skip_prefix={}, sign={} (unflipped)", lbl, use_sha256, skip_prefix, sign);
                    } else if flipped == expected_pk1_bytes.as_slice() {
                        println!("MATCH FOUND! lbl={}, sha256={}, skip_prefix={}, sign={} (flipped)", lbl, use_sha256, skip_prefix, sign);
                    }
                }
            }
        }
    }
}
