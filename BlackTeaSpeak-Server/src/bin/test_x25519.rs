use x25519_dalek::{StaticSecret, PublicKey};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

fn main() {
    let prv_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g=";
    let pbl_b64 = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let pbl_bytes = BASE64_STANDARD.decode(pbl_b64).unwrap();
    
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);
    
    let secret = StaticSecret::from(prv_array);
    let public = PublicKey::from(&secret);
    let pk_bytes = public.as_bytes();
    
    println!("Public key in file:  {}", hex::encode(&pbl_bytes));
    println!("Derived via X25519:  {}", hex::encode(pk_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH! The root key is an X25519 key!");
    } else {
        println!("NO MATCH!");
    }
}
