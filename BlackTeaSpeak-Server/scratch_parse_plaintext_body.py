import base64
import struct

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

print(f"Total key bytes: {len(key_bytes)}")

# Let's search for any offset where the decrypted-like body might start
# We will test offsets from 10 to 150
for offset in range(10, 150):
    if offset + 4 > len(key_bytes):
        break
    
    # Try LE
    len_prv_le = struct.unpack("<H", key_bytes[offset:offset+2])[0]
    len_h_le = struct.unpack("<H", key_bytes[offset+2:offset+4])[0]
    
    # Try BE
    len_prv_be = struct.unpack(">H", key_bytes[offset:offset+2])[0]
    len_h_be = struct.unpack(">H", key_bytes[offset+2:offset+4])[0]
    
    # Standard Header length is 88 bytes. Check if the body lengths fit
    if len_prv_le > 0 and len_h_le > 0 and offset + 88 + len_prv_le + len_h_le <= len(key_bytes):
        print(f"MATCH LE at offset {offset}: len_prv={len_prv_le}, len_h={len_h_le}")
        # Parse the rest
        body = key_bytes[offset:]
        print(f"  Private data hex: {body[88:88+len_prv_le].hex()}")
        print(f"  Hierarchy data hex: {body[88+len_prv_le:88+len_prv_le+len_h_le].hex()}")
        
    if len_prv_be > 0 and len_h_be > 0 and offset + 88 + len_prv_be + len_h_be <= len(key_bytes):
        print(f"MATCH BE at offset {offset}: len_prv={len_prv_be}, len_h={len_h_be}")
        body = key_bytes[offset:]
        print(f"  Private data hex: {body[88:88+len_prv_be].hex()}")
        print(f"  Hierarchy data hex: {body[88+len_prv_be:88+len_prv_be+len_h_be].hex()}")
