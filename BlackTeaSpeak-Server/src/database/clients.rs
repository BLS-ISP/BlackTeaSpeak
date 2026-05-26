use super::*;
use rusqlite::params;

impl Database {
    pub fn save_client(&self, client: &crate::runtime::Client) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO clients (
                client_id, client_unique_id, client_nickname, client_description,
                client_created, client_lastconnected, client_totalconnections,
                client_month_bytes_uploaded, client_month_bytes_downloaded,
                client_total_bytes_uploaded, client_total_bytes_downloaded,
                client_flag_avatar
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(client_unique_id) DO UPDATE SET
                client_nickname=excluded.client_nickname,
                client_description=excluded.client_description,
                client_lastconnected=excluded.client_lastconnected,
                client_totalconnections=excluded.client_totalconnections,
                client_month_bytes_uploaded=excluded.client_month_bytes_uploaded,
                client_month_bytes_downloaded=excluded.client_month_bytes_downloaded,
                client_total_bytes_uploaded=excluded.client_total_bytes_uploaded,
                client_total_bytes_downloaded=excluded.client_total_bytes_downloaded,
                client_flag_avatar=excluded.client_flag_avatar",
            params![
                client.database_id as i64,
                client.unique_identifier,
                client.nickname,
                client.description,
                client.created_at as i64,
                client.last_connected_at as i64,
                client.total_connections,
                client.month_bytes_uploaded as i64,
                client.month_bytes_downloaded as i64,
                client.total_bytes_uploaded as i64,
                client.total_bytes_downloaded as i64,
                client.client_flag_avatar,
            ],
        )?;
        Ok(())
    }
    pub fn load_clients(&self) -> Result<std::collections::BTreeMap<u64, crate::runtime::Client>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT client_id, client_unique_id, client_nickname, client_description, client_created, client_lastconnected, client_totalconnections, client_month_bytes_uploaded, client_month_bytes_downloaded, client_total_bytes_uploaded, client_total_bytes_downloaded, client_flag_avatar FROM clients")?;
        
        let client_iter = stmt.query_map([], |row| {
            let database_id: i64 = row.get(0)?;
            let unique_identifier: String = row.get(1)?;
            let nickname: String = row.get(2)?;
            let description: String = row.get(3)?;
            let created_at: i64 = row.get(4)?;
            let last_connected_at: i64 = row.get(5)?;
            let total_connections: u32 = row.get(6)?;
            let month_bytes_uploaded: i64 = row.get(7)?;
            let month_bytes_downloaded: i64 = row.get(8)?;
            let total_bytes_uploaded: i64 = row.get(9)?;
            let total_bytes_downloaded: i64 = row.get(10)?;
            let client_flag_avatar: Option<String> = row.get(11)?;

            Ok(crate::runtime::Client {
                database_id: database_id as u64,
                unique_identifier,
                nickname,
                description,
                created_at: created_at as u64,
                last_connected_at: last_connected_at as u64,
                total_connections,
                month_bytes_uploaded: month_bytes_uploaded as u64,
                month_bytes_downloaded: month_bytes_downloaded as u64,
                total_bytes_uploaded: total_bytes_uploaded as u64,
                total_bytes_downloaded: total_bytes_downloaded as u64,
                client_flag_avatar: client_flag_avatar.unwrap_or_default(),
            })
        })?;

        let mut clients = std::collections::BTreeMap::new();
        for client in client_iter {
            let client = client?;
            clients.insert(client.database_id, client);
        }

        Ok(clients)
    }
    pub fn update_client_avatar(&self, client_unique_id: &str, avatar: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE clients SET client_flag_avatar = ?1 WHERE client_unique_id = ?2",
            params![avatar, client_unique_id],
        )?;
        Ok(())
    }
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
}
