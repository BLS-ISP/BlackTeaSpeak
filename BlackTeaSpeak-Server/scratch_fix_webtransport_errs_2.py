import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Line 1207: rtc_manager, inside struct initialization
content = re.sub(r"^\s*rtc_manager,\n", "", content, flags=re.MULTILINE)

# Line 2092: self.rtc_manager.as_ref() inside BlackTeaWebSessionHandler
content = re.sub(r"^[ \t]*let Some\(rtc_manager\) = self\.rtc_manager\.as_ref\(\) else \{\n[ \t]*return Ok\(vec!\[error_frame\(\n[ \t]*&?return_code,\n[ \t]*ERROR_CURRENTLY_NOT_POSSIBLE,\n[ \t]*\"rtc unavailable\",\n[ \t]*None,\n[ \t]*\)\?\]\);\n[ \t]*\};\n", "", content, flags=re.MULTILINE)

# Line 2095: self.rtc_identity()
content = re.sub(r"^[ \t]*let Some\(identity\) = self\.rtc_identity\(\) else \{\n[ \t]*return Ok\(vec!\[error_frame\(\n[ \t]*&?return_code,\n[ \t]*ERROR_CURRENTLY_NOT_POSSIBLE,\n[ \t]*\"rtc unavailable\",\n[ \t]*None,\n[ \t]*\)\?\]\);\n[ \t]*\};\n", "", content, flags=re.MULTILINE)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done2")
