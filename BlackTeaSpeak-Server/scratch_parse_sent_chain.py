import base64
import struct
import sys

# Fully robust printing
def safe_print(*args):
    try:
        sys.stdout.write(" ".join(str(a) for a in args) + "\n")
    except Exception:
        sys.stdout.write("<print error>\n")

sys.stdout.reconfigure(encoding='utf-8')

# Correct, full 384-character unescaped base64
data = base64.b64decode("AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaQBkm8jezhcNlInzCupVKh7+vZZJARoPWszOSwyoSSKpPSAZOn0UGWIKFOLyeiSiazgMrfE3EWNZPQrPOyPfoQYVIbb52rApa5saHYqdwy19tk0zg2NInTVfR5YZOSIvOjOCo/aLvn5W/AA=")
safe_print("Length:", len(data))
pos = 0
chain_type = data[pos]
safe_print("Chain type:", chain_type)
pos += 1
while pos < len(data):
    entry_start = pos
    entry_prefix = data[pos]
    if pos + 42 > len(data):
        safe_print(f"Remaining bytes at {pos}: {data[pos:].hex()}")
        break
    pubkey = data[pos+1:pos+33]
    license_type = data[pos+33]
    begin = struct.unpack(">I", data[pos+34:pos+38])[0]
    end = struct.unpack(">I", data[pos+38:pos+42])[0]
    pos += 42
    safe_print(f"Entry at {entry_start}: Type={license_type}, Pubkey={pubkey.hex()}")
    safe_print(f"  Begin={begin} ({begin+1356998400}), End={end} ({end+1356998400})")
    if license_type == 0:
        dummy = data[pos:pos+4]
        pos += 4
        issuer_bytes = bytearray()
        while pos < len(data):
            b = data[pos]
            pos += 1
            if b == 0: break
            issuer_bytes.append(b)
        safe_print("  Issuer:", issuer_bytes.hex())
    elif license_type == 5:
        safe_print("  License Sign entry")
    elif license_type == 0x20:
        safe_print("  Ephemeral entry")
        # Does it have a signature?
        if pos + 64 <= len(data):
            sig = data[pos:pos+64]
            safe_print("  Signature:", sig.hex())
            pos += 64
        else:
            safe_print(f"  No signature / remaining: {data[pos:].hex()}")
