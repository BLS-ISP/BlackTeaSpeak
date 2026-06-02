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

# Read ==key== and ==key2== from licensekey.dat
key2_b64 = ""
key_b64 = ""
with open("../licensekey.dat", "r") as f:
    is_key2 = False
    is_key = False
    for line in f:
        if line.startswith("==key2=="):
            is_key2 = not is_key2
            continue
        if line.startswith("==key=="):
            is_key = not is_key
            continue
        if is_key2:
            key2_b64 += line.strip()
        if is_key:
            key_b64 += line.strip()

key2_bytes = base64.b64decode(key2_b64)
key_bytes = base64.b64decode(key_b64)

# Seeds
seed_3lic_le = struct.unpack("<Q", key_bytes[2:10])[0]
seed_3lic_be = struct.unpack(">Q", key_bytes[2:10])[0]

# Extract seed from key2 (first intermediate)
pos = 0
while pos < len(key2_bytes) and key2_bytes[pos] == 0x0a:
    pos += 1
    length = 0
    shift = 0
    while True:
        b = key2_bytes[pos]
        pos += 1
        length |= (b & 0x7f) << shift
        shift += 7
        if b & 0x80 == 0:
            break
seed_k2_le = struct.unpack("<Q", key2_bytes[pos+2 : pos+10])[0]
seed_k2_be = struct.unpack(">Q", key2_bytes[pos+2 : pos+10])[0]
verify_offset = key2_bytes[pos+10]

seeds = [
    ("3License_LE", seed_3lic_le, 103),
    ("3License_BE", seed_3lic_be, 103),
    ("Intermediate_LE", seed_k2_le, verify_offset),
    ("Intermediate_BE", seed_k2_be, verify_offset),
]

for seed_name, seed, v_off in seeds:
    for gen_name, gen_cls in [("twisted", Mt19937_64), ("untwisted", MtUntwisted_64)]:
        for continued in [False, True]:
            for offset in range(16, 150):
                if offset >= len(key_bytes):
                    continue
                encrypted = key_bytes[offset:]
                
                mt = gen_cls(seed)
                if continued:
                    for _ in range(v_off + 1):
                        mt.next()
                        
                decoded = bytearray(len(encrypted))
                idx_dec = 0
                idx_enc = 0
                while idx_enc + 4 <= len(encrypted):
                    val = struct.unpack("<I", encrypted[idx_enc : idx_enc + 4])[0]
                    rand_val = mt.next() & 0xffffffff
                    dec_val = val ^ rand_val
                    decoded[idx_dec : idx_dec + 4] = struct.pack("<I", dec_val)
                    idx_enc += 4
                    idx_dec += 4
                while idx_enc < len(encrypted):
                    val = encrypted[idx_enc]
                    rand_val = mt.next() & 0xff
                    decoded[idx_dec] = val ^ rand_val
                    idx_enc += 1
                    idx_dec += 1
                    
                if len(decoded) < 4:
                    continue
                    
                # Check LE lengths
                len_prv_le = struct.unpack("<H", decoded[0:2])[0]
                len_h_le = struct.unpack("<H", decoded[2:4])[0]
                
                # Check BE lengths
                len_prv_be = struct.unpack(">H", decoded[0:2])[0]
                len_h_be = struct.unpack(">H", decoded[2:4])[0]
                
                if 0 < len_prv_le < 500 and 0 < len_h_le < 500:
                    print(f"MATCH (LE lengths)!!! Seed: {seed_name}, Gen: {gen_name}, Continued: {continued}, Offset: {offset}")
                    print(f"  length_private_data: {len_prv_le}")
                    print(f"  length_hierarchy: {len_h_le}")
                    print(f"  First 32 bytes hex: {decoded[:32].hex()}")
                    
                if 0 < len_prv_be < 500 and 0 < len_h_be < 500:
                    print(f"MATCH (BE lengths)!!! Seed: {seed_name}, Gen: {gen_name}, Continued: {continued}, Offset: {offset}")
                    print(f"  length_private_data: {len_prv_be}")
                    print(f"  length_hierarchy: {len_h_be}")
                    print(f"  First 32 bytes hex: {decoded[:32].hex()}")
                    
print("Robust search finished.")
