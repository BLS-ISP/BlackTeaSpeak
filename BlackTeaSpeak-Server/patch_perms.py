import os

file_path = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\runtime\permissions.rs"

with open(file_path, "r", encoding="utf-8") as f:
    content = f.read()

replacements = [
    (
        "pub(super) fn handle_servergroupaddperm",
        "        for parsed_assignment in parsed_assignments {\n            group\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n\n        QueryResponse::ok()",
        "        for parsed_assignment in parsed_assignments {\n            group\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n        let _ = self.db.save_server_group(0, group);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_servergroupautoaddperm",
        "        for parsed_assignment in parsed_assignments {\n            group\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n\n        QueryResponse::ok()",
        "        for parsed_assignment in parsed_assignments {\n            group\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n        let _ = self.db.save_server_group(0, group);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_servergroupautodelperm",
        "        for permission_name in permission_names {\n            group.permissions.remove(&permission_name);\n        }\n\n        QueryResponse::ok()",
        "        for permission_name in permission_names {\n            group.permissions.remove(&permission_name);\n        }\n        let _ = self.db.save_server_group(0, group);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_servergroupdelperm",
        "        for permission_name in permission_names {\n            group.permissions.remove(&permission_name);\n        }\n\n        QueryResponse::ok()",
        "        for permission_name in permission_names {\n            group.permissions.remove(&permission_name);\n        }\n        let _ = self.db.save_server_group(0, group);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_channelgroupaddperm",
        "        for parsed_assignment in parsed_assignments {\n            group\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n\n        QueryResponse::ok()",
        "        for parsed_assignment in parsed_assignments {\n            group\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n        let _ = self.db.save_channel_group(0, group);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_channelgroupdelperm",
        "        for permission_name in permission_names {\n            group.permissions.remove(&permission_name);\n        }\n\n        QueryResponse::ok()",
        "        for permission_name in permission_names {\n            group.permissions.remove(&permission_name);\n        }\n        let _ = self.db.save_channel_group(0, group);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_channeladdperm",
        "        for parsed_assignment in parsed_assignments {\n            channel\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n\n        QueryResponse::ok()",
        "        for parsed_assignment in parsed_assignments {\n            channel\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n        let _ = self.db.save_channel(actor.server_id, channel);\n\n        QueryResponse::ok()"
    ),
    (
        "pub(super) fn handle_channeldelperm",
        "        for permission_name in permission_names {\n            channel.permissions.remove(&permission_name);\n        }\n\n        QueryResponse::ok()",
        "        for permission_name in permission_names {\n            channel.permissions.remove(&permission_name);\n        }\n        let _ = self.db.save_channel(actor.server_id, channel);\n\n        QueryResponse::ok()"
    )
]

for func_name, old_text, new_text in replacements:
    start_idx = content.find(func_name)
    if start_idx == -1:
        print(f"Error: Could not find {func_name}")
        continue
    
    end_idx = content.find("QueryResponse::ok()", start_idx)
    if end_idx == -1:
        print(f"Error: Could not find QueryResponse::ok() for {func_name}")
        continue
    
    chunk = content[start_idx:end_idx + 20]
    new_chunk = chunk.replace(old_text, new_text)
    content = content[:start_idx] + new_chunk + content[end_idx + 20:]
    print(f"Patched {func_name}")

with open(file_path, "w", encoding="utf-8") as f:
    f.write(content)
