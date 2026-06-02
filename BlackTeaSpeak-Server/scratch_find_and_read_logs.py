import os

root_dir = "d:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server"

found_logs = []
for root, dirs, files in os.walk(root_dir):
    # Ignore build/target/git directories
    if "target" in root or ".git" in root or ".vscode" in root:
        continue
    for f in files:
        if f.endswith(".log") or f.endswith(".txt"):
            found_logs.append(os.path.join(root, f))

print(f"Found {len(found_logs)} potential log/text files:")
for path in found_logs:
    size = os.path.getsize(path)
    print(f"  {path} ({size} bytes)")

# Read the last 50 lines of the largest or most recent log files
for path in found_logs:
    filename = os.path.basename(path)
    if "build" in filename or "scratch" in filename or "cargo" in filename or "lock" in filename or "check_err" in filename:
        continue
    
    size = os.path.getsize(path)
    if size == 0:
        continue
        
    print(f"\n=========================================")
    print(f"=== Last 50 lines of: {path} ===")
    print(f"=========================================")
    try:
        with open(path, "rb") as f:
            content = f.read()
        
        if content.startswith(b'\xff\xfe') or b'\x00' in content[:200]:
            text = content.decode('utf-16le', errors='ignore')
        else:
            text = content.decode('utf-8', errors='ignore')
            
        lines = text.splitlines()
        for line in lines[-50:]:
            print(line)
    except Exception as e:
        print(f"Error reading {path}: {e}")
