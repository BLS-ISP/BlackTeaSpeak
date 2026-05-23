import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

content = content.replace("data.push('\n');", "data.push('\\n');")
content = content.replace("send.write_all(b\"\n\").await?;", "send.write_all(b\"\\n\").await?;")

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
