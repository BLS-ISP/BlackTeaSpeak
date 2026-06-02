import base64

key_b64 = "VFMzTGljZW5zZWcwZQIwKlKRLLFIF9Q5S/vDhqNGmBxB+VoP8Svv0qvtkNG9jxFmI5E/wZfOQS3qlg4YZmW+AjEAq2SsZnRbw4YpFqiUKjG9hHhj/Mar55swaVKg6fXWgaDhPfrPDEKNk8fFf2ozuxdHIBivwQzTA8AHuQ1OiRHsRh+KabwjV4KyEh3b6pEmnzByrRUYoaHm/VLSbEKb4h5HloYr8G2oNlMIAcC8SvTO2p7XPj4gX2i0obPx6oveo7egTod7NnUAhz2d3M+fBBrSVn9OFtXSxTK0yiJjld9ix2HgJbCDHFDmCuCsSCM2UPVi3qcHXzjYAUQGoEXXAsyBlFAC/Fr4ioBTB2lT8V4rWbraGH4nrLPOu7EZwzRWpFglkQXibpU+aKuDEjw="
key_bytes = base64.b64decode(key_b64)

pubkeys = {
    "Root": "af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429",
    "Server": "7c28498b980304b795439e55d9cee696f0af5ed3af45d056b59a6f03078302a8",
    "License Sign": "e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f9767840f05ae",
}

for name, pk_hex in pubkeys.items():
    pk_bytes = bytes.fromhex(pk_hex)
    idx = key_bytes.find(pk_bytes)
    print(f"Searching for {name} Pubkey ({pk_hex[:10]}...):")
    if idx != -1:
        print(f"  Found at index {idx}!")
    else:
        print("  Not found")
        # Try flipped sign bit
        flipped = bytearray(pk_bytes)
        flipped[31] ^= 0x80
        idx_flipped = key_bytes.find(flipped)
        if idx_flipped != -1:
            print(f"  Found at index {idx_flipped} (with flipped sign)!")
        else:
            print("  Flipped sign also not found")
