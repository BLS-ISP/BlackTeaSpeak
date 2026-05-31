use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};

fn main() {
    let prv_b64 = "Hv63zAmpqFR/aNm+JNhf3EV11rxzqr/rRMYz3e68DQc=";
    let pbl_b64 = "6pf1vRZwwtXctqlLQgLd++V4uvpgVSi24zPf3H85zso=";
    
    let prv_bytes = BASE64_STANDARD.decode(prv_b64).unwrap();
    let pbl_bytes = BASE64_STANDARD.decode(pbl_b64).unwrap();
    
    let mut prv_array = [0u8; 32];
    prv_array.copy_from_slice(&prv_bytes);
    
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&prv_array);
    let pk_bytes = signing_key.verifying_key().to_bytes();
    
    println!("Public key in file:  {}", hex::encode(&pbl_bytes));
    println!("Derived via dalek:   {}", hex::encode(&pk_bytes));
    
    if pk_bytes == pbl_bytes.as_slice() {
        println!("MATCH! This TeaSpeak key pair is a standard Ed25519 key pair!");
    } else {
        println!("NO MATCH!");
    }
}
