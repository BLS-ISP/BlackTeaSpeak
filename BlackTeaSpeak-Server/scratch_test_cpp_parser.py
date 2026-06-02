import base64

lines = open("../licensekey.dat", "r").readlines()
key2_b64 = ""
is_key2 = False

for line in lines:
    # Strip carriage returns and spaces
    line = line.strip()
    # Check C++ style prefix match: line.rfind("==key==", 0) == 0
    if line.startswith("==key=="):
        is_key2 = not is_key2
        continue
    if is_key2:
        key2_b64 += line

print(f"Combined base64 length: {len(key2_b64)}")
try:
    decoded = base64.b64decode(key2_b64)
    print(f"Decoded length: {len(decoded)}")
except Exception as e:
    print(f"Decode failed: {e}")
