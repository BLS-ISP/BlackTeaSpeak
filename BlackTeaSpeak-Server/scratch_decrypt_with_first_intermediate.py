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

# Skip protobuf headers of key2
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

print(f"License block start offset in key2: {pos}")
# LicenseHeader from key2 starts at 'pos'
license_header_bytes = key2_bytes[pos:pos+16]
version = struct.unpack("<H", license_header_bytes[0:2])[0]
seed = struct.unpack("<Q", license_header_bytes[2:10])[0]
verify_offset = license_header_bytes[10]
verify = license_header_bytes[11:16]

print(f"Parsed seed: {seed}")
print(f"Parsed verify_offset: {verify_offset}")
print(f"Parsed verify bytes: {verify.hex()}")

# Verify with MT19937-64 (twisted)
mt = Mt19937_64(seed)
for _ in range(verify_offset):
    mt.next()
received = mt.next()
received_40 = (received ^ (received >> 40)) & 0xFFFFFFFFFF
received_bytes = struct.pack("<Q", received_40)[0:5]
print(f"Generated verify bytes (twisted): {received_bytes.hex()}")

if verify == received_bytes:
    print("VERIFICATION SUCCESSFUL!!!")
    # Decrypt ==key== block using this verified seed!
    # Wait, the encrypted block starts at index 114 of key_bytes (after signature)
    encrypted_body = key_bytes[114:]
    # Reseed to start of decryption
    mt = Mt19937_64(seed)
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
        
    print("\nDecrypted ==key== body header:")
    length_private_data = struct.unpack("<H", decoded[0:2])[0]
    length_hierarchy = struct.unpack("<H", decoded[2:4])[0]
    print(f"  length_private_data: {length_private_data}")
    print(f"  length_hierarchy: {length_hierarchy}")
    
    # Let's save the decrypted block
    with open("decrypted_key.bin", "wb") as out:
        out.write(decoded)
    print("Saved decrypted body to decrypted_key.bin")
else:
    print("VERIFICATION FAILED.")
