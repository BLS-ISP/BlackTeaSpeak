import struct

# The hex string printed by the rust program for Variant A:
hex_str = "00af6c715340363fb5eacf7a296ba7f10253dc421f9537c42f769584cd698ff429000916e7806b36ec80080000000255465616d537065616b2053797374656d7320476d62480000d1bec3ed3aefaa59e7c89874b87239615bb7b7d1951c29466373823fe8ef954d189b68d53eaae5696900000000245465616d537065616b2073797374656d7320476d62480005e9fb59be55ae5f9bffbd892841fe7f0e0ec0d2b4e8ac0f9b792f97678"
# Note: the string was truncated in the output, but we can check the header and dummy parts!

data = bytes.fromhex(hex_str)
print("Length of parsed data:", len(data))

print("Header:")
print("  Type:   ", data[0])
print("  Pubkey: ", data[1:33].hex())
print("  Begin:  ", data[33:37].hex())
print("  End:    ", data[37:41].hex())
print("  BodyLen:", data[41:43].hex(), "->", struct.unpack("<H", data[41:43])[0])
print("  Body start:")
print("    Dummy: ", data[43:47].hex())
print("    Issuer:", data[47:71].hex(), "->", data[47:71])
