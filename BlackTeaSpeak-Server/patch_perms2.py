import os

file_path = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\runtime\permissions.rs"

with open(file_path, "r", encoding="utf-8") as f:
    content = f.read()

replacements = [
    (
        "pub(super) fn handle_clientaddperm",
        "        } else {\n            let target =\n                self.ensure_client_permission_target_mut(actor.server_id, client_database_id);\n            for parsed_assignment in &parsed_assignments {\n                target.permissions.insert(\n                    parsed_assignment.name.clone(),\n                    parsed_assignment.assignment.clone(),\n                );\n            }\n        }",
        "        } else {\n            self.ensure_client_permission_target_mut(actor.server_id, client_database_id);\n            let target = self.store.client_permissions.iter_mut().find(|t| t.client_database_id == client_database_id && t.server_id == actor.server_id).unwrap();\n            for parsed_assignment in &parsed_assignments {\n                target.permissions.insert(\n                    parsed_assignment.name.clone(),\n                    parsed_assignment.assignment.clone(),\n                );\n            }\n            let _ = self.db.save_client_permission_target(target);\n        }"
    ),
    (
        "pub(super) fn handle_clientdelperm",
        "        } else if let Some(target_index) = self\n            .store\n            .client_permissions\n            .iter()\n            .position(|target| target.client_database_id == client_database_id)\n        {\n            for permission_name in &permission_names {\n                self.store.client_permissions[target_index]\n                    .permissions\n                    .remove(permission_name);\n            }\n        }",
        "        } else if let Some(target_index) = self\n            .store\n            .client_permissions\n            .iter()\n            .position(|target| target.client_database_id == client_database_id)\n        {\n            for permission_name in &permission_names {\n                self.store.client_permissions[target_index]\n                    .permissions\n                    .remove(permission_name);\n            }\n            let _ = self.db.save_client_permission_target(&self.store.client_permissions[target_index]);\n        }"
    ),
    (
        "pub(super) fn handle_channelclientaddperm",
        "        let target =\n            self.ensure_channel_client_permission_target_mut(channel_id, client_database_id);\n        for parsed_assignment in parsed_assignments {\n            target\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }",
        "        self.ensure_channel_client_permission_target_mut(channel_id, client_database_id);\n        let target = self.store.channel_client_permissions.iter_mut().find(|t| t.channel_id == channel_id && t.client_database_id == client_database_id).unwrap();\n        for parsed_assignment in parsed_assignments {\n            target\n                .permissions\n                .insert(parsed_assignment.name, parsed_assignment.assignment);\n        }\n        let _ = self.db.save_channel_client_permission_target(target);"
    ),
    (
        "pub(super) fn handle_channelclientdelperm",
        "        if let Some(target_index) = self\n            .store\n            .channel_client_permissions\n            .iter()\n            .position(|t| t.channel_id == channel_id && t.client_database_id == client_database_id)\n        {\n            for permission_name in permission_names {\n                self.store.channel_client_permissions[target_index]\n                    .permissions\n                    .remove(&permission_name);\n            }\n        }",
        "        if let Some(target_index) = self\n            .store\n            .channel_client_permissions\n            .iter()\n            .position(|t| t.channel_id == channel_id && t.client_database_id == client_database_id)\n        {\n            for permission_name in permission_names {\n                self.store.channel_client_permissions[target_index]\n                    .permissions\n                    .remove(&permission_name);\n            }\n            let _ = self.db.save_channel_client_permission_target(&self.store.channel_client_permissions[target_index]);\n        }"
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
    if old_text not in chunk:
        print(f"Warning: could not find exact old text in {func_name}, skipping.")
        continue
    new_chunk = chunk.replace(old_text, new_text)
    content = content[:start_idx] + new_chunk + content[end_idx + 20:]
    print(f"Patched {func_name}")

with open(file_path, "w", encoding="utf-8") as f:
    f.write(content)
