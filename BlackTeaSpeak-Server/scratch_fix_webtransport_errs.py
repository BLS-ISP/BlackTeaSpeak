import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Fix 1: rtc_manager in BlackTeaWebTransportServer
content = re.sub(r"\s*rtc_manager:\s*Arc<Mutex<Option<.+?>>>,\n", "\n", content)
content = re.sub(r"\s*rtc_manager:\s*None,\n", "\n", content)

# Fix 2: remove rtc_manager from handle_client call
# line 1242: let rtc_manager = Arc::clone(&self.rtc_manager);
content = re.sub(r"\s*let rtc_manager = Arc::clone\(&self\.rtc_manager\);\n", "\n", content)

# line 1252: rtc_manager,
content = re.sub(r"\s*rtc_manager,\n\s*sessions,", "\n                            sessions,", content)

# Fix 3: rtc_manager parameter in handle_client
# line 2185 (or wherever): rtc_manager: Arc<Mutex<Option<BlackTeaWebRtcManager>>>,
# We will just grep the string
content = re.sub(r"\s*rtc_manager:\s*Arc<Mutex<Option<BlackTeaWebRtcManager>>>,\n", "\n", content)

# Fix 4: remaining rtc_manager and identity uses in BlackTeaWebSessionHandler
# Just match the whole block for rtc_manager
pat1 = r"\s*let Some\(rtc_manager\) = self\.rtc_manager\.as_ref\(\) else \{\s*return Ok\(vec!\[error_frame\(\s*(?:&?)return_code,\s*ERROR_CURRENTLY_NOT_POSSIBLE,\s*\"rtc unavailable\",\s*None,\s*\)\?\]\);\s*\};\s*"
content = re.sub(pat1, "\n", content)

pat2 = r"\s*let Some\(identity\) = self\.rtc_identity\(\) else \{\s*return Ok\(vec!\[error_frame\(\s*(?:&?)return_code,\s*ERROR_CURRENTLY_NOT_POSSIBLE,\s*\"rtc unavailable\",\s*None,\s*\)\?\]\);\s*\};\s*"
content = re.sub(pat2, "\n", content)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
