import base64
import struct

key2_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
key2_bytes = base64.b64decode(key2_b64)

def parse_chain_entries(label, data):
    print(f"\n==================== {label} ====================")
    pos = 0
    if len(data) == 0:
        print("Empty data")
        return
        
    chain_type = data[pos]
    if chain_type != 1:
        print(f"Error: Invalid chain type: {chain_type}")
        return
    pos += 1
    
    entry_index = 0
    while pos < len(data):
        entry_start = pos
        if pos + 42 > len(data):
            print(f"Reached end of stream. Remaining bytes: {len(data) - pos} (hex: {data[pos:].hex()})")
            break
            
        entry_prefix = data[pos]
        if entry_prefix != 0x00:
            print(f"Error: Invalid entry prefix {entry_prefix:02X} at pos {pos}")
            break
        
        pubkey = data[pos+1:pos+33]
        license_type = data[pos+33]
        begin = struct.unpack(">I", data[pos+34:pos+38])[0]
        end = struct.unpack(">I", data[pos+38:pos+42])[0]
        pos += 42
        
        print(f"\n--- Entry {entry_index} (Type: {license_type} | Offset: {entry_start}) ---")
        print(f"  Pubkey: {pubkey.hex()} (b64: {base64.b64encode(pubkey).decode()})")
        print(f"  Begin : {begin} (Unix: {begin + 1356998400})")
        print(f"  End   : {end} (Unix: {end + 1356998400})")
        
        # Read content depending on license type
        if license_type == 0x00: # Intermediate
            if pos + 4 > len(data):
                print("Error: Missing dummy field in Intermediate entry")
                break
            dummy = data[pos:pos+4]
            pos += 4
            # Read null-terminated string
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
            
        elif license_type == 0x02: # Server
            if pos + 5 > len(data):
                print("Error: Missing slots or licenseType in Server entry")
                break
            srv_license_type = data[pos]
            slots = struct.unpack(">I", data[pos+1:pos+5])[0]
            pos += 5
            # Read null-terminated string
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
            
        elif license_type == 0x03: # Code
            issuer_bytes = bytearray()
            while pos < len(data):
                b = data[pos]
                pos += 1
                if b == 0:
                    break
                issuer_bytes.append(b)
            issuer = issuer_bytes.decode('utf-8', errors='ignore')
            print(f"  Issuer: {issuer}")
            
        elif license_type == 0x05: # License Sign
            print("  License Sign entry (no body)")
            
        elif license_type == 0x20: # Ephemeral
            print("  Ephemeral entry (no body)")
            
        else:
            print(f"Unknown license entry type: {license_type}")
            break
            
        entry_len = pos - entry_start
        print(f"  Entry raw bytes: {data[entry_start:pos].hex()}")
        entry_index += 1

parse_chain_entries("Chain 1", key2_bytes[6:276])
parse_chain_entries("Chain 2", key2_bytes[276:])
