import base64

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

for start in range(len(key_bytes) - 5):
    # Check if key_bytes[start] is 0x30 (SEQUENCE) or 0x02 (INTEGER)
    tag = key_bytes[start]
    length = key_bytes[start+1]
    if tag in [0x30, 0x02] and length > 0 and start + 2 + length <= len(key_bytes):
        print(f"Possible ASN.1 tag at {start}: tag={hex(tag)}, length={length}")
        # Print a snippet
        print("  Snippet:", key_bytes[start:start+10].hex())
