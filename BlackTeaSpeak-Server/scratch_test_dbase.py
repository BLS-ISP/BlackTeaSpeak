import base64
import base64

# Base key prv
d_base_b64 = "6BNOfdZZvplSDRM6EUYxtkYFzpb83GO840322dgtRHM="
d_base_bytes = base64.b64decode(d_base_b64)

# Calculate public key using standard ed25519 basepoint mult
# Using curve25519-dalek/ed25519 basepoint mult:
# Wait, curve25519 has a private-public keygen!
# Let's import ed25519 dalek or use cryptography / curve25519 python package.
# We can use cryptography package:
from cryptography.hazmat.primitives.asymmetric import ed25519

private_key = ed25519.Ed25519PrivateKey.from_private_bytes(d_base_bytes)
public_key = private_key.public_key()
pub_bytes = public_key.public_bytes_raw()

print("Base private key (b64):", d_base_b64)
print("Base private key (hex):", d_base_bytes.hex())
print("Base public key (hex): ", pub_bytes.hex())
