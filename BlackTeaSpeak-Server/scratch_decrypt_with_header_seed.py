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

# We will decrypt starting at pos 0 using seed from index 2..10 and verify_offset at index 10
pos = 0
license_block = key_bytes[pos:]
seed = struct.unpack("<Q", license_block[2:10])[0]
verify_offset = license_block[10]
verify = license_block[11:16]

print(f"Using seed: {seed}")
print(f"Using verify_offset: {verify_offset}")
print(f"Expected verify: {verify.hex()}")

# Run MT19937-64
mt = Mt19937_64(seed)
# Check verify
for _ in range(verify_offset):
    mt.next()
rec = mt.next()
rec_40 = (rec ^ (rec >> 40)) & 0xFFFFFFFFFF
rec_bytes = struct.pack("<Q", rec_40)[0:5]
print(f"Generated verify: {rec_bytes.hex()}")

# Decrypt
mt = Mt19937_64(seed) # Re-seed to start from index 16
encrypted_body = key_bytes[16:]
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

print("\nFirst 100 decrypted bytes:")
print("  Hex:", decoded[:100].hex())
print("  ASCII:", bytes(decoded[:100]).decode('utf-8', errors='ignore'))
