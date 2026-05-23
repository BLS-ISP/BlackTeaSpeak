use aes::Aes128;
use eax::{Eax, aead::{Aead, KeyInit, Payload}};

fn main() {
    let dummy_key = b"c:\windows\syste";
    let dummy_nonce = b"m\firewall32.cpl";
    let cipher = Eax::<Aes128>::new(dummy_key.into());
    let own_packet_id = 23u16;
    let client_id = 0u16;
    let flags = 0x26u8;
    let mut header = vec![];
    header.extend_from_slice(&own_packet_id.to_be_bytes());
    header.extend_from_slice(&client_id.to_be_bytes());
    header.push(flags);
    let ack_packet_id = 0u16;
    let payload = Payload { msg: &ack_packet_id.to_be_bytes(), aad: &header };
    let encrypted = cipher.encrypt(dummy_nonce.into(), payload).unwrap();
    println!("Encrypted: {:?}", encrypted);
}
