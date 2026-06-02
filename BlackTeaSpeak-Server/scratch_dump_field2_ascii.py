import base64

with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
    lines = f.readlines()

key2_b64 = ""
is_key2 = False
for line in lines:
    trimmed = line.strip()
    if trimmed == "==key2==":
        is_key2 = not is_key2
        continue
    if is_key2:
        key2_b64 += trimmed

key2_bytes = base64.b64decode(key2_b64)

# Field 2 starts at index 276
f2_data = key2_bytes[276:276+181]
print("Field 2 length:", len(f2_data))

# Print byte by byte
for i in range(len(f2_data)):
    b = f2_data[i]
    c = chr(b) if 32 <= b <= 126 else '.'
    print(f"{i:03d} (0x{i:02x}): {b:02x} '{c}'")
