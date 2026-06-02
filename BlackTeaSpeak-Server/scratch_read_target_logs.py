import os

target_files = ["error.txt", "error.log", "check.log", "pcap_output.txt"]

for filename in target_files:
    path = os.path.join("d:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server", filename)
    if os.path.exists(path):
        size = os.path.getsize(path)
        print(f"=== File: {filename} ({size} bytes) ===")
        try:
            with open(path, "rb") as f:
                content = f.read()
            if content.startswith(b'\xff\xfe') or b'\x00' in content[:200]:
                text = content.decode('utf-16le', errors='ignore')
            else:
                text = content.decode('utf-8', errors='ignore')
            
            lines = text.splitlines()
            print(f"Total lines: {len(lines)}")
            # Print the last 40 lines
            for line in lines[-40:]:
                print(line)
        except Exception as e:
            print(f"Error reading {filename}: {e}")
        print("\n")
    else:
        print(f"File {filename} does not exist at {path}\n")
