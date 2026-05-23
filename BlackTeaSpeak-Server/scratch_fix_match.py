import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# We want to replace everything inside:
#         match command.as_str() {
#             ...
#         }
# However, this match block is HUGE (from ~line 1550 to 1890).
# Let's find "match command.as_str() {" and the matching closing brace.

start_idx = content.find("        match command.as_str() {\n")
if start_idx != -1:
    # Find the closing brace for this match block.
    # We can just count braces.
    brace_count = 0
    end_idx = -1
    for i in range(start_idx, len(content)):
        if content[i] == '{':
            brace_count += 1
        elif content[i] == '}':
            brace_count -= 1
            if brace_count == 0:
                end_idx = i
                break
    
    if end_idx != -1:
        new_match_body = """        match command.as_str() {
            _ => Ok(vec![ok_frame(&return_code)?]),
        }"""
        new_content = content[:start_idx] + new_match_body + content[end_idx+1:]
        
        with open(filepath, "w", encoding="utf-8") as f:
            f.write(new_content)
        print("Successfully replaced match block.")
    else:
        print("Could not find matching brace.")
else:
    print("Could not find match command.as_str() {")

