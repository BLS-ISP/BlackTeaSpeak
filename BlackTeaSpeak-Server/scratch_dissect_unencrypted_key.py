import base64
import struct

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

print("Total length:", len(key_bytes))

# Slice starting at 114 (after the 103 bytes signature SEQUENCE)
body = key_bytes[114:]
print(f"Body length at 114: {len(body)}")
print("Body hex:", body.hex())

# Let's print the body with labels/offsets
print("\nDissecting body:")
offset = 0
while offset < len(body):
    remaining = len(body) - offset
    print(f"  [{offset:03d}]: hex={body[offset:offset+16].hex():<32} | ascii='{''.join(chr(b) if 32 <= b <= 126 else '.' for b in body[offset:offset+16])}'")
    offset += 16
