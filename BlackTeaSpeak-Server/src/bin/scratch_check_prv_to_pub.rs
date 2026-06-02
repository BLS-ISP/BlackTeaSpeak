use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Digest};
use std::ops::Mul;

fn main() {
    let prv_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo=";
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();

    let target_pub = hex::decode("188025f57cf0bf4a0ab770e4827cf8e20993b290cdf21d430a635078a094e4be").unwrap();
    println!("Target pub: {:02X?}", target_pub);

    // Try 1: Direct LE
    let d_le = Scalar::from_bytes_mod_order(prv_bytes.clone().try_into().unwrap());
    let pub_le = (&ED25519_BASEPOINT_TABLE).mul(&d_le).compress().to_bytes();
    let mut flipped_le = pub_le;
    flipped_le[31] ^= 0x80;
    println!("LE: {:02X?} (flipped: {:02X?})", pub_le, flipped_le);

    // Try 2: Direct BE
    let mut be_bytes = prv_bytes.clone();
    be_bytes.reverse();
    let d_be = Scalar::from_bytes_mod_order(be_bytes.try_into().unwrap());
    let pub_be = (&ED25519_BASEPOINT_TABLE).mul(&d_be).compress().to_bytes();
    let mut flipped_be = pub_be;
    flipped_be[31] ^= 0x80;
    println!("BE: {:02X?} (flipped: {:02X?})", pub_be, flipped_be);

    // Try 3: Clamped LE
    let mut hasher = Sha512::new();
    hasher.update(&prv_bytes);
    let hash = hasher.finalize();
    let mut cl_bytes = [0u8; 32];
    cl_bytes.copy_from_slice(&hash[0..32]);
    cl_bytes[0] &= 248;
    cl_bytes[31] &= 127;
    cl_bytes[31] |= 64;
    let d_cl = Scalar::from_bytes_mod_order(cl_bytes);
    let pub_cl = (&ED25519_BASEPOINT_TABLE).mul(&d_cl).compress().to_bytes();
    let mut flipped_cl = pub_cl;
    flipped_cl[31] ^= 0x80;
    println!("Clamped LE: {:02X?} (flipped: {:02X?})", pub_cl, flipped_cl);
}
