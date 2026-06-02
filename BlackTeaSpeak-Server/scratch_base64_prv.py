import base64

key_hex = "d141d60887ea45e089275cabb83181696dd3ca6339a5ed2ca228dbd41cb61157"
key_bytes = bytes.fromhex(key_hex)
key_b64 = base64.b64encode(key_bytes).decode()
print("Private Key (base64):", key_b64)
