use base64::{engine::general_purpose::STANDARD as base64_std, Engine as _};
use ed25519_dalek::SigningKey;

fn main() {
    let keys = vec![
        ("User Key prv (YAR...)", "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo="),
        ("Tea Master prv (oCP...)", "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g="),
        ("Base Key prv (6BN...)", "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="),
        ("Base Key prv (YBN...)", "YBNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="),
        ("root_key_prv (QCU...)", "QCUIVtjakIWe3BoNWHt9c6BX8lUyR4QOPirywBuPI0s="),
    ];
    
    let target_pbl = "d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d";
    let target_bytes = hex::decode(target_pbl).unwrap();
    println!("Target Public Key: {}\n", target_pbl);

    for (label, prv_b64) in keys {
        let prv_bytes = base64_std.decode(prv_b64).unwrap();
        let mut secret_bytes = [0u8; 32];
        secret_bytes.copy_from_slice(&prv_bytes);
        
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        let pk_bytes = verifying_key.to_bytes();
        
        let mut flipped_pk = pk_bytes;
        flipped_pk[31] ^= 0x80;
        
        println!("{}:", label);
        println!("  Derived PK (Unflipped): {}", hex::encode(&pk_bytes));
        println!("  Derived PK (Flipped):   {}", hex::encode(&flipped_pk));
        
        if pk_bytes == target_bytes.as_slice() {
            println!("  => PERFECT MATCH (Unflipped)!");
        } else if flipped_pk == target_bytes.as_slice() {
            println!("  => MATCH WITH FLIPPED SIGN BIT!");
        } else {
            println!("  => NO MATCH");
        }
    }
}
