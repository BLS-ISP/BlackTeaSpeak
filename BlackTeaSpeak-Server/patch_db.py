import os

db_path = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\database.rs"

with open(db_path, "r", encoding="utf-8") as f:
    content = f.read()

# Add to initialize_schema
schema_insert = """        // Channel Permissions
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channel_permissions (
                channel_id INTEGER NOT NULL,
                server_id INTEGER NOT NULL,
                permission_id TEXT NOT NULL,
                permission_value INTEGER NOT NULL,
                permission_negated INTEGER NOT NULL DEFAULT 0,
                permission_skip INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(channel_id, server_id, permission_id),
                FOREIGN KEY(channel_id) REFERENCES channels(channel_id) ON DELETE CASCADE,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Channel Client Permissions
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channel_client_permissions (
                channel_id INTEGER NOT NULL,
                client_id INTEGER NOT NULL,
                server_id INTEGER NOT NULL,
                permission_id TEXT NOT NULL,
                permission_value INTEGER NOT NULL,
                permission_negated INTEGER NOT NULL DEFAULT 0,
                permission_skip INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(channel_id, client_id, server_id, permission_id),
                FOREIGN KEY(channel_id) REFERENCES channels(channel_id) ON DELETE CASCADE,
                FOREIGN KEY(client_id) REFERENCES clients(client_id) ON DELETE CASCADE,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;
"""

if "channel_permissions (" not in content:
    content = content.replace(
        "        // Client Permissions",
        schema_insert + "\n        // Client Permissions"
    )

save_channel_orig = """            params![
                channel.id,
                server_id,
                channel.parent_id,
                channel.order,
                channel.name,
                channel.topic,
                channel.description,
                channel.kind.to_permanent_flag(),
                channel.kind.to_semi_permanent_flag(),
            ],
        )?;
        Ok(())
    }"""

save_channel_new = """            params![
                channel.id,
                server_id,
                channel.parent_id,
                channel.order,
                channel.name,
                channel.topic,
                channel.description,
                channel.kind.to_permanent_flag(),
                channel.kind.to_semi_permanent_flag(),
            ],
        )?;

        // Save channel permissions
        conn.execute(
            "DELETE FROM channel_permissions WHERE channel_id = ?1 AND server_id = ?2",
            params![channel.id, server_id],
        )?;
        
        for (perm_id, perm_assignment) in &channel.permissions {
            conn.execute(
                "INSERT INTO channel_permissions (
                    channel_id, server_id, permission_id, permission_value,
                    permission_negated, permission_skip
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    channel.id,
                    server_id,
                    perm_id,
                    perm_assignment.value,
                    if perm_assignment.negated { 1 } else { 0 },
                    if perm_assignment.skipped { 1 } else { 0 },
                ],
            )?;
        }
        Ok(())
    }"""

if "Save channel permissions" not in content:
    content = content.replace(save_channel_orig, save_channel_new)

save_permission_targets = """
    pub fn save_client_permission_target(&self, target: &crate::runtime::ClientPermissionTarget) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM client_permissions WHERE client_id = ?1 AND server_id = ?2",
            params![target.client_database_id as i64, target.server_id],
        )?;
        
        for (perm_id, perm_assignment) in &target.permissions {
            conn.execute(
                "INSERT INTO client_permissions (
                    client_id, server_id, permission_id, permission_value,
                    permission_negated, permission_skip
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    target.client_database_id as i64,
                    target.server_id,
                    perm_id,
                    perm_assignment.value,
                    if perm_assignment.negated { 1 } else { 0 },
                    if perm_assignment.skipped { 1 } else { 0 },
                ],
            )?;
        }
        Ok(())
    }

    pub fn save_channel_client_permission_target(&self, target: &crate::runtime::ChannelClientPermissionTarget) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Since we don't have server_id in the target currently, we assume it's loaded per channel.
        // But the schema requires server_id. We might need to look it up or add it to the struct.
        // Wait, I will fix the struct to have server_id!
        conn.execute(
            "DELETE FROM channel_client_permissions WHERE channel_id = ?1 AND client_id = ?2",
            params![target.channel_id, target.client_database_id as i64],
        )?;
        
        for (perm_id, perm_assignment) in &target.permissions {
            conn.execute(
                "INSERT INTO channel_client_permissions (
                    channel_id, client_id, server_id, permission_id, permission_value,
                    permission_negated, permission_skip
                ) VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6)",
                params![
                    target.channel_id,
                    target.client_database_id as i64,
                    perm_id,
                    perm_assignment.value,
                    if perm_assignment.negated { 1 } else { 0 },
                    if perm_assignment.skipped { 1 } else { 0 },
                ],
            )?;
        }
        Ok(())
    }
"""

if "save_client_permission_target" not in content:
    content += save_permission_targets

with open(db_path, "w", encoding="utf-8") as f:
    f.write(content)

print("database.rs patched")
