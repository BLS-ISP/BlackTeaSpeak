import base64
import struct

# The base64 chain from protocol_key.txt
chain_b64 = "AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAA=="
chain_bytes = base64.b64decode(chain_b64)

print(f"Total chain length: {len(chain_bytes)} bytes")
print("Hex representation:", chain_bytes.hex())

# Let's parse the entries
# According to standard TS3 license/certificate format:
# A chain contains a sequence of entries. Let's see what they look like.
# Usually:
# - Type / Separator (1 byte or something?)
# Let's inspect the bytes:
# 01 00 95 5d 39 4a 17 e5 10 73 4c a0 6b df 5d 39 0f 45 24 2c 0b 68 9a ad 0d bd 46 ef 16 0f 32 f1 c5 33 02 0a 55 f2 80 0c 5e b3 00 07 00 00 00 20 41 6e 6f 6e 79 6d 6f 75 73 00
#
# Let's analyze:
# Offset 0: 01 (Type/Version?) -> Wait, in protocol_key.txt, it's 0x01
# Wait! Let's print each byte or chunk:
pos = 0
while pos < len(chain_bytes):
    print(f"\n--- Entry at offset {pos} ---")
    entry_type = chain_bytes[pos]
    print(f"Entry type/header: {entry_type:02X}")
    pos += 1
    
    # Let's look at public key. Ed25519 public key is 32 bytes.
    if pos + 32 <= len(chain_bytes):
        pub_key = chain_bytes[pos:pos+32]
        print(f"Public Key: {pub_key.hex()} (b64: {base64.b64encode(pub_key).decode()})")
        pos += 32
    else:
        break
        
    # License Type (1 byte)
    if pos < len(chain_bytes):
        license_type = chain_bytes[pos]
        print(f"License Type: {license_type:02X}")
        pos += 1
        
    # Timestamps (2 x 4 bytes)
    if pos + 8 <= len(chain_bytes):
        begin_raw = chain_bytes[pos:pos+4]
        end_raw = chain_bytes[pos+4:pos+8]
        begin = struct.unpack(">I", begin_raw)[0]
        end = struct.unpack(">I", end_raw)[0]
        print(f"Begin timestamp (raw): {begin} (actual Unix: {begin + 1356998400})")
        print(f"End timestamp (raw): {end} (actual Unix: {end + 1356998400})")
        pos += 8
        
    # Let's see what's next. 
    # For license entries, there is a body:
    # slots (4 bytes)? or size and string?
    # In the Anonymous chain, we see:
    # 00 07 00 00 00 20 41 6e 6f 6e 79 6d 6f 75 73 00
    # Let's look at these bytes:
    # 00 07 -> length of issuer name or license flags?
    # 00 00 00 20 -> slots = 32
    # 41 6e 6f 6e 79 6d 6f 75 73 00 -> "Anonymous\x00"
    if pos + 2 <= len(chain_bytes):
        body_type = struct.unpack(">H", chain_bytes[pos:pos+2])[0]
        print(f"Body Type / Info: {body_type:04X}")
        pos += 2
        
    if pos + 4 <= len(chain_bytes):
        slots = struct.unpack(">I", chain_bytes[pos:pos+4])[0]
        print(f"Slots: {slots}")
        pos += 4
        
    # Issuer name string? Let's read until 0x00
    issuer = b""
    while pos < len(chain_bytes):
        char = chain_bytes[pos]
        pos += 1
        if char == 0:
            break
        issuer += bytes([char])
    print(f"Issuer: {issuer.decode('utf-8', errors='ignore')}")
