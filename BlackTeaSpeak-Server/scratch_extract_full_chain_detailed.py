import base64
import struct

key2_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
key2_bytes = base64.b64decode(key2_b64)

pos = 6
print(f"Total key2 length: {len(key2_bytes)}")
print(f"Starting parsing at pos {pos}, bytes: {key2_bytes[pos:pos+20].hex()}")

# Let's parse entries one by one
while pos < len(key2_bytes):
    print(f"\n--- At pos {pos} ---")
    prefix = key2_bytes[pos]
    print(f"Prefix byte: {prefix:02X}")
    pos += 1
    
    # Check if there is a separator (usually 0x00)
    separator = key2_bytes[pos]
    print(f"Separator byte: {separator:02X}")
    pos += 1
    
    pubkey = key2_bytes[pos:pos+32]
    print(f"Pubkey: {pubkey.hex()[:15]}...")
    pos += 32
    
    license_type = key2_bytes[pos]
    print(f"License type: {license_type}")
    pos += 1
    
    begin = struct.unpack(">I", key2_bytes[pos:pos+4])[0]
    end = struct.unpack(">I", key2_bytes[pos+4:pos+8])[0]
    print(f"Begin: {begin}, End: {end}")
    pos += 8
    
    body_len = struct.unpack(">H", key2_bytes[pos:pos+2])[0]
    print(f"Body len: {body_len}")
    pos += 2
    
    body = key2_bytes[pos:pos+body_len]
    print(f"Body: {body}")
    pos += body_len
    
    # Check if next bytes are a signature
    # In TS3, entries are chained. Let's see what the next bytes are:
    if pos < len(key2_bytes):
        next_byte = key2_bytes[pos]
        print(f"Next byte is: {next_byte:02X}")
        if next_byte != 0x00 and next_byte != 0x01:
            # It's a signature! Let's extract 64 bytes signature
            sig = key2_bytes[pos:pos+64]
            print(f"Signature: {sig.hex()[:15]}...")
            pos += 64
        else:
            print("Next byte is 0x00 or 0x01, so it is the next entry start.")
            
    # Let's break if we hit the end of the first chain (before the second activation)
    # The second activation starts with Co4C or similar.
    # Let's print the remaining bytes to see.
    if pos >= len(key2_bytes):
        print("Reached end of key2_bytes.")
        break
    
    if key2_bytes[pos] == 0x12:
        print(f"Encountered tag 0x12 (protobuf field) at pos {pos}, stopping first chain parsing.")
        break
