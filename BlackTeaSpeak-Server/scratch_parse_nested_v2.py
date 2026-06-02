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
if chain2_data[0] != 1:
    print("Invalid Chain 2 prefix")
    exit(1)

# Recursive V2 entry parser
def parse_v2_entry(buffer, offset, parent_label="Root"):
    entry_start = offset
    entry_type = buffer[offset]
    pubkey = buffer[offset+1 : offset+33]
    ts_begin = struct.unpack("<I", buffer[offset+33 : offset+37])[0]
    ts_end = struct.unpack("<I", buffer[offset+37 : offset+41])[0]
    body_len = struct.unpack("<H", buffer[offset+41 : offset+43])[0]
    offset += 43
    
    body = buffer[offset : offset+body_len]
    entry_end = offset + body_len
    entry_bytes = buffer[entry_start:entry_end]
    
    print(f"\n--- Entry type {entry_type} ({parent_label} -> Child) ---")
    print(f"  Start offset: {entry_start}")
    print(f"  End offset:   {entry_end}")
    print(f"  Length:       {len(entry_bytes)}")
    print(f"  Pubkey:       {pubkey.hex()}")
    print(f"  Begin:        {ts_begin}")
    print(f"  End:          {ts_end}")
    print(f"  Body len:     {body_len}")
    print(f"  Entry raw:    {entry_bytes.hex()}")
    
    # Within the body of an intermediate entry (Type 0), the body structure is:
    # [4 bytes dummy] [null-terminated issuer string] [next entry raw bytes...]
    if entry_type == 0:
        # Locate null-terminator for issuer
        dummy = body[0:4]
        issuer_bytes = bytearray()
        idx = 4
        while idx < len(body):
            b = body[idx]
            idx += 1
            if b == 0:
                break
            issuer_bytes.append(b)
        issuer = issuer_bytes.decode('utf-8', errors='ignore')
        print(f"  Issuer:       {issuer}")
        print(f"  Dummy:        {dummy.hex()}")
        
        # If there are remaining bytes in the body, it is the next nested entry!
        if idx < len(body):
            parse_v2_entry(body, idx, parent_label=f"Entry(Type 0, {pubkey.hex()[:6]})")
            
    elif entry_type == 2: # Server
        srv_type = body[0]
        slots = struct.unpack("<I", body[1:5])[0]
        issuer_bytes = bytearray()
        idx = 5
        while idx < len(body):
            b = body[idx]
            idx += 1
            if b == 0:
                break
            issuer_bytes.append(b)
        issuer = issuer_bytes.decode('utf-8', errors='ignore')
        print(f"  Issuer:       {issuer}")
        print(f"  Server type:  {srv_type}")
        print(f"  Slots:        {slots}")
        
    elif entry_type == 5: # License Sign
        print("  License Sign (no body elements)")

# Parse Chain 2 starting at offset 1 (skipping the chain type byte 0x01)
parse_v2_entry(chain2_data, 1)
