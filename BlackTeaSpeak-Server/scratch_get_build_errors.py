import subprocess

result = subprocess.run(
    ["cargo", "check"],
    cwd="d:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server",
    capture_output=True,
    text=True
)

print("=== stdout ===")
print(result.stdout)
print("=== stderr ===")
# Print lines containing "error:" or "error[" and the surrounding error context
lines = result.stderr.splitlines()
for idx, line in enumerate(lines):
    if "error" in line.lower() or "aborting" in line.lower():
        start = max(0, idx - 3)
        end = min(len(lines), idx + 10)
        print(f"\n--- Context around line {idx+1} ---")
        for j in range(start, end):
            print(f"{j+1:04d}: {lines[j]}")
