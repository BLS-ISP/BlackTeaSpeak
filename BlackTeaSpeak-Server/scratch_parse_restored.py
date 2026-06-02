import base64
import struct
import sys

sys.stdout.reconfigure(encoding='utf-8')

data = base64.b64decode("AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ==")
print("Length:", len(data))
pos = 0
chain_type = data[pos]
print("Chain type:", chain_type)
pos += 1
while pos < len(data):
    entry_start = pos
    entry_prefix = data[pos]
    if pos + 42 > len(data):
        print(f"Remaining bytes at {pos}: {data[pos:].hex()}")
        break
    pubkey = data[pos+1:pos+33]
    license_type = data[pos+33]
    begin = struct.unpack(">I", data[pos+34:pos+38])[0]
    end = struct.unpack(">I", data[pos+38:pos+42])[0]
    pos += 42
    print(f"Entry at {entry_start}: Type={license_type}, Pubkey={pubkey.hex()}")
    print(f"  Begin={begin} ({begin+1356998400}), End={end} ({end+1356998400})")
    if license_type == 0:
        dummy = data[pos:pos+4]
        pos += 4
        issuer_bytes = bytearray()
        while pos < len(data):
            b = data[pos]
            pos += 1
            if b == 0: break
            issuer_bytes.append(b)
        print("  Issuer:", issuer_bytes.decode('utf-8', errors='ignore'))
    elif license_type == 5:
        print("  License Sign entry")
    elif license_type == 0x20:
        print("  Ephemeral entry")
        if pos + 64 <= len(data):
            sig = data[pos:pos+64]
            print("  Signature:", sig.hex())
            pos += 64
        else:
            print(f"  No signature / remaining: {data[pos:].hex()}")
