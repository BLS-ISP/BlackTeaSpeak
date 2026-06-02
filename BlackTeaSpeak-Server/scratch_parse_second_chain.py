import base64
import struct

chain2_b64 = "AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
chain_bytes = base64.b64decode(chain2_b64)

print("Second chain length:", len(chain_bytes))

pos = 0
prefix = chain_bytes[pos]
print(f"Chain type prefix: {prefix:02X}")
pos += 1

while pos < len(chain_bytes):
    print(f"\n--- Entry at pos {pos} ---")
    separator = chain_bytes[pos]
    print(f"Separator: {separator:02X}")
    pos += 1
    
    pubkey = chain_bytes[pos:pos+32]
    print(f"Pubkey: {pubkey.hex()} (b64: {base64.b64encode(pubkey).decode()})")
    pos += 32
    
    license_type = chain_bytes[pos]
    print(f"License type: {license_type}")
    pos += 1
    
    begin = struct.unpack(">I", chain_bytes[pos:pos+4])[0]
    end = struct.unpack(">I", chain_bytes[pos+4:pos+8])[0]
    print(f"Begin: {begin} ({begin + 1356998400}), End: {end} ({end + 1356998400})")
    pos += 8
    
    body_len = struct.unpack(">H", chain_bytes[pos:pos+2])[0]
    print(f"Body length: {body_len}")
    pos += 2
    
    body = chain_bytes[pos:pos+body_len]
    print(f"Body hex: {body.hex()}")
    pos += body_len
    
    # Check for signature
    if pos < len(chain_bytes):
        next_byte = chain_bytes[pos]
        if next_byte != 0x00 and next_byte != 0x01:
            if pos + 64 <= len(chain_bytes):
                sig = chain_bytes[pos:pos+64]
                print(f"Signature: {sig.hex()[:15]}...")
                pos += 64
            else:
                print(f"Remaining bytes {len(chain_bytes) - pos} are too short for signature.")
                break
