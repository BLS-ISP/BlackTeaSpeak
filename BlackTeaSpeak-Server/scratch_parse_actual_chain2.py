import base64
import struct

with open("protocol_key.txt", "r") as f:
    content = f.read()
chain_b64 = ""
for line in content.splitlines():
    if line.startswith("chain:"):
        chain_b64 = line.split("chain:")[1].strip()

key2_bytes = base64.b64decode(chain_b64)
print("Total key2_bytes length:", len(key2_bytes))

# Parse entries from Chain 2 (Field 2 of key2, starting at offset 3 of the tag-delimited field? No, let's see. 
# Wait, let's just parse the whole key2_bytes as a protobuf or let's parse from where Chain 2 starts!
# Let's find occurrences of AQCvbHFT (01 00 af 6c 71 53) in key2_bytes:
target = b"\x01\x00\xaf\x6c"
occurrences = []
pos = 0
while True:
    idx = key2_bytes.find(target, pos)
    if idx == -1:
        break
    occurrences.append(idx)
    pos = idx + 1
print("Occurrences of chain starts:", occurrences)

# If there is only one occurrence at 0, then the decoded bytes is just a single chain!
# Let's write a general entry parser for the decoded bytes.
def parse_chain_from_bytes(data):
    pos = 0
    chain_type = data[pos]
    print(f"Chain type: {chain_type}")
    pos += 1
    
    entry_index = 0
    while pos < len(data):
        entry_start = pos
        if pos + 44 > len(data):
            print(f"Remaining trailing bytes: {len(data) - pos} (hex: {data[pos:].hex()})")
            break
            
        prefix = data[pos]
        pubkey = data[pos+1:pos+33]
        license_type = data[pos+33]
        begin = struct.unpack(">I", data[pos+34:pos+38])[0]
        end = struct.unpack(">I", data[pos+38:pos+42])[0]
        body_len = struct.unpack(">H", data[pos+42:pos+44])[0]
        pos += 44
        
        print(f"\n--- Entry {entry_index} (Type: {license_type} | Offset: {entry_start}) ---")
        print(f"  Pubkey   : {pubkey.hex()}")
        print(f"  Begin/End: {begin} / {end}")
        print(f"  BodyLen  : {body_len}")
        
        body = data[pos:pos+body_len]
        pos += body_len
        print(f"  Body hex : {body.hex()}")
        
        # Check if signed by parent (if there are more bytes and next byte is not 0x00/0x01)
        signature = b""
        if pos < len(data) and data[pos] != 0x00 and data[pos] != 0x01:
            if pos + 64 <= len(data):
                signature = data[pos:pos+64]
                pos += 64
                print(f"  Signature: {signature.hex()}")
        
        entry_raw = data[entry_start:pos]
        print(f"  Entry raw len: {len(entry_raw)}")
        entry_index += 1

parse_chain_from_bytes(key2_bytes[276:276+181])
