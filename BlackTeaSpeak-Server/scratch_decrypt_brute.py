import base64
import struct

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

class MtUntwisted_64:
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

# Read ==key== from licensekey.dat
key_b64 = ""
with open("../licensekey.dat", "r") as f:
    is_key = False
    for line in f:
        if line.startswith("==key=="):
            is_key = True
            continue
        if line.startswith("=="):
            is_key = False
            continue
        if is_key:
            key_b64 += line.strip()

key_bytes = base64.b64decode(key_b64)
print(f"Loaded key_bytes: {len(key_bytes)} bytes")

seed_le = struct.unpack("<Q", key_bytes[2:10])[0]
seed_be = struct.unpack(">Q", key_bytes[2:10])[0]

# Try combinations
seeds = [
    ("LE_64", seed_le),
    ("BE_64", seed_be),
    ("old_64", 13060217094303476911),
]

for seed_name, seed in seeds:
    for gen_name, gen_cls in [("twisted_64", Mt19937_64), ("untwisted_64", MtUntwisted_64), ("twisted_32", Mt19937_32)]:
        # Let's try offsets
        for offset in range(11, 200):
            if offset >= len(key_bytes):
                continue
            encrypted_body = key_bytes[offset:]
            mt = gen_cls(seed)
            decoded = bytearray(len(encrypted_body))
            
            # Decrypt
            idx_dec = 0
            idx_enc = 0
            while idx_enc + 4 <= len(encrypted_body):
                val = struct.unpack("<I", encrypted_body[idx_enc : idx_enc + 4])[0]
                rand_val = mt.next() & 0xffffffff
                dec_val = val ^ rand_val
                decoded[idx_dec : idx_dec + 4] = struct.pack("<I", dec_val)
                idx_enc += 4
                idx_dec += 4
            while idx_enc < len(encrypted_body):
                val = encrypted_body[idx_enc]
                rand_val = mt.next() & 0xff
                decoded[idx_dec] = val ^ rand_val
                idx_enc += 1
                idx_dec += 1
                
            if len(decoded) < 88:
                continue
                
            # Parse lengths
            length_private_data = struct.unpack("<H", decoded[0:2])[0]
            length_hierarchy = struct.unpack("<H", decoded[2:4])[0]
            
            if length_private_data > 0 and length_hierarchy > 0:
                total_len = 88 + length_private_data + length_hierarchy
                if total_len == len(decoded):
                    print(f"MATCH FOUND!!! Offset: {offset}, Seed: {seed_name}, Gen: {gen_name}")
                    print(f"  length_private_data: {length_private_data}")
                    print(f"  length_hierarchy: {length_hierarchy}")
                    # Save decrypted buffer
                    with open("decrypted_license.bin", "wb") as out:
                        out.write(decoded)
                    exit(0)

print("Brute force finished.")
