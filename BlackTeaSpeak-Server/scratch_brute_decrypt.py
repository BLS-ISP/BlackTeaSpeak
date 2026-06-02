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

# We will collect candidate seeds:
seeds = []
# From pos 0 to 50, extract potential seeds:
for pos in range(0, min(50, len(key_bytes) - 16)):
    license_block = key_bytes[pos:]
    seed_le = struct.unpack("<Q", license_block[2:10])[0]
    seed_be = struct.unpack(">Q", license_block[2:10])[0]
    seeds.append((pos, seed_le, "LE"))
    seeds.append((pos, seed_be, "BE"))

# Also add the old seed
seeds.append((0, 13060217094303476911, "OLD_SEED"))

# Try to decrypt with each candidate
for pos, seed, seed_name in seeds:
    for gen_type, gen_cls in [("twisted", Mt19937_64), ("untwisted", MtUntwisted)]:
        # The encrypted body starts after the 16-byte LicenseHeader (offset by 'pos')
        header_len = 16
        if len(key_bytes) <= pos + header_len:
            continue
            
        encrypted_body = key_bytes[pos + header_len:]
        mt = gen_cls(seed)
        
        # Standard decryption loop
        decoded = bytearray(len(encrypted_body))
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
            
        length_private_data = struct.unpack("<H", decoded[0:2])[0]
        length_hierarchy = struct.unpack("<H", decoded[2:4])[0]
        
        # Check if the lengths are valid
        if length_private_data > 0 and length_hierarchy > 0:
            total_expected_len = 88 + length_private_data + length_hierarchy
            if total_expected_len == len(decoded):
                print(f"SUCCESS MATCH!!!")
                print(f"  Pos: {pos}")
                print(f"  Seed: {seed} ({seed_name})")
                print(f"  Generator: {gen_type}")
                print(f"  length_private_data: {length_private_data}")
                print(f"  length_hierarchy: {length_hierarchy}")
                print(f"  Decrypted bytes hex: {decoded.hex()}")
                # Save the decrypted bytes
                with open("decrypted_license.bin", "wb") as out:
                    out.write(decoded)
                print("Saved decrypted body to decrypted_license.bin")
                exit(0)
                
print("Finished brute force check.")
