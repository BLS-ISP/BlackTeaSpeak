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
with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
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

# 1. Decrypt Outer Layer
outer_seed = struct.unpack("<Q", b"3License")[0]
mt_outer = Mt19937_64(outer_seed)

encrypted_body = key_bytes[16:]
decoded_outer = bytearray(len(encrypted_body))
idx_dec = 0
idx_enc = 0
while idx_enc + 4 <= len(encrypted_body):
    val = struct.unpack("<I", encrypted_body[idx_enc : idx_enc + 4])[0]
    rand_val = mt_outer.next() & 0xffffffff
    dec_val = val ^ rand_val
    decoded_outer[idx_dec : idx_dec + 4] = struct.pack("<I", dec_val)
    idx_enc += 4
    idx_dec += 4
while idx_enc < len(encrypted_body):
    val = encrypted_body[idx_enc]
    rand_val = mt_outer.next() & 0xff
    decoded_outer[idx_dec] = val ^ rand_val
    idx_enc += 1
    idx_dec += 1

print("Outer decryption complete. Decoded length:", len(decoded_outer))

# 2. Extract Inner LicenseHeader (starts at offset 98 of decoded_outer)
print("\nHex dump of decoded_outer around index 98:")
for i in range(80, 130, 8):
    chunk = decoded_outer[i:i+8]
    print(f"  Index {i:3d}: {chunk.hex()}  |  {struct.unpack('<Q', chunk)[0] if len(chunk)==8 else 0}")

inner_header_bytes = decoded_outer[98 : 98+16]
inner_version = struct.unpack("<H", inner_header_bytes[0:2])[0]
inner_seed = struct.unpack("<Q", inner_header_bytes[2:10])[0]
inner_verify_offset = inner_header_bytes[10]
inner_verify = inner_header_bytes[11:16]

print(f"\nInner Header:")
print(f"  Version:       {inner_version}")
print(f"  Seed:          {inner_seed}")
print(f"  Verify offset: {inner_verify_offset}")
print(f"  Verify bytes:  {inner_verify.hex()}")

# 3. Verify Inner Seed against MT19937-64
mt_inner = Mt19937_64(inner_seed)
for _ in range(inner_verify_offset):
    mt_inner.next()
received = mt_inner.next()
received_40 = (received ^ (received >> 40)) & 0xFFFFFFFFFF
received_bytes = struct.pack("<Q", received_40)[0:5]

print(f"\nInner Verification Check:")
print(f"  Expected: {inner_verify.hex()}")
print(f"  Received: {received_bytes.hex()}")

if inner_verify == received_bytes:
    print("\nINNER VERIFICATION SUCCESSFUL!!!")
else:
    print("\nINNER VERIFICATION FAILED.")
