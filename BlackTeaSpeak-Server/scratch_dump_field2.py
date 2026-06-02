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

print("Field 2 length:", len(field2_data))
print("Field 2 raw hex:")
print(field2_data.hex())

# Let's print around offset 33 (which is after pubkey 0)
# Entry 0: starts at 1
# entry_type: 1 (at 1)
# pubkey: 32 (at 2..34)
# begin: 4 (at 34..38)
# end: 4 (at 38..42)
# body_len: 2 (at 42..44)
print("\nEntry 0 header details:")
print("  Type:    ", field2_data[1])
print("  Pubkey:  ", field2_data[2:34].hex())
print("  Begin:   ", field2_data[34:38].hex())
print("  End:     ", field2_data[38:42].hex())
print("  BodyLen: ", field2_data[42:44].hex())
print("  Body (next 30 bytes):", field2_data[44:74].hex())
