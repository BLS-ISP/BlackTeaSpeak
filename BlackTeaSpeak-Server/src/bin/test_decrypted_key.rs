use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::ops::Mul;

fn main() {
    let key_hex = "d141d60887ea45e089275cabb83181696dd3ca6339a5ed2ca228dbd41cb61157";
    let key_bytes = hex::decode(key_hex).unwrap();
    
    let target_pub_hex = "7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8";
    let target_bytes = hex::decode(target_pub_hex).unwrap();

    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&key_bytes);

    let scalar = Scalar::from_bytes_mod_order(prv_array);
    let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
    let pk_bytes = pk_point.compress().to_bytes();
    
    let mut flipped_pk = pk_bytes;
    flipped_pk[31] ^= 0x80;

    println!("Derived Public Key:  {}", hex::encode(&pk_bytes));
    println!("Expected Public Key: {}", target_pub_hex);

    if pk_bytes == target_bytes.as_slice() {
        println!("VERIFICATION SUCCESSFUL: PERFECT MATCH!");
        println!("Private Key (base64): {}", BASE64_STANDARD.encode(&prv_array));
    } else if flipped_pk == target_bytes.as_slice() {
        println!("VERIFICATION SUCCESSFUL: MATCH WITH FLIPPED SIGN!");
        println!("Private Key (base64): {}", BASE64_STANDARD.encode(&prv_array));
    } else {
        println!("VERIFICATION FAILED.");
    }
}
