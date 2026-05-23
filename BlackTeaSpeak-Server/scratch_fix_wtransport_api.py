import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Fix imports
content = re.sub(
    r"use wtransport::\{Endpoint, ServerConfig, Identity\};",
    r"use wtransport::{Endpoint, ServerConfig as WTransportServerConfig, Identity};",
    content
)

# Remove unused rustls imports that caused confusion if any, but we just alias WTransportServerConfig

# Fix with_bind_address
content = re.sub(
    r"let config = ServerConfig::builder\(\)\n\s*\.with_bind_address\(self\.bind_addr\)",
    r"let config = WTransportServerConfig::builder()\n            .with_bind_default(self.bind_addr.port())",
    content
)

# And fix Identity to not conflict if there was one, though there wasn't.

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
