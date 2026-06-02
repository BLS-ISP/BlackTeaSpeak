import base64
import struct

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.decodebytes(key_b64.encode())

print("Key bytes length:", len(key_bytes))

# Print all printable strings
strings = []
curr = bytearray()
for b in key_bytes:
    if 32 <= b <= 126:
        curr.append(b)
    else:
        if len(curr) >= 4:
            strings.append(curr.decode('ascii'))
        curr = bytearray()
if len(curr) >= 4:
    strings.append(curr.decode('ascii'))

print("\nPrintable strings in key_bytes:")
print(strings)

# Print hex block-by-block
print("\nHex representation in 16-byte lines:")
for i in range(0, len(key_bytes), 16):
    line = key_bytes[i:i+16]
    hex_str = line.hex()
    ascii_str = "".join(chr(b) if 32 <= b <= 126 else "." for b in line)
    print(f"  {i:03d}:  {hex_str:<32}  |{ascii_str}|")
