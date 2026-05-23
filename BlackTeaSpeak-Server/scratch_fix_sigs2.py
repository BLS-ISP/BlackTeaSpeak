import re

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "r", encoding="utf-8") as f:
    content = f.read()

# Replace patterns like:
#     fn handle_private_chat_signal(
#         &mut self,
# 
# With nothing.
while True:
    new_content = re.sub(r"^\s*fn [a-zA-Z0-9_]+\(\n\s*\&(?:mut )?self,\n\s*notify_command: Option\<\&str\>,\n", "", content, flags=re.MULTILINE)
    new_content = re.sub(r"^\s*fn [a-zA-Z0-9_]+\(\n\s*\&(?:mut )?self,\n\n", "", new_content, flags=re.MULTILINE)
    if new_content == content:
        break
    content = new_content

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "w", encoding="utf-8") as f:
    f.write(content)
