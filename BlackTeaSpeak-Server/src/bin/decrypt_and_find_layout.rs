use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use std::fs::File;
use std::io::{BufRead, BufReader};

struct Mt19937_64 {
    mt: [u64; 312],
    index: usize,
}

impl Mt19937_64 {
    fn new(seed: u64) -> Self {
        let mut mt = [0u64; 312];
        mt[0] = seed;
        for i in 1..312 {
            mt[i] = 6364136223846793005u64
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 62))
                .wrapping_add(i as u64);
        }
        Self { mt, index: 312 }
    }

    fn next(&mut self) -> u64 {
        if self.index >= 312 {
            for i in 0..312 {
                let y = (self.mt[i] & 0xFFFFFFFF80000000u64)
                    | (self.mt[(i + 1) % 312] & 0x7FFFFFFFu64);
                let mut x = y >> 1;
                if y & 1 != 0 {
                    x ^= 0xB5026F5AA96619E9u64;
                }
                self.mt[i] = self.mt[(i + 156) % 312] ^ x;
            }
            self.index = 0;
        }
        let mut x = self.mt[self.index];
        self.index += 1;
        x ^= (x >> 29) & 0x5555555555555555u64;
        x ^= (x << 17) & 0x71D67FFFEDA60000u64;
        x ^= (x << 37) & 0xFFF7EEE000000000u64;
        x ^= x >> 43;
        x
    }
}

struct MtUntwisted64 {
    mt: [u64; 312],
    index: usize,
}

impl MtUntwisted64 {
    fn new(seed: u64) -> Self {
        let mut mt = [0u64; 312];
        mt[0] = seed;
        for i in 1..312 {
            mt[i] = 6364136223846793005u64
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 62))
                .wrapping_add(i as u64);
        }
        Self { mt, index: 0 }
    }

    fn next(&mut self) -> u64 {
        if self.index >= 312 {
            for i in 0..312 {
                let y = (self.mt[i] & 0xFFFFFFFF80000000u64)
                    | (self.mt[(i + 1) % 312] & 0x7FFFFFFFu64);
                let mut x = y >> 1;
                if y & 1 != 0 {
                    x ^= 0xB5026F5AA96619E9u64;
                }
                self.mt[i] = self.mt[(i + 156) % 312] ^ x;
            }
            self.index = 0;
        }
        let mut x = self.mt[self.index];
        self.index += 1;
        x ^= (x >> 29) & 0x5555555555555555u64;
        x ^= (x << 17) & 0x71D67FFFEDA60000u64;
        x ^= (x << 37) & 0xFFF7EEE000000000u64;
        x ^= x >> 43;
        x
    }
}

fn main() {
    let file = File::open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat").unwrap();
    let reader = BufReader::new(file);
    let mut key_b64 = String::new();
    let mut is_key = false;

    for line in reader.lines() {
        let line = line.unwrap();
        let trimmed = line.trim();
        if trimmed == "==key2==" {
            is_key = !is_key;
            continue;
        }
        if is_key {
            key_b64.push_str(trimmed);
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

    let seeds = vec![("LE", seed_le), ("BE", seed_be)];
    let encrypted_body = &key_bytes[114..];

    for &(seed_name, seed) in &seeds {
        for &use_twisted in &[false, true] {
            for steps_to_skip in 0..=100 {
                let mut decoded = vec![0u8; encrypted_body.len()];
                if use_twisted {
                    let mut mt = Mt19937_64::new(seed);
                    for _ in 0..steps_to_skip { mt.next(); }
                    decoded = decrypt_body_u64(encrypted_body, &mut || mt.next());
                } else {
                    let mut mt = MtUntwisted64::new(seed);
                    for _ in 0..steps_to_skip { mt.next(); }
                    decoded = decrypt_body_u64(encrypted_body, &mut || mt.next());
                }

                if decoded.len() < 4 {
                    continue;
                }

                // Check possible layout:
                // 1. New layout (Header = 132 bytes):
                // length_private_data (2 bytes)
                // length_hierarchy (2 bytes)
                let len_prv = u16::from_le_bytes([decoded[0], decoded[1]]) as usize;
                let len_h = u16::from_le_bytes([decoded[2], decoded[3]]) as usize;

                let len_prv_be = u16::from_be_bytes([decoded[0], decoded[1]]) as usize;
                let len_h_be = u16::from_be_bytes([decoded[2], decoded[3]]) as usize;

                // Validate new layout
                if len_prv > 0 && len_h > 0 && 132 + len_prv + len_h <= decoded.len() {
                    println!("MATCH FOUND (New layout, LE lengths)!!!");
                    println!("  Seed: {}, Twisted: {}, Steps: {}", seed_name, use_twisted, steps_to_skip);
                    println!("  length_private_data: {}", len_prv);
                    println!("  length_hierarchy: {}", len_h);
                    println!("  First 32 bytes of decoded hex: {}", hex::encode(&decoded[..32]));
                    std::fs::write("decrypted_body.bin", &decoded).unwrap();
                }
                if len_prv_be > 0 && len_h_be > 0 && 132 + len_prv_be + len_h_be <= decoded.len() {
                    println!("MATCH FOUND (New layout, BE lengths)!!!");
                    println!("  Seed: {}, Twisted: {}, Steps: {}", seed_name, use_twisted, steps_to_skip);
                    println!("  length_private_data: {}", len_prv_be);
                    println!("  length_hierarchy: {}", len_h_be);
                    println!("  First 32 bytes of decoded hex: {}", hex::encode(&decoded[..32]));
                    std::fs::write("decrypted_body.bin", &decoded).unwrap();
                }

                // 2. Old layout (Header = 88 bytes):
                if len_prv > 0 && len_h > 0 && 88 + len_prv + len_h <= decoded.len() {
                    println!("MATCH FOUND (Old layout, LE lengths)!!!");
                    println!("  Seed: {}, Twisted: {}, Steps: {}", seed_name, use_twisted, steps_to_skip);
                    println!("  length_private_data: {}", len_prv);
                    println!("  length_hierarchy: {}", len_h);
                    println!("  First 32 bytes of decoded hex: {}", hex::encode(&decoded[..32]));
                    std::fs::write("decrypted_body.bin", &decoded).unwrap();
                }
                if len_prv_be > 0 && len_h_be > 0 && 88 + len_prv_be + len_h_be <= decoded.len() {
                    println!("MATCH FOUND (Old layout, BE lengths)!!!");
                    println!("  Seed: {}, Twisted: {}, Steps: {}", seed_name, use_twisted, steps_to_skip);
                    println!("  length_private_data: {}", len_prv_be);
                    println!("  length_hierarchy: {}", len_h_be);
                    println!("  First 32 bytes of decoded hex: {}", hex::encode(&decoded[..32]));
                    std::fs::write("decrypted_body.bin", &decoded).unwrap();
                }
            }
        }
    }
    println!("Layout scan finished.");
}

fn decrypt_body_u64(encrypted: &[u8], next_rand: &mut dyn FnMut() -> u64) -> Vec<u8> {
    let mut decoded = vec![0u8; encrypted.len()];
    let mut index = 0;
    while index + 4 <= encrypted.len() {
        let val = u32::from_le_bytes([
            encrypted[index], encrypted[index+1], encrypted[index+2], encrypted[index+3]
        ]);
        let rand_val = next_rand() as u32;
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
