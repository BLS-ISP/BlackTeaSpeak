use super::*;
use rusqlite::params;

impl Database {
    pub fn save_server_group(&self, server_id: u32, group: &crate::runtime::ServerGroup) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO server_groups (
                group_id, server_id, name, type, iconid, savedb
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(group_id) DO UPDATE SET
                name=excluded.name,
                type=excluded.type,
                iconid=excluded.iconid,
                savedb=excluded.savedb",
            params![
                group.id,
                server_id,
                group.name,
                group.group_type,
                group.icon_id,
                if group.save_db { 1 } else { 0 },
            ],
        )?;

        // Save permissions
        conn.execute(
            "DELETE FROM group_server_permissions WHERE group_id = ?1 AND server_id = ?2",
            params![group.id, server_id],
        )?;
        
        for (perm_id, perm_assignment) in &group.permissions {
            conn.execute(
                "INSERT INTO group_server_permissions (
                    group_id, server_id, permission_id, permission_value,
                    permission_negated, permission_skip
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    group.id,
                    server_id,
                    perm_id,
                    perm_assignment.value,
                    if perm_assignment.negated { 1 } else { 0 },
                    if perm_assignment.skipped { 1 } else { 0 },
                ],
            )?;
        }

        Ok(())
    }
    pub fn load_server_groups(&self) -> Result<std::collections::BTreeMap<u32, crate::runtime::ServerGroup>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT group_id, name, type, iconid, savedb FROM server_groups")?;
        
        let mut groups = std::collections::BTreeMap::new();
        
        let group_iter = stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let name: String = row.get(1)?;
            let group_type: u32 = row.get(2)?;
            let icon_id: i64 = row.get(3)?;
            let save_db_int: i32 = row.get(4)?;

            Ok(crate::runtime::ServerGroup {
                id,
                name,
                group_type,
                icon_id,
                save_db: save_db_int != 0,
                permissions: std::collections::BTreeMap::new(),
            })
        })?;

        for group in group_iter {
            let group = group?;
            groups.insert(group.id, group);
        }

        let mut perm_stmt = conn.prepare("SELECT group_id, permission_id, permission_value, permission_negated, permission_skip FROM group_server_permissions")?;
        let perm_iter = perm_stmt.query_map([], |row| {
            let group_id: u32 = row.get(0)?;
            let permission_id: String = row.get(1)?;
            let value: i64 = row.get(2)?;
            let negated: i32 = row.get(3)?;
            let skip: i32 = row.get(4)?;
            
            Ok((group_id, permission_id, crate::runtime::PermissionAssignment {
                value,
                negated: negated != 0,
                skipped: skip != 0,
            }))
        })?;

        for perm in perm_iter {
            let (group_id, permission_id, assignment) = perm?;
            if let Some(group) = groups.get_mut(&group_id) {
                group.permissions.insert(permission_id, assignment);
            }
        }

        Ok(groups)
    }
    pub fn save_channel_group(&self, server_id: u32, group: &crate::runtime::ChannelGroup) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO channel_groups (
                group_id, server_id, name, type, iconid, savedb
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(group_id) DO UPDATE SET
                name=excluded.name,
                type=excluded.type,
                iconid=excluded.iconid,
                savedb=excluded.savedb",
            params![
                group.id,
                server_id,
                group.name,
                group.group_type,
                group.icon_id,
                if group.save_db { 1 } else { 0 },
            ],
        )?;

        // Save permissions
        conn.execute(
            "DELETE FROM group_channel_permissions WHERE group_id = ?1 AND server_id = ?2",
            params![group.id, server_id],
        )?;
        
        for (perm_id, perm_assignment) in &group.permissions {
            conn.execute(
                "INSERT INTO group_channel_permissions (
                    group_id, server_id, permission_id, permission_value,
                    permission_negated, permission_skip
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    group.id,
                    server_id,
                    perm_id,
                    perm_assignment.value,
                    if perm_assignment.negated { 1 } else { 0 },
                    if perm_assignment.skipped { 1 } else { 0 },
                ],
            )?;
        }

        Ok(())
    }
    pub fn load_channel_groups(&self) -> Result<std::collections::BTreeMap<u32, crate::runtime::ChannelGroup>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT group_id, name, type, iconid, savedb FROM channel_groups")?;
        
        let mut groups = std::collections::BTreeMap::new();

        let group_iter = stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let name: String = row.get(1)?;
            let group_type: u32 = row.get(2)?;
            let icon_id: i64 = row.get(3)?;
            let save_db_int: i32 = row.get(4)?;

            Ok(crate::runtime::ChannelGroup {
                id,
                name,
                group_type,
                icon_id,
                save_db: save_db_int != 0,
                permissions: std::collections::BTreeMap::new(),
            })
        })?;

        for group in group_iter {
            let group = group?;
            groups.insert(group.id, group);
        }

        let mut perm_stmt = conn.prepare("SELECT group_id, permission_id, permission_value, permission_negated, permission_skip FROM group_channel_permissions")?;
        let perm_iter = perm_stmt.query_map([], |row| {
            let group_id: u32 = row.get(0)?;
            let permission_id: String = row.get(1)?;
            let value: i64 = row.get(2)?;
            let negated: i32 = row.get(3)?;
            let skip: i32 = row.get(4)?;
            
            Ok((group_id, permission_id, crate::runtime::PermissionAssignment {
                value,
                negated: negated != 0,
                skipped: skip != 0,
            }))
        })?;

        for perm in perm_iter {
            let (group_id, permission_id, assignment) = perm?;
            if let Some(group) = groups.get_mut(&group_id) {
                group.permissions.insert(permission_id, assignment);
            }
        }

        Ok(groups)
    }
    pub fn save_channel_group_assignment(&self, server_id: u32, assignment: &crate::runtime::ChannelGroupAssignment) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO channel_group_members (
                server_id, channel_id, client_id, group_id
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(server_id, channel_id, client_id) DO UPDATE SET
                group_id=excluded.group_id",
            params![
                server_id,
                assignment.channel_id,
                assignment.client_database_id as i64,
                assignment.channel_group_id,
            ],
        )?;
        Ok(())
    }
    pub fn load_channel_group_assignments(&self) -> Result<Vec<crate::runtime::ChannelGroupAssignment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT channel_id, client_id, group_id FROM channel_group_members")?;
        
        let assignment_iter = stmt.query_map([], |row| {
            let channel_id: u32 = row.get(0)?;
            let client_database_id: i64 = row.get(1)?;
            let channel_group_id: u32 = row.get(2)?;

            Ok(crate::runtime::ChannelGroupAssignment {
                channel_id,
                client_database_id: client_database_id as u64,
                channel_group_id,
            })
        })?;

        let mut vec = Vec::new();
        for assignment in assignment_iter {
            vec.push(assignment?);
        }
        Ok(vec)
    }
}
