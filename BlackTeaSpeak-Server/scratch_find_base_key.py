import os

root_dir = "C:\\Users\\Gener\\.gemini\\antigravity\\brain\\15814df9-2921-4cc4-99a8-cbe39721be56\\scratch\\TeaSpeak-Server"

target_file = None
for root, dirs, files in os.walk(root_dir):
    for f in files:
        if f == "license.cpp":
            target_file = os.path.join(root, f)
            break

if target_file:
    with open(target_file, "r") as f:
        lines = f.readlines()
        
    for idx, line in enumerate(lines):
        if "base" in line.lower() or "root" in line.lower() or "key" in line.lower():
            if "static" in line or "const" in line or "byte" in line or "unsigned" in line:
                print(f"Line {idx+1}: {line.strip()}")
else:
    print("license.cpp not found!")
