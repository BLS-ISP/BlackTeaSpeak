import os

root_dir = "C:\\Users\\Gener\\.gemini\\antigravity\\brain\\15814df9-2921-4cc4-99a8-cbe39721be56\\scratch\\TeaSpeak-Server"

target_file = None
for root, dirs, files in os.walk(root_dir):
    for f in files:
        if f == "license.h":
            target_file = os.path.join(root, f)
            break

if target_file:
    print("Found license.h at:", target_file)
    with open(target_file, "r") as f:
        lines = f.readlines()
        
    start_line = 200
    end_line = 300
    for j in range(start_line - 1, min(end_line, len(lines))):
        print(f"{j+1:04d}: {lines[j]}", end="")
else:
    print("license.h not found!")
