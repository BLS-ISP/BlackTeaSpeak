use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::ops::Mul;

fn main() {
    let d0_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g=";
    let d0_bytes = BASE64_STANDARD.decode(d0_b64).unwrap();
    let d0 = Scalar::from_bytes_mod_order(d0_bytes.try_into().unwrap());
    let pub_bytes = (&ED25519_BASEPOINT_TABLE).mul(&d0).compress().to_bytes();
    let mut flipped_pub = pub_bytes;
    flipped_pub[31] ^= 0x80;
    println!("Master Root Pub derived:      {}", hex::encode(&pub_bytes));
    println!("Master Root Pub (flipped):    {}", hex::encode(&flipped_pub));
    println!("Master Root Pub base64:       {}", BASE64_STANDARD.encode(&pub_bytes));
}
