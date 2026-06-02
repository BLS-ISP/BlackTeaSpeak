import base64

key2_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVhayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
key2_bytes = base64.b64decode(key2_b64)

print(f"Length of key2: {len(key2_bytes)} bytes")
print("Hex representation:")
print(key2_bytes.hex())

# Let's see if we can find any base64 sub-strings or chains inside.
# Usually, a licensekey.dat contains serialized protobuf.
# Let's search for the pattern "AQCvbHFT" which is the chain we saw in protocol_key.txt!
target = base64.b64decode("AQCvbHFT")
idx = key2_bytes.find(target)
if idx != -1:
    print(f"\nFound chain prefix at byte {idx}!")
    # Let's see how much we can read from here.
    # The chain prefix is AQCvbHFT... Let's extract and decode it.
    chain_candidate = key2_bytes[idx:]
    print("Candidate chain bytes hex:", chain_candidate[:200].hex())
else:
    print("\nChain prefix 'AQCvbHFT' not found in raw bytes of key2.")
