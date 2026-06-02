import base64

key2_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
key2_bytes = base64.b64decode(key2_b64)

# A TS3 license has two main parts in key2:
# Usually, field 1 is the license chain or license data.
# Let's inspect where AQCvbHFT appears.
# AQCvbHFT... starts with byte sequence: 01 00 af 6c 71 53 40 36 3f b5 ea cf 7a 29 6b a7 f1 02 53 dc 42 1f 95 37 c4 2f 76 95 84 cd 69 8f f4 29
target = base64.b64decode("AQCvbHFT")
occurrences = []
pos = 0
while True:
    idx = key2_bytes.find(target, pos)
    if idx == -1:
        break
    occurrences.append(idx)
    pos = idx + 1

print("Found target occurrences at indexes:", occurrences)

for i, idx in enumerate(occurrences):
    print(f"\n--- Occurrence {i} at byte {idx} ---")
    # Let's see the length of the chain.
    # A chain entry in TS3 contains:
    # 1 byte prefix (0x01)
    # 1 byte entry separator (0x00)
    # 32 bytes public key
    # 1 byte type
    # 4 bytes begin
    # 4 bytes end
    # 2 bytes body length (say L)
    # L bytes body
    #
    # Let's parse this entry:
    e_pos = idx
    print("Prefix/Separator:", key2_bytes[e_pos:e_pos+2].hex())
    pubkey = key2_bytes[e_pos+2:e_pos+34]
    print("Pubkey:", pubkey.hex(), "b64:", base64.b64encode(pubkey).decode())
    l_type = key2_bytes[e_pos+34]
    print("License type:", l_type)
    import struct
    begin = struct.unpack(">I", key2_bytes[e_pos+35:e_pos+39])[0]
    end = struct.unpack(">I", key2_bytes[e_pos+39:e_pos+43])[0]
    print(f"Begin: {begin} ({begin + 1356998400}), End: {end} ({end + 1356998400})")
    
    body_len = struct.unpack(">H", key2_bytes[e_pos+43:e_pos+45])[0]
    print("Body length:", body_len)
    
    body = key2_bytes[e_pos+45:e_pos+45+body_len]
    print("Body string:", body)
    
    total_entry_len = 45 + body_len
    print("Total entry length:", total_entry_len)
    
    chain_bytes = key2_bytes[e_pos:e_pos+total_entry_len]
    print("Chain b64:", base64.b64encode(chain_bytes).decode())
