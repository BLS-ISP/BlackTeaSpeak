use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

fn main() {
    let root_pbl = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=";
    let decoded = BASE64_STANDARD.decode(root_pbl).unwrap();
    println!("Hex: {:02X?}", decoded);
}
