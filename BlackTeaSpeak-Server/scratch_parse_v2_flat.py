import base64
import struct

# Read ==key2== from licensekey.dat
key2_b64 = ""
with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
    is_key2 = False
    for line in f:
        if line.startswith("==key2=="):
            is_key2 = not is_key2
            continue
        if is_key2:
            key2_b64 += line.strip()

key2_bytes = base64.b64decode(key2_b64)

# Parse protobuf fields
offset = 0
chains = {}
while offset < len(key2_bytes):
    tag_byte = key2_bytes[offset]
    field_num = tag_byte >> 3
    wire_type = tag_byte & 0x07
    offset += 1
    
    if wire_type == 2:
        length = 0
        shift = 0
        while True:
            b = key2_bytes[offset]
            offset += 1
            length |= (b & 0x7f) << shift
            shift += 7
            if b & 0x80 == 0:
                break
        field_data = key2_bytes[offset:offset+length]
        chains[field_num] = field_data
        offset += length

chain2_data = chains[2]
print(f"Chain 2 data length: {len(chain2_data)}")
print(f"Chain 2 hex: {chain2_data.hex()}")

# Linear parsing
pos = 1 # Skip chain type byte
entry_idx = 0
while pos < len(chain2_data):
    entry_start = pos
    if pos + 43 > len(chain2_data):
        print(f"\nRemaining bytes at pos {pos}: {chain2_data[pos:].hex()}")
        break
        
    entry_type = chain2_data[pos]
    pubkey = chain2_data[pos+1 : pos+33]
    
    # Let's test both LE and BE unpacks for timestamps and body_len
    ts_begin_le = struct.unpack("<I", chain2_data[pos+33 : pos+37])[0]
    ts_begin_be = struct.unpack(">I", chain2_data[pos+33 : pos+37])[0]
    
    ts_end_le = struct.unpack("<I", chain2_data[pos+37 : pos+41])[0]
    ts_end_be = struct.unpack(">I", chain2_data[pos+37 : pos+41])[0]
    
    body_len_le = struct.unpack("<H", chain2_data[pos+41 : pos+43])[0]
    body_len_be = struct.unpack(">H", chain2_data[pos+41 : pos+43])[0]
    
    print(f"\n--- Entry {entry_idx} at offset {entry_start} ---")
    print(f"  Type:          {entry_type}")
    print(f"  Pubkey:        {pubkey.hex()}")
    print(f"  Begin (LE/BE): {ts_begin_le} / {ts_begin_be}")
    print(f"  End (LE/BE):   {ts_end_le} / {ts_end_be}")
    print(f"  BodyLen(LE/BE):{body_len_le} / {body_len_be}")
    
    # We will assume BE for lengths to see if it makes sense
    chosen_len = body_len_be
    # If BE is too large, try LE
    if pos + 43 + chosen_len > len(chain2_data):
        chosen_len = body_len_le
        print(f"  [Using LE body length: {chosen_len}]")
    else:
        print(f"  [Using BE body length: {chosen_len}]")
        
    body = chain2_data[pos+43 : pos+43+chosen_len]
    pos += 43 + chosen_len
    entry_bytes = chain2_data[entry_start:pos]
    print(f"  Body hex:      {body.hex()}")
    print(f"  Entry raw hex: {entry_bytes.hex()}")
    entry_idx += 1
