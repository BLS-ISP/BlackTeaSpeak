import base64
import hashlib

# Base64 string for V1 key
v1_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="

v1_bytes = base64.b64decode(v1_b64)
print("V1 key total length:", len(v1_bytes))

# V1 Header is 66 bytes: version (2) + cryptKey (64)
header = v1_bytes[:66]
version = header[0] | (header[1] << 8)
crypt_key = header[2:]

print("Version:", version)
print("Crypt key hex:", crypt_key.hex())

# Body starts at 66
body = bytearray(v1_bytes[66:])
print("Encrypted body length:", len(body))

# XOR body using crypt_key (64 bytes)
for i in range(len(body)):
    body[i] ^= crypt_key[i % len(crypt_key)]
    
print("\nDecrypted body hex:")
print(body.hex())

# Let's print as ASCII where possible
print("\nDecrypted body ASCII:")
print("".join(chr(b) if 32 <= b <= 126 else '.' for b in body))
