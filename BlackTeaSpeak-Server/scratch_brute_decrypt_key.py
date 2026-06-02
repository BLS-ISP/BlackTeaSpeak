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

# Read ==key== from licensekey.dat
key_b64 = ""
with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
    is_key = False
    for line in f:
        if line.startswith("==key=="):
            is_key = not is_key
            continue
        if is_key:
            key_b64 += line.strip()

key_bytes = base64.b64decode(key_b64)
print(f"Total key_bytes length: {len(key_bytes)}")

# Let's search all possible starting offsets (usually 16 or around there)
for offset in range(0, 40):
    if offset + 16 > len(key_bytes):
        break
    
    # Extract candidate LicenseHeader
    header_bytes = key_bytes[offset:offset+16]
    version = struct.unpack("<H", header_bytes[0:2])[0]
    seed = struct.unpack("<Q", header_bytes[2:10])[0]
    v_off = header_bytes[10]
    verify = header_bytes[11:16]
    
    # Try twisted & untwisted
    for gen_name, gen_cls in [("twisted", Mt19937_64), ("untwisted", MtUntwisted_64)]:
        mt = gen_cls(seed)
        for _ in range(v_off):
            mt.next()
        received = mt.next()
        received_40 = (received ^ (received >> 40)) & 0xFFFFFFFFFF
        received_bytes = struct.pack("<Q", received_40)[0:5]
        
        if received_bytes == verify:
            print(f"VERIFICATION SUCCESS! offset={offset}, version={version}, seed={seed}, v_off={v_off}, gen={gen_name}")
            
            # Let's decrypt the body
            mt_dec = gen_cls(seed)
            encrypted_body = key_bytes[offset+16:]
            decoded = bytearray(len(encrypted_body))
            idx_dec = 0
            idx_enc = 0
            while idx_enc + 4 <= len(encrypted_body):
                val = struct.unpack("<I", encrypted_body[idx_enc : idx_enc + 4])[0]
                rand_val = mt_dec.next() & 0xffffffff
                dec_val = val ^ rand_val
                decoded[idx_dec : idx_dec + 4] = struct.pack("<I", dec_val)
                idx_enc += 4
                idx_dec += 4
            while idx_enc < len(encrypted_body):
                val = encrypted_body[idx_enc]
                rand_val = mt_dec.next() & 0xff
                decoded[idx_dec] = val ^ rand_val
                idx_enc += 1
                idx_dec += 1
                
            len_prv = struct.unpack("<H", decoded[0:2])[0]
            len_h = struct.unpack("<H", decoded[2:4])[0]
            print(f"  Decrypted length_private_data: {len_prv}")
            print(f"  Decrypted length_hierarchy: {len_h}")
            
            # Save decrypted body
            with open("decrypted_key_body.bin", "wb") as out:
                out.write(decoded)
            print("  Saved decrypted body to decrypted_key_body.bin")
            
print("Search finished.")
