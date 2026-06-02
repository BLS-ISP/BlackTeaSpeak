import base64
import struct
import hashlib

class Mt19937_64:
    def __init__(self, seed):
        self.mt = [0] * 312
        self.mt[0] = seed
        for i in range(1, 312):
            self.mt[i] = (6364136223846793005 * (self.mt[i-1] ^ (self.mt[i-1] >> 62)) + i) & 0xffffffffffffffff
        self.index = 312

    def next(self):
        if self.index >= 312:
            for i in range(312):
                y = (self.mt[i] & 0xFFFFFFFF80000000) | (self.mt[(i + 1) % 312] & 0x7FFFFFFF)
                x = y >> 1
                if y & 1 != 0:
                    x ^= 0xB5026F5AA96619E9
                self.mt[i] = self.mt[(i + 156) % 312] ^ x
            self.index = 0
        x = self.mt[self.index]
        self.index += 1
        x ^= (x >> 29) & 0x5555555555555555
        x ^= (x << 17) & 0x71D67FFFEDA60000
        x ^= (x << 37) & 0xFFF7EEE000000000
        x ^= x >> 43
        return x

class MtUntwisted:
    def __init__(self, seed):
        self.mt = [0] * 312
        self.mt[0] = seed
        for i in range(1, 312):
            self.mt[i] = (6364136223846793005 * (self.mt[i-1] ^ (self.mt[i-1] >> 62)) + i) & 0xffffffffffffffff
        self.index = 0

    def next(self):
        if self.index >= 312:
            for i in range(312):
                y = (self.mt[i] & 0xFFFFFFFF80000000) | (self.mt[(i + 1) % 312] & 0x7FFFFFFF)
                x = y >> 1
                if y & 1 != 0:
                    x ^= 0xB5026F5AA96619E9
                self.mt[i] = self.mt[(i + 156) % 312] ^ x
            self.index = 0
        x = self.mt[self.index]
        self.index += 1
        x ^= (x >> 29) & 0x5555555555555555
        x ^= (x << 17) & 0x71D67FFFEDA60000
        x ^= (x << 37) & 0xFFF7EEE000000000
        x ^= x >> 43
        return x

# Let's check candidate seeds:
word = b"3License"
hashes = [
    ("raw_le", struct.unpack("<Q", word)[0]),
    ("raw_be", struct.unpack(">Q", word)[0]),
    ("sha256_le", struct.unpack("<Q", hashlib.sha256(word).digest()[:8])[0]),
    ("sha256_be", struct.unpack(">Q", hashlib.sha256(word).digest()[:8])[0]),
    ("md5_le", struct.unpack("<Q", hashlib.md5(word).digest()[:8])[0]),
    ("md5_be", struct.unpack(">Q", hashlib.md5(word).digest()[:8])[0]),
    ("sha1_le", struct.unpack("<Q", hashlib.sha1(word).digest()[:8])[0]),
    ("sha1_be", struct.unpack(">Q", hashlib.sha1(word).digest()[:8])[0]),
]

expected_u32 = 0x812ef462
expected_5th = 0x62

print(f"Target first u32: {hex(expected_u32)}")
print(f"Target 5th byte: {hex(expected_5th)}")

# Test standard hashes and small static seeds
for name, seed in hashes:
    for gen_type, gen_cls in [("twisted", Mt19937_64), ("untwisted", MtUntwisted)]:
        mt = gen_cls(seed)
        first_val = mt.next() & 0xffffffff
        if first_val == expected_u32:
            second_val = mt.next() & 0xff
            if second_val == expected_5th:
                print(f"MATCH FOUND!!! Seed: {seed} ({name}), Gen: {gen_type}")
                exit(0)

# Brute force search for seed in a larger range
print("Brute forcing seeds 0 to 10,000,000...")
for seed in range(10000000):
    # Twisted
    mt = Mt19937_64(seed)
    if (mt.next() & 0xffffffff) == expected_u32:
        if (mt.next() & 0xff) == expected_5th:
            print(f"MATCH FOUND!!! Seed: {seed}, Gen: twisted")
            exit(0)
    # Untwisted
    mt_un = MtUntwisted(seed)
    if (mt_un.next() & 0xffffffff) == expected_u32:
        if (mt_un.next() & 0xff) == expected_5th:
            print(f"MATCH FOUND!!! Seed: {seed}, Gen: untwisted")
            exit(0)

print("Finished search.")
