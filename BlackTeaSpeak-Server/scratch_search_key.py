import os

search_dir = r"C:\Users\Gener\.gemini\antigravity\brain\15814df9-2921-4cc4-99a8-cbe39721be56\scratch\TeaSpeak-Server"
queries = ["licensekey", "licensekey.dat", "LicenseManager", "license::read", "readLocalLicence"]

for root, dirs, files in os.walk(search_dir):
    for file in files:
        if file.endswith((".cpp", ".h", ".txt", ".cmake")):
            path = os.path.join(root, file)
            try:
                with open(path, "r", errors="ignore") as f:
                    content = f.read()
                for q in queries:
                    if q in content:
                        print(f"Found '{q}' in: {os.path.relpath(path, search_dir)}")
                        lines = content.splitlines()
                        for i, line in enumerate(lines):
                            if q in line:
                                print(f"  Line {i+1}: {line.strip()}")
            except Exception as e:
                pass
