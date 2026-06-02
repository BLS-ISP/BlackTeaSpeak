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

fn main() {
    // 1. Read licensekey.dat
    let file = File::open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat").unwrap();
    let reader = BufReader::new(file);
    let mut key2_b64 = String::new();
    let mut is_key = false;

    for line in reader.lines() {
        let line = line.unwrap();
        let trimmed = line.trim();
        if trimmed == "==key==" {
            is_key = !is_key;
            continue;
        }
        if trimmed == "==key2==" {
            continue;
        }
        if is_key {
            key2_b64.push_str(trimmed);
        }
    }

    let key2_bytes = BASE64_STANDARD.decode(&key2_b64).unwrap();
    println!("Total key bytes: {}", key2_bytes.len());

    // Skip protobuf headers (starts with 0x0a)
    let mut pos = 0;
    while pos < key2_bytes.len() && key2_bytes[pos] == 0x0a {
        pos += 1;
        // Parse varint length
        let mut len: usize = 0;
        let mut shift = 0;
        loop {
            let b = key2_bytes[pos];
            pos += 1;
            len |= ((b & 0x7f) as usize) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        println!("Protobuf field length: {}", len);
    }

    let license_block = &key2_bytes[pos..];
    println!("License block starting at pos {}: length={}", pos, license_block.len());

    // Parse LicenseHeader
    let version = u16::from_le_bytes([license_block[0], license_block[1]]);
    let seed_le = u64::from_le_bytes([
        license_block[2], license_block[3], license_block[4], license_block[5],
        license_block[6], license_block[7], license_block[8], license_block[9]
    ]);
    let seed_be = u64::from_be_bytes([
        license_block[2], license_block[3], license_block[4], license_block[5],
        license_block[6], license_block[7], license_block[8], license_block[9]
    ]);
    let verify_offset = license_block[10];
    let verify = &license_block[11..16];

    println!("License Header:");
    println!("  version: {}", version);
    println!("  seed (LE): {}", seed_le);
    println!("  seed (BE): {}", seed_be);
    println!("  verify_offset: {}", verify_offset);
    println!("  verify: {:?}", verify);

    let mut seed = seed_le;
    // Let's check which seed works!
    for &test_seed in &[seed_le, seed_be] {
        let mut mt_t = Mt19937_64::new(test_seed);
        for _ in 0..verify_offset {
            mt_t.next();
        }
        let rec = mt_t.next();
        let rec_40 = (rec ^ (rec >> 40)) & 0xFFFFFFFFFFu64;
        let rec_bytes = &rec_40.to_le_bytes()[0..5];
        if rec_bytes == verify {
            println!("  MATCH FOUND! seed = {}, twisted", test_seed);
            seed = test_seed;
            break;
        }

        // Try untwisted as well
        let mut mt_t_un = MtUntwisted::new(test_seed);
        for _ in 0..verify_offset {
            mt_t_un.next();
        }
        let rec_un = mt_t_un.next();
        let rec_un_40 = (rec_un ^ (rec_un >> 40)) & 0xFFFFFFFFFFu64;
        let rec_un_bytes = &rec_un_40.to_le_bytes()[0..5];
        if rec_un_bytes == verify {
            println!("  MATCH FOUND! seed = {}, untwisted", test_seed);
            seed = test_seed;
            break;
        }
    }

    // Verification check (twisted)
    let mut mt_test = Mt19937_64::new(seed);
    for _ in 0..verify_offset {
        mt_test.next();
    }
    let received = mt_test.next();
    let received_40 = (received ^ (received >> 40)) & 0xFFFFFFFFFFu64;
    let received_bytes = &received_40.to_le_bytes()[0..5];
    println!("  Expected verify bytes: {:?}", verify);
    println!("  Received verify bytes (twisted): {:?}", received_bytes);

    // Verification check (untwisted - starting at index 0 without initial twist)
    struct MtUntwisted {
        mt: [u64; 312],
        index: usize,
    }
    impl MtUntwisted {
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

    let mut mt_test2 = MtUntwisted::new(seed);
    for _ in 0..verify_offset {
        mt_test2.next();
    }
    let received2 = mt_test2.next();
    let received2_40 = (received2 ^ (received2 >> 40)) & 0xFFFFFFFFFFu64;
    let received2_bytes = &received2_40.to_le_bytes()[0..5];
    println!("  Received verify bytes (untwisted): {:?}", received2_bytes);
    if received2_bytes == verify {
        println!("  MT19937 UNTWISTED VERIFICATION SUCCESSFUL!");
    }

    // Decrypt loop
    let mut mt = Mt19937_64::new(seed);
    let mut decoded = vec![0u8; license_block.len() - 16];
    
    let mut index = 16;
    let mut index_decoded = 0;
    while index + 4 <= license_block.len() {
        let val = u32::from_le_bytes([
            license_block[index], license_block[index+1], license_block[index+2], license_block[index+3]
        ]);
        let rand_val = mt.next() as u32;
        let dec_val = val ^ rand_val;
        decoded[index_decoded..index_decoded+4].copy_from_slice(&dec_val.to_le_bytes());
        index += 4;
        index_decoded += 4;
    }
    while index < license_block.len() {
        let val = license_block[index];
        let rand_val = mt.next() as u8;
        let dec_val = val ^ rand_val;
        decoded[index_decoded] = dec_val;
        index += 1;
        index_decoded += 1;
    }

    println!("Decrypted buffer size: {}", decoded.len());

    // Parse BodyHeader safely
    if decoded.len() < 88 {
        println!("Decrypted buffer too small for BodyHeader!");
        return;
    }
    let length_private_data = u16::from_le_bytes([decoded[0], decoded[1]]) as usize;
    let length_hierarchy = u16::from_le_bytes([decoded[2], decoded[3]]) as usize;
    let checksum_hierarchy = &decoded[4..24];
    let private_data_sign = &decoded[24..88];

    println!("Body Header:");
    println!("  length_private_data: {}", length_private_data);
    println!("  length_hierarchy: {}", length_hierarchy);
    println!("  checksum_hierarchy: {:x?}", checksum_hierarchy);

    if 88 + length_private_data > decoded.len() || 88 + length_private_data + length_hierarchy > decoded.len() {
        println!("Error: BodyHeader lengths exceed decoded buffer size!");
        println!("First 50 decrypted bytes hex: {:x?}", &decoded[..std::cmp::min(50, decoded.len())]);
        return;
    }

    let private_data = &decoded[88..88+length_private_data];
    let hierarchy_data = &decoded[88+length_private_data..88+length_private_data+length_hierarchy];

    println!("Private Data length: {}", private_data.len());
    println!("Hierarchy Data length: {}", hierarchy_data.len());

    // Parse Private Data
    let mut offset = 0;
    let has_precalc = private_data[offset] != 0;
    offset += 1;

    let mut precalc_index = -2;
    let mut precalc_key = vec![0u8; 32];
    if has_precalc {
        precalc_index = private_data[offset] as i32;
        offset += 1;
        precalc_key.copy_from_slice(&private_data[offset..offset+32]);
        offset += 32;
        println!("Precalc Key found at index {}: b64={}", precalc_index, BASE64_STANDARD.encode(&precalc_key));
    } else {
        println!("No precalculated private key found!");
    }

    let private_key_count = private_data[offset] as usize;
    offset += 1;
    println!("Private key count: {}", private_key_count);

    let mut private_keys = std::collections::BTreeMap::new();
    for _ in 0..private_key_count {
        let idx = private_data[offset];
        offset += 1;
        let mut key = [0u8; 32];
        key.copy_from_slice(&private_data[offset..offset+32]);
        offset += 32;
        private_keys.insert(idx, key);
        println!("  Raw Private Key at index {}: b64={}", idx, BASE64_STANDARD.encode(&key));
    }

    // Parse Hierarchy Entries
    let mut entries = Vec::new();
    let mut h_offset = 0;
    while h_offset < hierarchy_data.len() {
        let entry_start = h_offset;
        let entry_type = hierarchy_data[h_offset];
        h_offset += 1;
        
        let mut pub_key = [0u8; 32];
        pub_key.copy_from_slice(&hierarchy_data[h_offset..h_offset+32]);
        h_offset += 32;
        
        let timestamp_begin = u32::from_le_bytes([
            hierarchy_data[h_offset], hierarchy_data[h_offset+1], hierarchy_data[h_offset+2], hierarchy_data[h_offset+3]
        ]);
        h_offset += 4;
        
        let timestamp_end = u32::from_le_bytes([
            hierarchy_data[h_offset], hierarchy_data[h_offset+1], hierarchy_data[h_offset+2], hierarchy_data[h_offset+3]
        ]);
        h_offset += 4;
        
        let body_len = u16::from_le_bytes([
            hierarchy_data[h_offset], hierarchy_data[h_offset+1]
        ]) as usize;
        h_offset += 2;
        
        let body = &hierarchy_data[h_offset..h_offset+body_len];
        h_offset += body_len;
        
        let total_entry_len = h_offset - entry_start;
        let entry_bytes = &hierarchy_data[entry_start..h_offset];
        
        println!("Hierarchy Entry Type={}: pubkey={}... begin={}, end={}, body_len={}", 
            entry_type, hex::encode(&pub_key)[..10].to_string(), timestamp_begin, timestamp_end, body_len
        );
        entries.push((entry_type, pub_key, entry_bytes.to_vec()));
    }

    // Now derive the final private key at index 3!
    use curve25519_dalek::scalar::Scalar;
    use sha2::{Sha512, Digest};

    fn import_hash(hash: &[u8]) -> Scalar {
        let mut buffer = [0u8; 64];
        buffer[0..32].copy_from_slice(&hash[0..32]);
        buffer[0] &= 0xF8;
        buffer[31] &= 0x3F;
        buffer[31] |= 0x40;
        Scalar::from_bytes_mod_order_wide(&buffer)
    }

    let mut current_prv_bytes = precalc_key;
    let mut base_index = precalc_index;
    let target_index = 3;

    let mut current_prv = Scalar::from_bytes_mod_order(
        current_prv_bytes.try_into().unwrap()
    );

    while base_index < target_index {
        base_index += 1;
        let child_prv_bytes = private_keys.get(&(base_index as u8)).unwrap();
        let child_prv = Scalar::from_bytes_mod_order(*child_prv_bytes);

        // Calculate hash of hierarchy entry
        let entry_bytes = &entries[base_index as usize].2;
        
        // Match the hashing behavior in HierarchyEntry::hash:
        // Hash the entire entry_bytes!
        let mut hasher = Sha512::new();
        hasher.update(entry_bytes);
        let entry_hash = hasher.finalize();

        let hash_scalar = import_hash(&entry_hash);

        // buffer = (child_prv * hash_scalar) + buffer
        current_prv = (child_prv * hash_scalar) + current_prv;
    }

    let derived_prv_bytes = current_prv.to_bytes();
    println!("\n=== DERIVATION SUCCESSFUL ===");
    println!("Derived Private Key (base64): {}", BASE64_STANDARD.encode(&derived_prv_bytes));
    
    // Also let's base64-encode the full public chain!
    // A full public chain starts with 0x01 followed by all hierarchy entries.
    let mut full_chain = Vec::new();
    full_chain.push(0x01);
    for entry in &entries {
        full_chain.extend_from_slice(&entry.2);
    }
    println!("Full Chain (base64): {}", BASE64_STANDARD.encode(&full_chain));
}
