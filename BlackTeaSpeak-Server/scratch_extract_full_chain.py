import base64
import struct

key2_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
key2_bytes = base64.b64decode(key2_b64)

# Let's write a function that parses a list of entries from a buffer starting at a specific position.
def parse_entries_from(pos):
    entries = []
    while pos < len(key2_bytes):
        if pos + 2 > len(key2_bytes):
            break
        # Look for the export chain entry prefix 0x01, or separator 0x00
        # If it's a new entry, it starts with 0x00 or 0x01
        entry_start = pos
        prefix = key2_bytes[pos]
        if prefix != 0x00 and prefix != 0x01:
            print(f"Non-entry prefix {prefix:02X} at pos {pos}")
            break
        pos += 1
        
        if pos + 32 > len(key2_bytes):
            break
        pubkey = key2_bytes[pos:pos+32]
        pos += 32
        
        if pos >= len(key2_bytes):
            break
        license_type = key2_bytes[pos]
        pos += 1
        
        if pos + 8 > len(key2_bytes):
            break
        begin = struct.unpack(">I", key2_bytes[pos:pos+4])[0]
        end = struct.unpack(">I", key2_bytes[pos+4:pos+8])[0]
        pos += 8
        
        if pos + 2 > len(key2_bytes):
            break
        body_len = struct.unpack(">H", key2_bytes[pos:pos+2])[0]
        pos += 2
        
        if pos + body_len > len(key2_bytes):
            break
        body = key2_bytes[pos:pos+body_len]
        pos += body_len
        
        # Check if there is a signature for the entry (some entries like sub-licenses have a 64-byte signature)
        # In a serialized chain, sub-licenses signed by a parent key have a 64-byte signature appended!
        # Let's check if the entry type is sub-license (0x02 or 0x03 or similar) and signature is present.
        signature = b""
        # Let's see: usually, if it is a sub-license entry, it has a signature if it's part of a chain.
        # But wait, in a serialized chain, each entry (after the first root) has a 64-byte signature!
        # Let's look at the next bytes: if they are 64 bytes of signature, let's extract them.
        # How do we know? Let's check the type.
        print(f"Parsed Entry: Pubkey={pubkey.hex()[:10]}... Type={license_type} Begin={begin} End={end} BodyLen={body_len} Body={body}")
        
        # Let's see if the next bytes look like a signature or the start of a new entry (0x00/0x01)
        # If the next byte is NOT 0x00 or 0x01, it is probably a signature!
        if pos < len(key2_bytes) and key2_bytes[pos] != 0x00 and key2_bytes[pos] != 0x01:
            if pos + 64 <= len(key2_bytes):
                signature = key2_bytes[pos:pos+64]
                pos += 64
                print(f"  Extracted 64-byte signature: {signature.hex()[:15]}...")
        
        entry_data = key2_bytes[entry_start:pos]
        entries.append(entry_data)
    return entries

print("Parsing entries starting at byte 6:")
entries = parse_entries_from(6)

print(f"\nTotal entries parsed: {len(entries)}")
# Let's construct a full chain!
# A full chain starts with 0x01 (ExportChain type)
# followed by the entries appended together!
full_chain = bytearray([0x01])
for entry in entries:
    full_chain.extend(entry)

print("\n--- CONSTRUCTED FULL CHAIN ---")
print(base64.b64encode(full_chain).decode())
