use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::ops::Mul;

fn main() {
    let ts3_master_prv_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo=";
    let ts3_master_prv_bytes = BASE64_STANDARD.decode(ts3_master_prv_b64).unwrap();
    let mut master_prv_array = [0u8; 32];
    master_prv_array.copy_from_slice(&ts3_master_prv_bytes);
    let master_prv = Scalar::from_bytes_mod_order(master_prv_array);
    
    let master_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&master_prv);
    let master_pk_bytes = master_pk_point.compress().to_bytes();
    
    println!("Master private key:          {}", hex::encode(&ts3_master_prv_bytes));
    println!("Master public key (Edwards): {}", hex::encode(&master_pk_bytes));
    println!("Master public key (base64):  {}", BASE64_STANDARD.encode(&master_pk_bytes));
}
