import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

while True:
    new_content = re.sub(r"^[ \t]*fn [a-zA-Z0-9_]+\([^\)]+?^[ \t]*(fn )", r"    \1", content, flags=re.MULTILINE)
    if new_content == content:
        break
    content = new_content

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
