#[test]
fn test_scalar_derivation() {
    use curve25519_dalek::scalar::Scalar;
    use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use std::ops::Mul;

    let prv_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo=";
    let prv_bytes = BASE64.decode(prv_b64).unwrap();
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);

    let s = Scalar::from_bytes_mod_order(prv_array);
    let r_point = (&ED25519_BASEPOINT_TABLE).mul(&s);
    let mut r_bytes = r_point.compress().to_bytes();
    
    println!("--- TEST SCALAR DERIVATION ---");
    println!("Public key unflipped hex: {}", hex::encode(&r_bytes));
    
    // Flipped (TeamSpeak standard)
    r_bytes[31] ^= 0x80;
    println!("Public key flipped hex:   {}", hex::encode(&r_bytes));
    println!("Public key flipped b64:   {}", BASE64.encode(&r_bytes));
    println!("-----------------------------");
}

