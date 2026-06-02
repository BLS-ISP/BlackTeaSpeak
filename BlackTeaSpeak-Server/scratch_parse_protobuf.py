import base64

def parse_varint(data, pos):
    val = 0
    shift = 0
    while True:
        b = data[pos]
        pos += 1
        val |= (b & 0x7f) << shift
        if not (b & 0x80):
            break
        shift += 7
    return val, pos

# Read key2 block
with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
    lines = f.readlines()

key2_b64 = ""
is_key2 = False
for line in lines:
    trimmed = line.trim() if hasattr(line, "trim") else line.strip()
    if trimmed == "==key2==":
        is_key2 = not is_key2
        continue
    if is_key2:
        key2_b64 += trimmed

key2_bytes = base64.b64decode(key2_b64)
print("Total key2 bytes:", len(key2_bytes))

# Parse protobuf fields
pos = 0
while pos < len(key2_bytes):
    tag_pos = pos
    tag, pos = parse_varint(key2_bytes, pos)
    field_number = tag >> 3
    wire_type = tag & 7
    print(f"Pos {tag_pos}: Field {field_number}, Wire {wire_type}")
    
    if wire_type == 2: # length-delimited
        length, pos = parse_varint(key2_bytes, pos)
        field_data = key2_bytes[pos:pos+length]
        print(f"  Length: {length}")
        print(f"  Data hex (first 30): {field_data[:30].hex()}")
        pos += length
    elif wire_type == 0: # varint
        val, pos = parse_varint(key2_bytes, pos)
        print(f"  Value: {val}")
    else:
        print("  Unsupported wire type!")
        break
