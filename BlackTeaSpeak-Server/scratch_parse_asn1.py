import base64

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

def parse_asn1(data, indent=0):
    pos = 0
    while pos < len(data):
        if pos + 2 > len(data):
            break
        tag = data[pos]
        length = data[pos+1]
        
        # Handle long form length
        header_len = 2
        if length & 0x80:
            num_octets = length & 0x7f
            if pos + 2 + num_octets > len(data):
                break
            length = 0
            for i in range(num_octets):
                length = (length << 8) | data[pos + 2 + i]
            header_len = 2 + num_octets
            
        tag_name = {
            0x02: "INTEGER",
            0x03: "BIT STRING",
            0x04: "OCTET STRING",
            0x05: "NULL",
            0x06: "OBJECT IDENTIFIER",
            0x30: "SEQUENCE",
        }.get(tag, f"TAG_{hex(tag)}")
        
        content = data[pos+header_len : pos+header_len+length]
        print("  " * indent + f"{tag_name} (len={length}):")
        
        if tag == 0x30: # SEQUENCE (constructed)
            parse_asn1(content, indent + 1)
        else:
            # Print hex/ascii content
            hex_str = content.hex()
            if len(hex_str) > 64:
                hex_str = hex_str[:64] + "..."
            print("  " * (indent + 1) + f"Value: {hex_str}")
            
        pos += header_len + length

print("Dissecting ASN.1 from index 11:")
parse_asn1(key_bytes[11:])
