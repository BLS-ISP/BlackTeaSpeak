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
print(f"Total key2 bytes: {len(key2_bytes)}")

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

# Field 1 is Chain 1, Field 2 is Chain 2
for field_num, data in sorted(chains.items()):
    if field_num not in [1, 2]:
        continue
    print(f"\n==================== Field {field_num} (Chain) ====================")
    if data[0] != 1:
        print(f"Error: Invalid chain prefix: {data[0]}")
        continue
    
    pos = 1
    entry_idx = 0
    while pos < len(data):
        entry_start = pos
        if pos + 43 > len(data):
            print(f"Remaining trailing bytes: {data[pos:].hex()}")
            break
            
        entry_type = data[pos]
        pubkey = data[pos+1 : pos+33]
        ts_begin = struct.unpack("<I", data[pos+33 : pos+37])[0]
        ts_end = struct.unpack("<I", data[pos+37 : pos+41])[0]
        body_len = struct.unpack("<H", data[pos+41 : pos+43])[0]
        
        pos += 43
        body = data[pos : pos+body_len]
        pos += body_len
        
        entry_data = data[entry_start:pos]
        print(f"\n--- Entry {entry_idx} ---")
        print(f"  Start:      {entry_start}")
        print(f"  Type:       {entry_type}")
        print(f"  Pubkey:     {pubkey.hex()}")
        print(f"  Begin:      {ts_begin}")
        print(f"  End:        {ts_end}")
        print(f"  Body length:{body_len}")
        print(f"  Body hex:   {body.hex()}")
        print(f"  Entry hex:  {entry_data.hex()}")
        entry_idx += 1
