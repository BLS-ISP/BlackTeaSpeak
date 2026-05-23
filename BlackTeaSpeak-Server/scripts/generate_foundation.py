import os
import csv
import json
import re

# Paths
BASE_DIR = r"D:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server"
TS_DECLARATIONS = os.path.join(BASE_DIR, "data", "tsdeclarations")
TEASPEAK_RESOURCES = r"D:\projekt\BlackTeaSpeak\temp-will get deleted\TeaSpeak-1.5.6-server\resources"
TEASPEAK_COMMANDDOCS = r"D:\projekt\BlackTeaSpeak\temp-will get deleted\TeaSpeak-1.5.6-server\commanddocs"
OUT_DIR = os.path.join(BASE_DIR, "data", "foundation")

os.makedirs(OUT_DIR, exist_ok=True)

print("Generating permission-catalog.json...")
catalog = []
name_to_id = {}
with open(os.path.join(TS_DECLARATIONS, "Permissions.csv"), "r", encoding="utf-8") as f:
    reader = csv.reader(f)
    next(reader) # skip header
    for idx, row in enumerate(reader):
        name = row[0].strip()
        doc = row[1].strip() if len(row) > 1 else ""
        entry = {
            "name": name,
            "id": idx,
            "description": doc,
            "idSource": "tsdeclarations/Permissions.csv",
            "descriptionSource": "tsdeclarations/Permissions.csv"
        }
        catalog.append(entry)
        name_to_id[name] = idx

with open(os.path.join(OUT_DIR, "permission-catalog.json"), "w", encoding="utf-8") as f:
    json.dump(catalog, f, indent=2)


print("Generating permission-groups.json...")
groups = []
current_group = None

with open(os.path.join(TEASPEAK_RESOURCES, "permissions.template"), "r", encoding="utf-8") as f:
    lines = f.readlines()

for line in lines:
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    if line == "--start":
        current_group = {"permissions": []}
    elif line == "--end":
        if current_group:
            groups.append(current_group)
            current_group = None
    elif ":" in line and current_group is not None:
        key, val = line.split(":", 1)
        if key == "name":
            current_group["name"] = val
        elif key == "target":
            tgt = int(val)
            current_group["target"] = tgt
            if tgt == 0:
                current_group["targetName"] = "QUERY"
            elif tgt == 1:
                current_group["targetName"] = "SERVER"
            elif tgt == 2:
                current_group["targetName"] = "CHANNEL"
            else:
                current_group["targetName"] = str(tgt)
        elif key == "property":
            current_group["property"] = val
        elif key == "permission":
            parts = val.split("=")
            pname = parts[0]
            vals = parts[1].split(",")
            # value, granted, skipped, negated
            current_group["permissions"].append({
                "name": pname,
                "value": int(vals[0]),
                "grantedBy": int(vals[1]),
                "skipped": int(vals[2]),
                "negated": int(vals[3])
            })

with open(os.path.join(OUT_DIR, "permission-groups.json"), "w", encoding="utf-8") as f:
    json.dump(groups, f, indent=2)


print("Generating permission-mapping.json...")
mappings = []
cur_mapping = None

with open(os.path.join(TEASPEAK_RESOURCES, "permission_mapping.txt"), "r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("group:"):
            gid = int(line.split(":")[1])
            cur_mapping = {"groupId": gid, "groupName": f"Group {gid}", "mappings": []}
            mappings.append(cur_mapping)
        elif line.startswith("mapping:"):
            parts = line.split(":")
            if len(parts) >= 3 and cur_mapping is not None:
                orig = parts[1]
                mapped = parts[2]
                cur_mapping["mappings"].append({
                    "originalName": orig,
                    "mappedValue": mapped
                })

with open(os.path.join(OUT_DIR, "permission-mapping.json"), "w", encoding="utf-8") as f:
    json.dump(mappings, f, indent=2)


print("Generating commands-manifest.json...")
commands = []
for file in os.listdir(TEASPEAK_COMMANDDOCS):
    if not file.endswith(".txt"): continue
    path = os.path.join(TEASPEAK_COMMANDDOCS, file)
    cmd_name = file[:-4]
    
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    
    usage_match = re.search(r"Usage:(.*?)(?:\n\n|\n[A-Z][a-z]+:)", content, re.DOTALL)
    perms_match = re.search(r"Permissions:(.*?)(?:\n\n|\n[A-Z][a-z]+:)", content, re.DOTALL)
    desc_match = re.search(r"Description:(.*?)(?:\n\n|\n[A-Z][a-z]+:)", content, re.DOTALL)
    example_match = re.search(r"Example:(.*)", content, re.DOTALL)
    
    usage = [line.strip() for line in usage_match.group(1).strip().split('\n')] if usage_match else []
    perms = [line.strip() for line in perms_match.group(1).strip().split('\n') if line.strip()] if perms_match else []
    desc = desc_match.group(1).strip().replace('\n', ' ') if desc_match else ""
    examples = [line.strip() for line in example_match.group(1).strip().split('\n')] if example_match else []
    
    commands.append({
        "name": cmd_name,
        "category": "Query",
        "docPath": f"commanddocs/{file}",
        "binaryOffset": None,
        "usage": usage,
        "permissions": perms,
        "description": desc,
        "result": None,
        "notes": None,
        "examples": examples
    })

with open(os.path.join(OUT_DIR, "commands-manifest.json"), "w", encoding="utf-8") as f:
    json.dump(commands, f, indent=2)

print("Generating query-baseline.json and binary-manifest.json and subsystems.json...")

baseline = {
    "profile": "TeaSpeak 1.5.6 Mock",
    "goal": "Reconstruct metadata",
    "essentialCommands": commands,
    "essentialPermissions": ["b_serverinstance_help_view", "b_virtualserver_select", "b_serverquery_login"],
    "essentialSourcePaths": [],
    "renameSeeds": []
}
with open(os.path.join(OUT_DIR, "query-baseline.json"), "w", encoding="utf-8") as f:
    json.dump(baseline, f, indent=2)

binary = {
    "runtimeDependencies": [],
    "binary": {
        "path": "unknown",
        "sha256": "unknown",
        "sizeBytes": 0
    },
    "commandOffsets": {}
}
with open(os.path.join(OUT_DIR, "binary-manifest.json"), "w", encoding="utf-8") as f:
    json.dump(binary, f, indent=2)

with open(os.path.join(OUT_DIR, "subsystems.json"), "w", encoding="utf-8") as f:
    json.dump([], f, indent=2)

print("Done generating foundation JSONs.")
