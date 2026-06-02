import base64
import struct

# Read ==key2== from licensekey.dat
key2_b64 = ""
with open("../licensekey.dat", "r") as f:
    is_key2 = False
    for line in f:
        if line.startswith("==key2=="):
            is_key2 = not is_key2
            continue
        if is_key2:
            key2_b64 += line.strip()

key2_bytes = base64.b64decode(key2_b64)
print(f"Total key2 length: {len(key2_bytes)}")

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

# Look at key2_bytes structure.
# Proto header:
# Let's inspect where Chain 1 and Chain 2 actually are.
# From the old key2, Chain 1 was at [6:276], and Chain 2 was at [276:]. Let's see if this matches or if we can automatically find them.
# The protobuf headers in key2:
# key2_bytes starts with Co4C (protobuf field 1 with tag 2 for LicenseChain or similar)
# Let's skip protobuf tag headers:
pos = 0
while pos < len(key2_bytes) and key2_bytes[pos] == 0x0a:
    pos += 1
    length = 0
    shift = 0
    while True:
        b = key2_bytes[pos]
        pos += 1
        length |= (b & 0x7f) << shift
        shift += 7
        if b & 0x80 == 0:
            break
    print(f"Protobuf field len: {length} starting at {pos}")

# Let's parse all bytes starting from pos as entries!
# Let's find where the 0x01 bytes are (chain headers)
chain_starts = []
for i in range(len(key2_bytes)):
    if key2_bytes[i] == 0x01:
        # Check if the next byte is 0x00
        if i + 1 < len(key2_bytes) and key2_bytes[i+1] == 0x00:
            chain_starts.append(i)

print("Chain starts (0x01, 0x00) found at:", chain_starts)

if len(chain_starts) >= 1:
    end_0 = chain_starts[1] if len(chain_starts) > 1 else len(key2_bytes)
    parse_chain_entries("Chain 1", key2_bytes[chain_starts[0]:end_0])
if len(chain_starts) >= 2:
    parse_chain_entries("Chain 2", key2_bytes[chain_starts[1]:])
