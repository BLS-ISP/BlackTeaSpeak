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

# Field 1 nested field 1 starts at index 6 of key2_bytes (tag 0a, len 213)
# Let's verify by checking tag:
print(f"Tag at 3: {key2_bytes[3]:02x}")
print(f"Len at 4: {key2_bytes[4]:02x}")

nested_chain = key2_bytes[6:6+213]
print("\nNested Chain length:", len(nested_chain))

# Print byte by byte
for i in range(len(nested_chain)):
    b = nested_chain[i]
    c = chr(b) if 32 <= b <= 126 else '.'
    print(f"{i:03d} (0x{i:02x}): {b:02x} '{c}'")
