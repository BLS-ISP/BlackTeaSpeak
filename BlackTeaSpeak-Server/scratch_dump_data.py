import base64

with open("protocol_key.txt", "r") as f:
    content = f.read()
chain_b64 = ""
for line in content.splitlines():
    if line.startswith("chain:"):
        chain_b64 = line.split("chain:")[1].strip()

data = base64.b64decode(chain_b64)
print("Data length:", len(data))
print("Bytes 0 to 40 in hex:", data[0:40].hex())
print("Tag at 212:", data[212])
print("Next few bytes:", data[213:213+32].hex())
