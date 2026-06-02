use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::Mul;

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

    // Candidate target public keys to scan for:
    let targets = vec![
        ("Root", hex::decode("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429").unwrap()),
        ("Intermediate", hex::decode("d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d").unwrap()),
        ("License Sign", hex::decode("e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae").unwrap()),
        ("Server", hex::decode("7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8").unwrap()),
    ];

    // Check header at offset 16 (for license_block starting at 16)
    let block16 = &key_bytes[16..];
    let seed16_le = u64::from_le_bytes(block16[2..10].try_into().unwrap());
    let seed16_be = u64::from_be_bytes(block16[2..10].try_into().unwrap());
    let verify_offset16 = block16[10];

    // Check header at offset 114
    let block114 = &key_bytes[114..];
    let seed114_le = u64::from_le_bytes(block114[2..10].try_into().unwrap());
    let seed114_be = u64::from_be_bytes(block114[2..10].try_into().unwrap());
    let verify_offset114 = block114[10];

    let seed_configs = vec![
        ("16_LE", seed16_le, 16, verify_offset16 as usize),
        ("16_BE", seed16_be, 16, verify_offset16 as usize),
        ("114_LE", seed114_le, 114, verify_offset114 as usize),
        ("114_BE", seed114_be, 114, verify_offset114 as usize),
    ];

    for &(seed_name, seed, header_offset, verify_offset) in &seed_configs {
        let encrypted_body = &key_bytes[(header_offset + 16)..];
        
        for &use_twisted in &[false, true] {
            // Also scan standard steps, and steps around the verify_offset
            let step_candidates = vec![
                0, 
                1, 
                verify_offset, 
                verify_offset + 1,
            ];
            
            for &steps in &step_candidates {
                let mut decoded = vec![0u8; encrypted_body.len()];
                if use_twisted {
                    let mut mt = Mt19937_64::new(seed);
                    for _ in 0..steps { mt.next(); }
                    decoded = decrypt_body_u64(encrypted_body, &mut || mt.next());
                } else {
                    let mut mt = MtUntwisted64::new(seed);
                    for _ in 0..steps { mt.next(); }
                    decoded = decrypt_body_u64(encrypted_body, &mut || mt.next());
                }
                
                // Scan buffer for all targets
                for &(target_name, ref target_bytes) in &targets {
                    scan_buffer(&decoded, target_bytes, target_name, seed_name, use_twisted, steps);
                }
            }
        }
    }

    println!("Comprehensive scan finished.");
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

fn scan_buffer(buffer: &[u8], target_pub: &[u8], target_name: &str, seed_name: &str, twisted: bool, steps: usize) {
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
            println!("  Target: {}", target_name);
            println!("  Seed: {}, Twisted: {}, Steps: {}, Offset: {}", seed_name, twisted, steps, i);
            println!("  Private Key (hex): {}", hex::encode(&prv_bytes));
            println!("  Private Key (base64): {}", BASE64_STANDARD.encode(&prv_bytes));
        }
    }
}
