import base64

root_pbl = "zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo="
decoded = base64.b64decode(root_pbl)
chain_root = bytes.fromhex("af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429")

print("Decoded root_pbl:", decoded.hex())
print("Chain root      :", chain_root.hex())

# Try endianness swap (reverse bytes)
rev_decoded = decoded[::-1]
print("Reversed root_pbl:", rev_decoded.hex())

# Try bit flip on last byte of reversed
flipped_rev = bytearray(rev_decoded)
flipped_rev[31] ^= 0x80
print("Flipped reversed :", flipped_rev.hex())
