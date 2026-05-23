import re

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "r", encoding="utf-8") as f:
    content = f.read()

# Remove all fn handle_rtc_ functions
content = re.sub(r"\s*fn handle_rtc_[a-zA-Z0-9_]+\(\&self[^\}]*?\}\n", "\n", content, flags=re.DOTALL)
# some have longer bodies so the regex might stop early if there are nested braces.
# We will just write a custom parser for {} balancing.

def remove_rtc_funcs(code):
    lines = code.split("\n")
    out_lines = []
    skip = False
    brace_depth = 0
    for line in lines:
        if re.search(r"fn handle_rtc_", line):
            skip = True
            brace_depth = 0
            if "{" in line:
                brace_depth += line.count("{") - line.count("}")
            if brace_depth <= 0 and line.strip().endswith("}"):
                skip = False
            continue
            
        if skip:
            brace_depth += line.count("{") - line.count("}")
            if brace_depth <= 0:
                skip = False
            continue
            
        # Also remove matches in the command routing
        if '"rtcsessiondescribe" =>' in line:
            skip = True
            brace_depth = line.count("{") - line.count("}")
            continue
        if '"rtcicecandidate" =>' in line or '"rtcsessionreset" =>' in line or '"broadcastvideo" =>' in line or '"broadcastaudio" =>' in line or '"broadcastvideoconfigure" =>' in line or '"broadcastvideojoin" =>' in line or '"broadcastvideoleave" =>' in line or '"whispersessioninitialize" =>' in line or '"whispersessionreset" =>' in line:
            skip = True
            brace_depth = line.count("{") - line.count("}")
            continue

        out_lines.append(line)
        
    return "\n".join(out_lines)

content = remove_rtc_funcs(content)

with open(r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs", "w", encoding="utf-8") as f:
    f.write(content)
