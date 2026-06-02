use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::Mul;

struct Mt19937_32 {
    mt: [u32; 624],
    index: usize,
}

impl Mt19937_32 {
    fn new(seed: u32) -> Self {
        let mut mt = [0u32; 624];
        mt[0] = seed;
        for i in 1..624 {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { mt, index: 624 }
    }

    fn next(&mut self) -> u32 {
        if self.index >= 624 {
            for i in 0..624 {
                let y = (self.mt[i] & 0x80000000u32)
                    | (self.mt[(i + 1) % 624] & 0x7fffffffu32);
                let mut x = y >> 1;
                if y & 1 != 0 {
                    x ^= 0x9908b0dfu32;
                }
                self.mt[i] = self.mt[(i + 397) % 624] ^ x;
            }
            self.index = 0;
        }
        let mut x = self.mt[self.index];
        self.index += 1;
        x ^= x >> 11;
        x ^= (x << 7) & 0x9d2c5680u32;
        x ^= (x << 15) & 0xefc60000u32;
        x ^= x >> 18;
        x
    }
}

struct MtUntwisted32 {
    mt: [u32; 624],
    index: usize,
}

impl MtUntwisted32 {
    fn new(seed: u32) -> Self {
        let mut mt = [0u32; 624];
        mt[0] = seed;
        for i in 1..624 {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { mt, index: 0 }
    }

    fn next(&mut self) -> u32 {
        if self.index >= 624 {
            for i in 0..624 {
                let y = (self.mt[i] & 0x80000000u32)
                    | (self.mt[(i + 1) % 624] & 0x7fffffffu32);
                let mut x = y >> 1;
                if y & 1 != 0 {
                    x ^= 0x9908b0dfu32;
                }
                self.mt[i] = self.mt[(i + 397) % 624] ^ x;
            }
            self.index = 0;
        }
        let mut x = self.mt[self.index];
        self.index += 1;
        x ^= x >> 11;
        x ^= (x << 7) & 0x9d2c5680u32;
        x ^= (x << 15) & 0xefc60000u32;
        x ^= x >> 18;
        x
    }
}

fn main() {
    // 1. Read licensekey.dat ==key== block
    let file = File::open("../licensekey.dat").unwrap();
    let reader = BufReader::new(file);
    let mut key_b64 = String::new();
    let mut is_key = false;

    for line in reader.lines() {
        let line = line.unwrap();
        if line.starts_with("==key==") {
            is_key = !is_key;
            continue;
        }
        if is_key {
            key_b64.push_str(line.trim());
        }
    }

    let key_bytes = BASE64_STANDARD.decode(&key_b64).unwrap();
    println!("Total key bytes: {}", key_bytes.len());

    let seed_le = u64::from_le_bytes([
        key_bytes[2], key_bytes[3], key_bytes[4], key_bytes[5],
        key_bytes[6], key_bytes[7], key_bytes[8], key_bytes[9]
    ]);
    let seed_be = u64::from_be_bytes([
        key_bytes[2], key_bytes[3], key_bytes[4], key_bytes[5],
        key_bytes[6], key_bytes[7], key_bytes[8], key_bytes[9]
    ]);

    let target_pub_hex = "7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8";
    let target_bytes = hex::decode(target_pub_hex).unwrap();

    let seeds = vec![
        ("seed_le_low", (seed_le & 0xffffffff) as u32),
        ("seed_le_high", (seed_le >> 32) as u32),
        ("seed_be_low", (seed_be & 0xffffffff) as u32),
        ("seed_be_high", (seed_be >> 32) as u32),
    ];

    // Scan all offsets from 10 to 150
    for offset in 10..=150 {
        if offset >= key_bytes.len() {
            break;
        }
        let encrypted_body = &key_bytes[offset..];

        for &(seed_name, seed) in &seeds {
            // Try twisted MT32
            {
                let mut mt = Mt19937_32::new(seed);
                let decoded = decrypt_body_u32(encrypted_body, &mut || mt.next());
                scan_buffer(&decoded, &target_bytes, "twisted_mt32", seed_name, offset);
            }
            // Try untwisted MT32
            {
                let mut mt = MtUntwisted32::new(seed);
                let decoded = decrypt_body_u32(encrypted_body, &mut || mt.next());
                scan_buffer(&decoded, &target_bytes, "untwisted_mt32", seed_name, offset);
            }
        }
    }

    println!("Scan finished.");
}

fn decrypt_body_u32(encrypted: &[u8], next_rand: &mut dyn FnMut() -> u32) -> Vec<u8> {
    let mut decoded = vec![0u8; encrypted.len()];
    let mut index = 0;
    while index + 4 <= encrypted.len() {
        let val = u32::from_le_bytes([
            encrypted[index], encrypted[index+1], encrypted[index+2], encrypted[index+3]
        ]);
        let rand_val = next_rand();
        let dec_val = val ^ rand_val;
        decoded[index..index+4].copy_from_slice(&dec_val.to_le_bytes());
        index += 4;
    }
    while index < encrypted.len() {
        let val = encrypted[index];
        let rand_val = next_rand() as u8;
        let dec_val = val ^ rand_val;
        decoded[index] = dec_val;
        index += 1;
    }
    decoded
}

fn scan_buffer(buffer: &[u8], target_pub: &[u8], gen_name: &str, seed_name: &str, offset: usize) {
    if buffer.len() < 32 {
        return;
    }
    for i in 0..=(buffer.len() - 32) {
        let mut prv_bytes = [0u8; 32];
        prv_bytes.copy_from_slice(&buffer[i..i+32]);

        let scalar = Scalar::from_bytes_mod_order(prv_bytes);
        let pk_point = (&ED25519_BASEPOINT_TABLE).mul(&scalar);
        let pk_bytes = pk_point.compress().to_bytes();

        let mut flipped_pk = pk_bytes;
        flipped_pk[31] ^= 0x80;

        if pk_bytes == target_pub || flipped_pk == target_pub {
            println!("MATCH FOUND!!!");
            println!("  Generator: {}, Seed: {}, Offset: {}, Sub-offset: {}", gen_name, seed_name, offset, i);
            println!("  Private Key (hex): {}", hex::encode(&prv_bytes));
            println!("  Private Key (base64): {}", BASE64_STANDARD.encode(&prv_bytes));
        }
    }
}
