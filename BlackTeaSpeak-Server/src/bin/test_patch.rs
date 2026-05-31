use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
fn main() {
    let chain_b64 = "AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAA==";
    let crypto_chain = BASE64_STANDARD.decode(&chain_b64).unwrap();
    let mut exported_chain = crypto_chain.clone();
    
    let new_begin: u32 = 0x1757E2D3;
    let new_end: u32 = 0x2A123456;
    
    exported_chain[35..39].copy_from_slice(&new_begin.to_be_bytes());
    exported_chain[39..43].copy_from_slice(&new_end.to_be_bytes());
    
    let new_b64 = BASE64_STANDARD.encode(&exported_chain);
    println!("Original: {}", chain_b64);
    println!("Patched:  {}", new_b64);
}
