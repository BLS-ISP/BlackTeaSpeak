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

# Read ONLY ==key== from licensekey.dat
key_b64 = ""
with open("../licensekey.dat", "r") as f:
    lines = f.readlines()

is_key = False
for line in lines:
    line = line.strip()
    if line == "==key==":
        is_key = not is_key
        continue
    if is_key:
        key_b64 += line

key_bytes = base64.b64decode(key_b64)
print(f"Loaded correct ==key== length: {len(key_bytes)} bytes")

# Check if the prefix is TS3License
if key_bytes.startswith(b"TS3License"):
    print("Correct TS3License prefix found!")
    # The signature header is: "TS3License" (10 bytes) + type (1 byte, 'g'=0x67) + ASN.1 DER header (5 bytes: 30 65 02 30 2a)
    # The actual encrypted license block starts at offset 16!
    license_block = key_bytes[16:]
    print(f"Encrypted license block length: {len(license_block)}")
    
    # Parse LicenseHeader
    version = struct.unpack("<H", license_block[0:2])[0]
    seed_le = struct.unpack("<Q", license_block[2:10])[0]
    seed_be = struct.unpack(">Q", license_block[2:10])[0]
    verify_offset = license_block[10]
    verify = license_block[11:16]
    
    print("License Header:")
    print(f"  version: {version}")
    print(f"  seed (LE): {seed_le}")
    print(f"  verify_offset: {verify_offset}")
    print(f"  verify bytes: {list(verify)}")
    
    # Verify using MT19937-64
    for seed_name, seed in [("LE", seed_le), ("BE", seed_be)]:
        mt = Mt19937_64(seed)
        for _ in range(verify_offset):
            mt.next()
        received = mt.next()
        received = (received ^ (received >> 40)) & 0xFFFFFFFFFF
        
        expected = struct.unpack("<Q", verify + b"\x00\x00\x00")[0] & 0xFFFFFFFFFF
        print(f"Seed {seed_name} Verification Check:")
        print(f"  Expected: {expected}")
        print(f"  Received: {received}")
        
        if expected == received:
            print(f"  => SUCCESS! Seed {seed_name} is correct!")
            
            # Decrypt body starting at offset 16 of license_block
            mt_dec = Mt19937_64(seed)
            body = license_block[16:]
            decoded = bytearray(len(body))
            idx = 0
            while idx + 4 <= len(body):
                val = struct.unpack("<I", body[idx:idx+4])[0]
                rand_val = mt_dec.next() & 0xffffffff
                dec_val = val ^ rand_val
                decoded[idx:idx+4] = struct.pack("<I", dec_val)
                idx += 4
            while idx < len(body):
                val = body[idx]
                rand_val = mt_dec.next() & 0xff
                decoded[idx] = val ^ rand_val
                idx += 1
                
            print(f"Decrypted body size: {len(decoded)}")
            print("First 100 decrypted bytes:", decoded[:100].hex())
            
            # Save decrypted body
            with open("decrypted_license_correct.bin", "wb") as outf:
                outf.write(decoded)
            print("Saved decrypted body to decrypted_license_correct.bin")
            
else:
    print("Error: Does not start with TS3License!")
