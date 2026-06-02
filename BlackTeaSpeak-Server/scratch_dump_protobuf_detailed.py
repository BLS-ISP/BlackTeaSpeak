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

# Field 1
pos = 0
tag, pos = 1, 1 # First field tag
length = 270
f1_data = key2_bytes[2:2+length]
print("Field 1 total length:", len(f1_data))
print("Field 1 hex:")
print(f1_data.hex())

# Field 2
f2_data = key2_bytes[pos+length+2:pos+length+2+181]
print("\nField 2 total length:", len(f2_data))
print("Field 2 hex:")
print(f2_data.hex())
