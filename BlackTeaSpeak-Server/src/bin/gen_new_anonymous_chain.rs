use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sha2::{Sha512, Digest};
use std::ops::Mul;

fn import_hash(hash: &[u8]) -> Scalar {
    let mut buffer = [0u8; 64];
    buffer[0..32].copy_from_slice(&hash[0..32]);
    buffer[0] &= 0xF8;
    buffer[31] &= 0x3F;
    buffer[31] |= 0x40;
    Scalar::from_bytes_mod_order_wide(&buffer)
}

fn main() {
    let mut rng = rand::thread_rng();

    // 1. TeamSpeak Master Root Private Key
    let ts3_master_prv_b64 = "oCPmMAvfkS6z/UWghpcfl+a7EO11FMGh/DGKSVgJ33g=";
    let ts3_master_prv_bytes = BASE64_STANDARD.decode(ts3_master_prv_b64).unwrap();
    let mut master_prv_array = [0u8; 32];
    master_prv_array.copy_from_slice(&ts3_master_prv_bytes);
    let master_prv = Scalar::from_bytes_mod_order(master_prv_array);

    // 2. Generate Random Anonymous Base Private Key (s_anon_base)
    let mut rand_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rng, &mut rand_bytes);
    let anon_base_prv = Scalar::from_bytes_mod_order(rand_bytes);
    
    // 3. Derive Anonymous Base Public Key (A_anon_base)
    let anon_base_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&anon_base_prv);
    let anon_base_pk_bytes = anon_base_pk_point.compress().to_bytes();

    // 4. Construct Anonymous ServerLicenseEntry
    let mut entry_bytes = Vec::new();
    entry_bytes.push(0x00); // prefix/separator
    entry_bytes.extend_from_slice(&anon_base_pk_bytes); // 32 bytes public key
    entry_bytes.push(0x02); // License Type (0x02)
    
    // Timestamps: Begin = 2026-01-01, End = 2046-01-01
    const TIMESTAMP_OFFSET: u64 = 1356998400;
    let begin_time: u64 = 1767225600; // 2026-01-01 00:00:00 UTC
    let end_time: u64 = 2398291200;   // 2046-01-01 00:00:00 UTC
    
    let begin = (begin_time - TIMESTAMP_OFFSET) as u32;
    let end = (end_time - TIMESTAMP_OFFSET) as u32;
    
    entry_bytes.extend_from_slice(&begin.to_be_bytes());
    entry_bytes.extend_from_slice(&end.to_be_bytes());
    
    entry_bytes.push(0x07); // licenseType = 0x07 (Licensed features)
    
    let slots: u32 = 32; // slots = 32
    entry_bytes.extend_from_slice(&slots.to_be_bytes());
    
    entry_bytes.extend_from_slice(b"Anonymous\0"); // issuer string (10 bytes)

    // 5. Hash the entry (skipping the first 0x00 byte for hashing, matching TeamSpeak's actual derivation spec)
    let mut hasher = Sha512::new();
    hasher.update(&entry_bytes[1..]);
    let entry_hash = hasher.finalize();

    // 6. importHash to get hash_scalar
    let hash_scalar = import_hash(&entry_hash);

    // 7. Derive Anonymous Private Key
    //    s_anon = (anon_base_prv * hash_scalar) + master_prv mod l
    let anon_derived_prv = (anon_base_prv * hash_scalar) + master_prv;
    
    // 8. Derive Anonymous Public Key
    let anon_pk_point = (&ED25519_BASEPOINT_TABLE).mul(&anon_derived_prv);
    let anon_pk_bytes = anon_pk_point.compress().to_bytes();

    // 9. Construct Exported Chain
    let mut exported_chain = Vec::new();
    exported_chain.push(0x01); // ExportChain prefix (0x01)
    exported_chain.extend_from_slice(&entry_bytes);

    let chain_b64 = BASE64_STANDARD.encode(&exported_chain);
    let root_key_prv = BASE64_STANDARD.encode(anon_derived_prv.as_bytes());
    let root_key_pbl = BASE64_STANDARD.encode(&anon_pk_bytes);

    println!("Generated new Anonymous chain successfully!");
    println!("chain: {}", chain_b64);
    println!("root_key_prv: {}", root_key_prv);
    println!("root_key_pbl: {}", root_key_pbl);
}
