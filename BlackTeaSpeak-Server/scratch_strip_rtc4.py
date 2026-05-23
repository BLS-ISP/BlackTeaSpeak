import os

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    lines = f.readlines()

out = []
i = 0
while i < len(lines):
    # Check if the current line has `rows: &[CommandRow],` and the previous line does NOT have `fn `
    if "rows: &[CommandRow]," in lines[i] and i > 0 and "fn " not in lines[i-1]:
        print(f"Found orphaned signature at line {i}")
        # Skip until we find the opening brace of the function block
        while "{" not in lines[i]:
            i += 1
        
        # Now track braces to find the end of this orphaned block
        brace_count = 0
        started = False
        while i < len(lines):
            brace_count += lines[i].count("{") - lines[i].count("}")
            if "{" in lines[i]:
                started = True
            
            i += 1
            if started and brace_count == 0:
                break
        continue
    else:
        out.append(lines[i])
        i += 1

with open(filepath, "w", encoding="utf-8") as f:
    f.writelines(out)
