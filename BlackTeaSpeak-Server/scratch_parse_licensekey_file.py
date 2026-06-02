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

# Load key2 dynamically from licensekey.dat
key2_b64 = ""
with open("d:\\projekt\\BlackTeaSpeak\\licensekey.dat", "r") as f:
    is_key2 = False
    for line in f:
        if line.startswith("==key2=="):
            is_key2 = not is_key2
            continue
        if is_key2:
            key2_b64 += line.strip()

key2_bytes = base64.b64decode(key2_b64)
print(f"Loaded key2: {len(key2_bytes)} bytes")

# Search for chain start AQCvbHFT (b64) -> hex 01 00 af 6c 71 53 ...
target = base64.b64decode("AQCvbHFT")
occurrences = []
pos = 0
while True:
    idx = key2_bytes.find(target, pos)
    if idx == -1:
        break
    occurrences.append(idx)
    pos = idx + 1

print("Found chain starts at:", occurrences)

if len(occurrences) >= 2:
    # First chain is between occurrence 0 and occurrence 1 (minus protobuf metadata)
    chain1_data = key2_bytes[occurrences[0]:occurrences[1]]
    # Let's strip protobuf field descriptors from the end of chain1
    # Typically, the next field starts with 0x12 followed by length
    meta_idx = chain1_data.rfind(b"\x12")
    if meta_idx != -1:
        chain1_data = chain1_data[:meta_idx]
    
    # Second chain starts at occurrence 1
    chain2_data = key2_bytes[occurrences[1]:]
    meta_idx2 = chain2_data.rfind(b"\x1a")
    if meta_idx2 != -1:
        chain2_data = chain2_data[:meta_idx2]
        
    parse_chain_entries("Chain 1", chain1_data)
    parse_chain_entries("Chain 2", chain2_data)
    
    # Print root key private key (field 2)
    # Protobuf tag for field 2 is 0x12 (length delimited)
    # Let's extract the private key embedded between Chain 1 and Chain 2
    # The structure of key2 is:
    # tag 1 (0x0a) [length] [Chain 1]
    # tag 2 (0x12) [length] [Private Key 32 bytes]
    # tag 3 (0x1a) [length] [Chain 2]
    #
    # Let's verify this structure:
    # Let's parse all protobuf fields!
    print("\n==================== Protobuf Fields ====================")
    offset = 0
    while offset < len(key2_bytes):
        if offset >= len(key2_bytes):
            break
        tag_byte = key2_bytes[offset]
        field_num = tag_byte >> 3
        wire_type = tag_byte & 0x07
        offset += 1
        
        # Read varint length for length-delimited fields (wire type 2)
        if wire_type == 2:
            length = 0
            shift = 0
            while True:
                b = key2_bytes[offset]
                offset += 1
                length |= (b & 0x7f) << shift
                shift += 7
                if b & 0x80 == 0:
                    break
            print(f"Field {field_num}: length={length}, type=length-delimited")
            field_data = key2_bytes[offset:offset+length]
            if field_num == 1:
                print(f"  Field 1 (Chain 1) b64: {base64.b64encode(field_data).decode()}")
            elif field_num == 2:
                print(f"  Field 2 (Server identity base private key?):")
                print(f"    Hex: {field_data.hex()}")
                print(f"    Base64: {base64.b64encode(field_data).decode()}")
            elif field_num == 3:
                print(f"  Field 3 (Chain 2) b64: {base64.b64encode(field_data).decode()}")
            offset += length
        else:
            print(f"Unknown wire type {wire_type} for field {field_num}")
            break
