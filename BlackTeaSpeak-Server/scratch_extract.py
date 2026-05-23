import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# I want to find handle_client and extract it
m = re.search(r"fn handle_client\(.*?\) -> Result<\(\)> \{.*?(?=\nfn blackteaweb_trace_enabled)", content, flags=re.DOTALL)
if m:
    handle_client_src = m.group(0)
    print("Found handle_client!")
    # Now write it to a temp file so I can inspect and modify it
    with open("scratch_handle_client.txt", "w", encoding="utf-8") as out:
        out.write(handle_client_src)
else:
    print("Could not find handle_client")
