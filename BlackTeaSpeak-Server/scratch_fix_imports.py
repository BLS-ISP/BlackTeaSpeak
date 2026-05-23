import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Fix imports inside the block
content = re.sub(
    r"use tokio::io::\{AsyncBufReadExt, AsyncWriteExt, BufReader\};",
    r"use tokio::io::{AsyncBufReadExt, AsyncWriteExt};",
    content
)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
