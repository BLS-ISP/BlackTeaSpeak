import base64
from cryptography.hazmat.primitives.asymmetric import ed25519

prv_b64 = "YARwqypuXjU9b+zg/yBEGpdTNiYgcWYV87k6vXU7rGo="
prv_bytes = base64.b64decode(prv_b64)

# Derive public key
private_key = ed25519.Ed25519PrivateKey.from_private_bytes(prv_bytes)
public_key = private_key.public_key()
pub_bytes = public_key.public_bytes_raw()

print("Derived public key from root_key_prv:")
print("Hex:", pub_bytes.hex())
print("Base64:", base64.b64encode(pub_bytes).decode())
