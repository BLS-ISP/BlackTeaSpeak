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
        if "std::shared_ptr<LicensePrivate> LicensePrivate::read" in line or "LicensePrivate::read(" in line and "const" not in line and ";" not in line and "=" not in line:
            print(f"Match on line {idx+1}: {line.strip()}")
            # Print 60 lines
            for j in range(idx, min(idx + 60, len(lines))):
                print(f"{j+1:04d}: {lines[j]}", end="")
            break
else:
    print("license.cpp not found!")
