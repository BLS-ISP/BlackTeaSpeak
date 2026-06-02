import base64

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

# Print as protobuf fields
def parse_proto(data, indent=0):
    pos = 0
    while pos < len(data):
        if pos >= len(data):
            break
        tag_byte = data[pos]
        field_num = tag_byte >> 3
        wire_type = tag_byte & 0x07
        pos += 1
        
        # Read varint length for length-delimited fields
        if wire_type == 0: # Varint
            val = 0
            shift = 0
            while True:
                b = data[pos]
                pos += 1
                val |= (b & 0x7f) << shift
                shift += 7
                if b & 0x80 == 0:
                    break
            print("  " * indent + f"Field {field_num}: varint = {val}")
        elif wire_type == 2: # Length-delimited
            length = 0
            shift = 0
            while True:
                b = data[pos]
                pos += 1
                length |= (b & 0x7f) << shift
                shift += 7
                if b & 0x80 == 0:
                    break
            field_data = data[pos:pos+length]
            print("  " * indent + f"Field {field_num}: length={length}, bytes={field_data[:20].hex()}...")
            # Try to recursively parse
            try:
                parse_proto(field_data, indent + 1)
            except Exception:
                pass
            pos += length
        elif wire_type == 1: # 64-bit
            if pos + 8 <= len(data):
                val = int.from_bytes(data[pos:pos+8], 'little')
                pos += 8
                print("  " * indent + f"Field {field_num}: 64-bit = {val}")
        elif wire_type == 5: # 32-bit
            if pos + 4 <= len(data):
                val = int.from_bytes(data[pos:pos+4], 'little')
                pos += 4
                print("  " * indent + f"Field {field_num}: 32-bit = {val}")
        else:
            print("  " * indent + f"Field {field_num}: unknown wire type {wire_type}")
            break

print("Parsing key_bytes as protobuf:")
parse_proto(key_bytes)
