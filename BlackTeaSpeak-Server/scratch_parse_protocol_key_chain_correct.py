import base64
import struct

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
            
        elif license_type == 0x02: # Server
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

# Load chain from protocol_key.txt
with open("protocol_key.txt", "r") as f:
    content = f.read()
chain_b64 = ""
for line in content.splitlines():
    if line.startswith("chain:"):
        chain_b64 = line.split("chain:")[1].strip()

data = base64.b64decode(chain_b64)
parse_chain_entries("Active Chain", data)
