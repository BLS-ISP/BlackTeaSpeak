import base64
import struct

chain_b64 = "AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIA"
chain_bytes = base64.b64decode(chain_b64)

print(f"Working chain length: {len(chain_bytes)} bytes")

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
    print(f"Body string: {body.decode('utf-8', errors='ignore')}")
    pos += body_len
    
    # Check for signature
    if pos < len(chain_bytes):
        # In a serialized chain, if the next byte is NOT 0x00, it's a signature
        next_byte = chain_bytes[pos]
        if next_byte != 0x00 and next_byte != 0x01:
            if pos + 64 <= len(chain_bytes):
                sig = chain_bytes[pos:pos+64]
                print(f"Signature: {sig.hex()[:15]}...")
                pos += 64
            else:
                print(f"Remaining bytes {len(chain_bytes) - pos} are too short for signature.")
                break
