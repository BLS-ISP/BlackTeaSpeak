class Mt19937_32:
    def __init__(self, seed):
        self.mt = [0] * 624
        self.mt[0] = seed & 0xffffffff
        for i in range(1, 624):
            self.mt[i] = (1812433253 * (self.mt[i-1] ^ (self.mt[i-1] >> 30)) + i) & 0xffffffff
        self.index = 624

    def next(self):
        if self.index >= 624:
            for i in range(624):
                y = (self.mt[i] & 0x80000000) | (self.mt[(i + 1) % 624] & 0x7fffffff)
                x = y >> 1
                if y & 1 != 0:
                    x ^= 0x9908b0df
                self.mt[i] = self.mt[(i + 397) % 624] ^ x
            self.index = 0
        x = self.mt[self.index]
        self.index += 1
        x ^= x >> 11
        x ^= (x << 7) & 0x9d2c5680
        x ^= (x << 15) & 0xefc60000
        x ^= x >> 18
        return x

expected_u32 = 0x812ef462
expected_5th = 0x62

# Let's brute force all 32-bit seeds!
# Since there are 4,294,967,296 seeds, let's run in chunks or check the first 50,000,000 seeds first.
print("Checking first 50,000,000 32-bit seeds...")
for seed in range(50000000):
    mt = Mt19937_32(seed)
    if mt.next() == expected_u32:
        # Check 5th byte (first byte of second u32)
        if (mt.next() & 0xff) == expected_5th:
            print(f"MATCH FOUND!!! Seed: {seed}")
            exit(0)

print("Finished 32-bit search.")
