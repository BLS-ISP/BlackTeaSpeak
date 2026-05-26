use super::*;
use rusqlite::params;

impl Database {
    pub fn save_virtual_server(&self, server: &crate::runtime::VirtualServer) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO servers (
                server_id, server_name, server_welcomemessage, server_password,
                server_maxclients, server_port
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(server_id) DO UPDATE SET
                server_name=excluded.server_name,
                server_welcomemessage=excluded.server_welcomemessage,
                server_password=excluded.server_password,
                server_maxclients=excluded.server_maxclients,
                server_port=excluded.server_port",
            params![
                server.id,
                server.name,
                server.welcome_message,
                "", // Password
                server.max_clients,
                server.port,
            ],
        )?;
        Ok(())
    }
    pub fn load_virtual_servers(&self) -> Result<std::collections::BTreeMap<u32, crate::runtime::VirtualServer>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT server_id, server_name, server_welcomemessage, server_maxclients, server_port FROM servers")?;
        let server_iter = stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let port: u16 = row.get(4)?;
            let name: String = row.get(1)?;
            let welcome_message: String = row.get(2)?;
            let max_clients: u32 = row.get(3)?;
            
            // Reconstruct VirtualServer
            Ok(crate::runtime::VirtualServer {
                id,
                port,
                name,
                unique_identifier: format!("server-{}", id),
                welcome_message,
                host_message: String::new(),
                host_message_mode: 0,
                ask_for_privilegekey: 0,
                max_clients,
                antiflood_points_tick_reduce: 5,
                antiflood_points_needed_command_block: 150,
                antiflood_points_needed_ip_block: 250,
                antiflood_ban_time: 600,
            })
        })?;

        let mut map = std::collections::BTreeMap::new();
        for server in server_iter {
            let server = server?;
            map.insert(server.id, server);
        }
        Ok(map)
    }
}
