import os

log_files = ["error.txt", "error.log", "build_check_errs.txt", "build_error.log", "initivexpand2_logs.txt"]

for log in log_files:
    path = os.path.join("d:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server", log)
    if os.path.exists(path):
        print(f"=== Last 50 lines of {log} ===")
        try:
            with open(path, "rb") as f:
                content = f.read()
            # Attempt to decode as UTF-16LE if it starts with BOM or looks like it
            if content.startswith(b'\xff\xfe') or b'\x00' in content[:100]:
                text = content.decode('utf-16le', errors='ignore')
            else:
                text = content.decode('utf-8', errors='ignore')
            
            lines = text.splitlines()
            for line in lines[-50:]:
                print(line)
        except Exception as e:
            print(f"Error reading {log}: {e}")
        print("\n")
