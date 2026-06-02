import os

root_dir = "C:\\Users\\Gener\\.gemini\\antigravity\\brain\\15814df9-2921-4cc4-99a8-cbe39721be56\\scratch\\TeaSpeak-Server"

for root, dirs, files in os.walk(root_dir):
    for f in files:
        if f.endswith(".h") or f.endswith(".cpp"):
            path = os.path.join(root, f)
            try:
                with open(path, "r", encoding="utf-8", errors="ignore") as file:
                    content = file.read()
                if "MAGIC_NUMER" in content:
                    print("Found MAGIC_NUMER in:", path)
                    # print lines around it
                    lines = content.splitlines()
                    for idx, line in enumerate(lines):
                        if "MAGIC_NUMER" in line or "struct License " in line or "struct LicenseData" in line:
                            print(f"  {idx+1:04d}: {line.strip()}")
            except Exception as e:
                pass
