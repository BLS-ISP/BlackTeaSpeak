import base64
import struct

chain_b64 = "AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
data = base64.b64decode(chain_b64)
print("Chain length:", len(data))

def parse_chain_entries(label, data):
    print(f"\n==================== {label} ====================")
    pos = 0
    if len(data) == 0:
        return
    chain_type = data[pos]
    pos += 1
    
    entry_index = 0
    while pos < len(data):
        entry_start = pos
        if pos + 42 > len(data):
            print(f"Remaining bytes: {len(data) - pos} (hex: {data[pos:].hex()})")
            break
            
        entry_prefix = data[pos]
        pubkey = data[pos+1:pos+33]
        license_type = data[pos+33]
        begin = struct.unpack(">I", data[pos+34:pos+38])[0]
        end = struct.unpack(">I", data[pos+38:pos+42])[0]
        pos += 42
        
        print(f"\n--- Entry {entry_index} (Type: {license_type} | Offset: {entry_start}) ---")
        print(f"  Pubkey: {pubkey.hex()}")
        print(f"  Begin : {begin}")
        print(f"  End   : {end}")
        
        if license_type == 0x00:
            dummy = data[pos:pos+4]
            pos += 4
            issuer_bytes = bytearray()
            while pos < len(data):
                b = data[pos]
                pos += 1
                if b == 0:
                    break
                issuer_bytes.append(b)
            issuer = issuer_bytes.decode('utf-8', errors='ignore')
            print(f"  Issuer: {issuer}")
            print(f"  Dummy : {dummy.hex()}")
        elif license_type == 0x02:
            srv_license_type = data[pos]
            slots = struct.unpack(">I", data[pos+1:pos+5])[0]
            pos += 5
            issuer_bytes = bytearray()
            while pos < len(data):
                b = data[pos]
                pos += 1
                if b == 0:
                    break
                issuer_bytes.append(b)
            issuer = issuer_bytes.decode('utf-8', errors='ignore')
            print(f"  Issuer: {issuer}")
            print(f"  ServerLicenseType: {srv_license_type}")
            print(f"  Slots : {slots}")
        elif license_type == 0x05:
            print("  License Sign entry (no body)")
        else:
            print(f"Unknown type: {license_type}")
            break
        entry_index += 1

parse_chain_entries("Decoded Chain", data)
