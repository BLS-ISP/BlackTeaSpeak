import base64
import struct
import sys

sys.stdout.reconfigure(encoding='utf-8')

with open("protocol_key.txt", "r") as f:
    content = f.read()
chain_b64 = ""
for line in content.splitlines():
    if line.startswith("chain:"):
        chain_b64 = line.split("chain:")[1].strip()

data = base64.b64decode(chain_b64)
print("Length:", len(data))
pos = 0
chain_type = data[pos]
print("Chain type:", chain_type)
pos += 1
while pos < len(data):
    entry_start = pos
    entry_prefix = data[pos]
    pubkey = data[pos+1:pos+33]
    license_type = data[pos+33]
    begin = struct.unpack(">I", data[pos+34:pos+38])[0]
    end = struct.unpack(">I", data[pos+38:pos+42])[0]
    pos += 42
    print(f"Entry at {entry_start}: Type={license_type}, Pubkey={pubkey.hex()}")
    print(f"  Begin={begin} ({begin+1356998400}), End={end} ({end+1356998400})")
    if license_type == 0:
        dummy = data[pos:pos+4]
        pos += 4
        issuer_bytes = bytearray()
        while pos < len(data):
            b = data[pos]
            pos += 1
            if b == 0: break
            issuer_bytes.append(b)
        print("  Issuer:", issuer_bytes.decode('utf-8', errors='ignore'))
    elif license_type == 5:
        print("  License Sign entry")
