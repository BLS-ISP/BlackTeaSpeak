import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Fix BufReader
content = re.sub(
    r"let mut recv_reader = BufReader::new\(recv\);",
    r"let mut recv_reader = tokio::io::BufReader::new(recv);",
    content
)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
