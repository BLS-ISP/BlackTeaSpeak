import base64

key2_b64 = "Co4CCtUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAYgCX1fPC/Sgq3cOSCfPjiCZOykM3yHUMKY1B4oJTkvgAYfjHRPo3wSgAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAABxwP3eCxni86h7i2Ia/ipU7LXAi8NSy5+6d4IfYXiIrQIYh1iAGmiMAAYAAAAARmxvcmlhbiBNYXRoaWFzIEJlcmtlbWVpZXIAEiDoE0591lm+mVINEzoRRjG2RgXOlvzcY7zjTfbZ2C1EcxggIAEqDlRlYW1TcGVkayAzIEFMErUBAQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lYTNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAADRvsPtOu+qWefImHS4cjlhW7e30ZUcKUZjc4I/6O+VTQAYm2jVPqrlaQAAACRUZWFtU3BlYWsgc3lzdGVtcyBHbWJIAADp+1m+Va5fm/+9iShB/n8ODsDStOisD5t5L5dnhA8FrgUYm2jVPqrlaRpAHiS22STiEHj6vWZYoj2OWVJ0InrAhze46BS0M8hcjf3pe5sLzUlW0pX2umOm6iXi+0w55qCm+Ld7E799m2cZDQ=="
data = base64.b64decode(key2_b64)
print("Length:", len(data))

offset = 0
while offset < len(data):
    tag_byte = data[offset]
    field_num = tag_byte >> 3
    wire_type = tag_byte & 0x07
    offset += 1
    
    if wire_type == 2:
        # read varint length
        length = 0
        shift = 0
        while True:
            b = data[offset]
            offset += 1
            length |= (b & 0x7f) << shift
            shift += 7
            if b & 0x80 == 0:
                break
        print(f"Field {field_num}: len={length}")
        val = data[offset:offset+length]
        print(f"  Hex: {val.hex()[:50]}...")
        if field_num == 2:
            print(f"  Field 2 full hex: {val.hex()}")
            print(f"  Field 2 length: {len(val)}")
        offset += length
    else:
        print(f"Other field {field_num} wire {wire_type}")
        break
