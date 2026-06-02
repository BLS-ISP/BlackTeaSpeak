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

class MtUntwisted64:
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

with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
    lines = f.readlines()

key2_b64 = ""
is_key2 = False
for line in lines:
    trimmed = line.strip()
    if trimmed == "==key2==":
        is_key2 = not is_key2
        continue
    if is_key2:
        key2_b64 += trimmed

key2_bytes = base64.b64decode(key2_b64)
print("Total key2 bytes:", len(key2_bytes))

# Scan byte by byte
for i in range(len(key2_bytes) - 16):
    version = struct.unpack("<H", key2_bytes[i:i+2])[0]
    seed_le = struct.unpack("<Q", key2_bytes[i+2:i+10])[0]
    seed_be = struct.unpack(">Q", key2_bytes[i+2:i+10])[0]
    verify_offset = key2_bytes[i+10]
    verify = key2_bytes[i+11:i+16]
    
    if verify_offset == 0 or verify_offset > 200:
        continue
        
    for seed in [seed_le, seed_be]:
        for twisted in [True, False]:
            if twisted:
                mt = Mt19937_64(seed)
            else:
                mt = MtUntwisted64(seed)
                
            for _ in range(verify_offset):
                mt.next()
                
            rec = mt.next()
            rec_40 = (rec ^ (rec >> 40)) & 0xFFFFFFFFFF
            rec_bytes = struct.pack("<Q", rec_40)[:5]
            
            if rec_bytes == verify:
                print(f"\nMATCH FOUND at offset {i}!")
                print(f"  Version: {version}")
                print(f"  Seed: {seed}")
                print(f"  Twisted: {twisted}")
                print(f"  Verify Offset: {verify_offset}")
                print(f"  Verify: {verify.hex()}")
                
                # Decrypt the rest of the block starting at i + 16
                encrypted_body = key2_bytes[i+16:]
                mt_dec = Mt19937_64(seed) if twisted else MtUntwisted64(seed)
                # Skip the same offset
                for _ in range(verify_offset + 1):
                    mt_dec.next()
                    
                decrypted = bytearray()
                pos = 0
                while pos + 4 <= len(encrypted_body):
                    val = struct.unpack("<I", encrypted_body[pos:pos+4])[0]
                    rand_val = mt_dec.next() & 0xffffffff
                    dec_val = val ^ rand_val
                    decrypted.extend(struct.pack("<I", dec_val))
                    pos += 4
                while pos < len(encrypted_body):
                    val = encrypted_body[pos]
                    rand_val = mt_dec.next() & 0xff
                    dec_val = val ^ rand_val
                    decrypted.append(dec_val)
                    pos += 1
                    
                print(f"  Decrypted length: {len(decrypted)}")
                print(f"  First 50 decrypted bytes hex: {decrypted[:50].hex()}")
                
                # Parse layout
                len_prv = struct.unpack("<H", decrypted[0:2])[0]
                len_h = struct.unpack("<H", decrypted[2:4])[0]
                print(f"  Parsed Private Data Length: {len_prv}")
                print(f"  Parsed Hierarchy Data Length: {len_h}")
                
                if len_prv > 0 and len_h > 0 and len_prv + len_h + 88 <= len(decrypted):
                    print("  --> VALID OLD LAYOUT FOUND!")
                    prv_data = decrypted[88:88+len_prv]
                    print(f"  Private Data hex: {prv_data.hex()}")
                    
                    # Extract private keys!
                    # Parse Private Data:
                    # has_precalc (1 byte)
                    offset = 0
                    has_precalc = prv_data[offset] != 0
                    offset += 1
                    if has_precalc:
                        precalc_idx = prv_data[offset]
                        offset += 1
                        precalc_key = prv_data[offset:offset+32]
                        offset += 32
                        print(f"    Precalc key at index {precalc_idx}: b64={base64.b64encode(precalc_key).decode()}")
                        
                    private_key_count = prv_data[offset]
                    offset += 1
                    print(f"    Private key count: {private_key_count}")
                    for _ in range(private_key_count):
                        idx = prv_data[offset]
                        offset += 1
                        key = prv_data[offset:offset+32]
                        offset += 32
                        print(f"      Private key at index {idx}: b64={base64.b64encode(key).decode()}")
                
                if len_prv > 0 and len_h > 0 and len_prv + len_h + 132 <= len(decrypted):
                    print("  --> VALID NEW LAYOUT FOUND!")
                    prv_data = decrypted[132:132+len_prv]
                    print(f"  Private Data hex: {prv_data.hex()}")
