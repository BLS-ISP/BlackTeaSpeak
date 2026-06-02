import base64

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

# Extract Field 2
offset = 0
field2_data = None
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
        if field_num == 2:
            field2_data = field_data
            break
        offset += length

print(f"Total field2_data length: {len(field2_data)}")
for i in range(len(field2_data)):
    b = field2_data[i]
    char = chr(b) if 32 <= b < 127 else '.'
    print(f"{i:03d} (0x{i:02x}): {b:02x} '{char}'")
