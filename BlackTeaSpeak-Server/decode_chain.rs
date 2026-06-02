use std::fs;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

fn main() {
    let content = fs::read_to_string("protocol_key.txt").unwrap();
    for line in content.lines() {
        if line.starts_with("chain:") {
            let b64 = line.trim_start_matches("chain: ");
            let bytes = BASE64_STANDARD.decode(b64).unwrap();
            println!("Chain length: {}", bytes.len());
            println!("Chain hex: {:02X?}", bytes);
        }
    }
}
