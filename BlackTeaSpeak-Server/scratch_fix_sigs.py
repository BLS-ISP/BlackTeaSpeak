import re

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "r", encoding="utf-8") as f:
    content = f.read()

# Replace patterns like:
#     fn handle_conversation_fetch(
#         &self,
# 
#     fn handle_
# With just the next fn handle_
# Because the first one is unclosed.

while True:
    new_content = re.sub(r"^\s*fn [a-zA-Z0-9_]+\(\n\s*\&(?:mut )?self,\n\n(\s*fn )", r"\1", content, flags=re.MULTILINE)
    if new_content == content:
        break
    content = new_content

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "w", encoding="utf-8") as f:
    f.write(content)
