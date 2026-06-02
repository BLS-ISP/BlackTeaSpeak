import os

root_dir = "C:\\Users\\Gener\\.gemini\\antigravity\\brain\\15814df9-2921-4cc4-99a8-cbe39721be56\\scratch\\TeaSpeak-Server"

for root, dirs, files in os.walk(root_dir):
    for f in files:
        if f.endswith(".proto"):
            path = os.path.join(root, f)
            print("Found .proto file at:", path)
            with open(path, "r") as file:
                print(file.read())
