import base64

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

def try_parse_proto(data, start):
    pos = start
    fields = []
    while pos < len(data):
        tag_byte = data[pos]
        field_num = tag_byte >> 3
        wire_type = tag_byte & 0x07
        
        if field_num == 0 or wire_type > 5:
            # Invalid protobuf tag/wire type
            return None
            
        pos += 1
        if wire_type == 0: # Varint
            val = 0
            shift = 0
            while True:
                if pos >= len(data):
                    return None
                b = data[pos]
                pos += 1
                val |= (b & 0x7f) << shift
                shift += 7
                if b & 0x80 == 0:
                    break
            fields.append((field_num, "varint", val))
        elif wire_type == 2: # Length-delimited
            length = 0
            shift = 0
            while True:
                if pos >= len(data):
                    return None
                b = data[pos]
                pos += 1
                length |= (b & 0x7f) << shift
                shift += 7
                if b & 0x80 == 0:
                    break
            if pos + length > len(data):
                return None
            field_data = data[pos:pos+length]
            fields.append((field_num, "bytes", field_data))
            pos += length
        elif wire_type == 1: # 64-bit
            if pos + 8 > len(data):
                return None
            val = int.from_bytes(data[pos:pos+8], 'little')
            fields.append((field_num, "64bit", val))
            pos += 8
        elif wire_type == 5: # 32-bit
            if pos + 4 > len(data):
                return None
            val = int.from_bytes(data[pos:pos+4], 'little')
            fields.append((field_num, "32bit", val))
            pos += 4
        else:
            return None
            
    return fields

for offset in range(10, 150):
    res = try_parse_proto(key_bytes, offset)
    if res is not None and len(res) >= 2:
        print(f"\nSUCCESS at offset {offset}: found {len(res)} fields:")
        for fnum, ftype, val in res:
            if ftype == "bytes":
                print(f"  Field {fnum}: {ftype} (len={len(val)}): hex={val[:20].hex()}...")
            else:
                print(f"  Field {fnum}: {ftype} = {val}")
