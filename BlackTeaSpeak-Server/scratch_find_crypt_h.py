import os

root_dir = "C:\\Users\\Gener\\.gemini\\antigravity\\brain\\15814df9-2921-4cc4-99a8-cbe39721be56\\scratch\\TeaSpeak-Server"

target_file = None
for root, dirs, files in os.walk(root_dir):
    for f in files:
        if f == "crypt.h" or f == "crypt.cpp":
            target_file = os.path.join(root, f)
            break

if target_file:
    print("Found crypt file at:", target_file)
    with open(target_file, "r") as f:
        print(f.read())
else:
    print("crypt file not found!")
