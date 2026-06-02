import base64

with open("protocol_key.txt", "r") as f:
    for line in f:
        if line.startswith("chain:"):
            chain_b64 = line.split("chain:")[1].strip()
            break

chain_bytes = base64.b64decode(chain_b64)
print(f"Total chain length: {len(chain_bytes)}")

# Chain 1 length: 270 bytes
# Chain 2 length: 181 bytes
# 270 + 181 = 451
print("Bytes 451 to end:")
trailing = chain_bytes[451:]
print("  Hex   :", trailing.hex())
print("  Length:", len(trailing))
print("  Base64:", base64.b64encode(trailing).decode())
