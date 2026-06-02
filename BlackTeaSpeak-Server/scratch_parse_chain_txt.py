import base64
import struct

with open("protocol_key.txt", "r") as f:
    for line in f:
        if line.startswith("chain:"):
            chain_b64 = line.split("chain:")[1].strip()
            break

chain_bytes = base64.b64decode(chain_b64)
print(f"Decoded chain length: {len(chain_bytes)}")

# Parse using the correct entry parser
pos = 0
chain_type = chain_bytes[pos]
print(f"Chain type: {chain_type}")
pos += 1

entry_index = 0
while pos < len(chain_bytes):
    entry_start = pos
    if pos + 44 > len(chain_bytes):
        print(f"Remaining trailing bytes: {len(chain_bytes) - pos} (hex: {chain_bytes[pos:].hex()})")
        break
        
    prefix = chain_bytes[pos]
    pubkey = chain_bytes[pos+1:pos+33]
    license_type = chain_bytes[pos+33]
    begin = struct.unpack(">I", chain_bytes[pos+34:pos+38])[0]
    end = struct.unpack(">I", chain_bytes[pos+38:pos+42])[0]
    body_len = struct.unpack(">H", chain_bytes[pos+42:pos+44])[0]
    pos += 44
    
    print(f"\n--- Entry {entry_index} (Type: {license_type} | Offset: {entry_start}) ---")
    print(f"  Pubkey   : {pubkey.hex()}")
    print(f"  Begin/End: {begin} / {end}")
    print(f"  BodyLen  : {body_len}")
    
    body = chain_bytes[pos:pos+body_len]
    pos += body_len
    print(f"  Body hex : {body.hex()}")
    
    # Check if signed by parent (if there are more bytes and next byte is not 0x00/0x01)
    signature = b""
    if pos < len(chain_bytes) and chain_bytes[pos] != 0x00 and chain_bytes[pos] != 0x01:
        if pos + 64 <= len(chain_bytes):
            signature = chain_bytes[pos:pos+64]
            pos += 64
            print(f"  Signature: {signature.hex()}")
    
    entry_raw = chain_bytes[entry_start:pos]
    print(f"  Entry raw len: {len(entry_raw)}")
    entry_index += 1
