fn main() {
    use aes::Aes128;
    use eax::aead::{Aead, Payload};
    use eax::aead::generic_array::GenericArray;
    use eax::{Eax, NewAead};

    let key = b"c:\\windows\\syste";
    let nonce = b"m\\firewall32.cpl";
    let cipher = Eax::<Aes128>::new(GenericArray::from_slice(key));
    let nonce_arr = GenericArray::from_slice(nonce);

    let payload = b"Hello, EAX!";
    let encrypted = cipher.encrypt(nonce_arr, Payload { msg: payload, aad: b"" }).unwrap();
    println!("Encrypted length: {}", encrypted.len());

    // Try to decrypt in-place with wrong MAC
    let mut ciphertext = encrypted[..encrypted.len()-16].to_vec();
    let mut bad_mac = encrypted[encrypted.len()-16..].to_vec();
    bad_mac[0] ^= 1; // Corrupt MAC

    // Using AeadInPlace
    use eax::aead::AeadInPlace;
    let res = cipher.decrypt_in_place_detached(nonce_arr, b"", &mut ciphertext, GenericArray::from_slice(&bad_mac));
    println!("Result: {:?}", res);
    println!("Ciphertext buffer after failed decryption: {:?}", String::from_utf8_lossy(&ciphertext));
}
