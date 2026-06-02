use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

fn main() {
    let prv_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo=";
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);

    let msg = b"Hello, World!";

    // Test with Entry 2 pubkey (flipped): e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae
    let mut pub_entry2 = hex::decode("e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae").unwrap();
    
    // Test with Entry 1 pubkey (flipped): d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d
    let mut pub_entry1 = hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap();

    // Test with Master Root pubkey (Edwards): 99025e9cfa517c375f94edb32f10b8104825ed1bf389495ee341c97be5c981b9
    let mut pub_master = hex::decode("99025e9cfa517c375f94edb32f10b8104825ed1bf389495ee341c97be5c981b9").unwrap();

    for (name, mut pub_key) in vec![("Entry 2", pub_entry2), ("Entry 1", pub_entry1), ("Master", pub_master)] {
        let signature = blackteaspeak_server::desktop_crypto::sign_with_raw_scalar(
            &prv_array,
            &pub_key.clone().try_into().unwrap(),
            msg
        );

        // Verification in TS3 uses standard Ed25519 verification on the unflipped public key and unflipped signature R
        let mut pub_unflipped = pub_key.clone();
        pub_unflipped[31] ^= 0x80;

        use curve25519_dalek::edwards::CompressedEdwardsY;
        let pub_point = CompressedEdwardsY(pub_unflipped.try_into().unwrap()).decompress();
        if let Some(p) = pub_point {
            println!("  {}: Decodes successfully to Edwards Point!", name);
            // Verify signature
            let mut r_bytes = [0u8; 32];
            r_bytes.copy_from_slice(&signature[0..32]);
            let r_point = CompressedEdwardsY(r_bytes).decompress();
            if r_point.is_some() {
                println!("  {}: Signature R point decodes successfully!", name);
            } else {
                println!("  {}: Signature R point decoding FAILED", name);
            }
        } else {
            println!("  {}: Edwards Point decompression FAILED", name);
        }
    }
}
