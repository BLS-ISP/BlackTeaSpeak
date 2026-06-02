fn main() {
    // Let's implement standard MT19937-64 and check the values
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

    let seed = 13060217094303476911u64;
    let mut mt = Mt19937_64::new(seed);
    for _ in 0..234 {
        mt.next();
    }
    let val = mt.next();
    println!("val: {}", val);
    let val_40 = (val ^ (val >> 40)) & 0xFFFFFFFFFFu64;
    println!("val_40: {}", val_40);
    println!("val_40 bytes: {:?}", &val_40.to_le_bytes()[0..5]);
}
