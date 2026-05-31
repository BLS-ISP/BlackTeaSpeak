use std::time::{SystemTime, UNIX_EPOCH};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

fn main() {
    let chain_b64 = "AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAA==";
    let crypto_chain = BASE64_STANDARD.decode(&chain_b64).unwrap();
    println!("crypto_chain len: {}", crypto_chain.len());
    
    let mut exported_chain = crypto_chain.clone();
    exported_chain.push(0x00);
    
    // dummy ephem pk (32 bytes)
    let ephem_pk = [0u8; 32];
    exported_chain.extend_from_slice(&ephem_pk);
    exported_chain.push(0x20);
    
    let begin = 0u32;
    let end = 0u32;
    exported_chain.extend_from_slice(&begin.to_be_bytes());
    exported_chain.extend_from_slice(&end.to_be_bytes());
    
    println!("exported_chain len: {}", exported_chain.len());
    let new_b64 = BASE64_STANDARD.encode(&exported_chain);
    println!("new_b64: {}", new_b64);
}
