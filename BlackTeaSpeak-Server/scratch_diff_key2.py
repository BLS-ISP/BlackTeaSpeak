import base64

old_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="

with open("../licensekey.dat", "r") as f:
    is_key2 = False
    new_b64 = ""
    for line in f:
        if line.startswith("==key2=="):
            is_key2 = not is_key2
            continue
        if is_key2:
            new_b64 += line.strip()

old_bytes = base64.b64decode(old_b64)
new_bytes = base64.b64decode(new_b64)

print("Old key2 length:", len(old_bytes))
print("New key2 length:", len(new_bytes))

diffs = []
for i in range(len(old_bytes)):
    if old_bytes[i] != new_bytes[i]:
        diffs.append((i, old_bytes[i], new_bytes[i]))

print(f"Number of differing bytes: {len(diffs)}")
for idx, o_b, n_b in diffs[:20]:
    print(f"  Byte {idx}: {o_b:02X} -> {n_b:02X}")
