use base64;
fn main() {
    let b64 = "MEsDAgcAAgEgAiAqdBrPbdXf2ACpZePOSlLTYn0fpHQUMQDNBspncrlgaAIgVDeZ5bPjFdODkEshrgl9idXYwdUYAvrm3lES+e8jESA=";
    let bytes = base64::decode(b64).unwrap();
    println!("Omega length: {}", bytes.len());
    println!("Omega hex: {:X?}", bytes);
}
