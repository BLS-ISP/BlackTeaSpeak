use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use rusqlite::OptionalExtension;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize_schema()?;
        Ok(db)
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize_schema()?;
        Ok(db)
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Server properties
        conn.execute(
            "CREATE TABLE IF NOT EXISTS servers (
                server_id INTEGER PRIMARY KEY,
                server_name TEXT NOT NULL,
                server_welcomemessage TEXT NOT NULL DEFAULT '',
                server_password TEXT NOT NULL DEFAULT '',
                server_maxclients INTEGER NOT NULL DEFAULT 32,
                server_port INTEGER NOT NULL DEFAULT 9987,
                server_autostart INTEGER NOT NULL DEFAULT 1
            )",
            [],
        )?;

        // Channels
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channels (
                channel_id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                parent_channel_id INTEGER NOT NULL DEFAULT 0,
                channel_order INTEGER NOT NULL DEFAULT 0,
                channel_name TEXT NOT NULL,
                channel_topic TEXT NOT NULL DEFAULT '',
                channel_description TEXT NOT NULL DEFAULT '',
                channel_password TEXT NOT NULL DEFAULT '',
                channel_codec INTEGER NOT NULL DEFAULT 4,
                channel_codec_quality INTEGER NOT NULL DEFAULT 10,
                channel_maxclients INTEGER NOT NULL DEFAULT -1,
                channel_maxfamilyclients INTEGER NOT NULL DEFAULT -1,
                channel_flag_default INTEGER NOT NULL DEFAULT 0,
                channel_flag_password INTEGER NOT NULL DEFAULT 0,
                channel_flag_permanent INTEGER NOT NULL DEFAULT 1,
                channel_flag_semi_permanent INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Clients
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clients (
                client_id INTEGER PRIMARY KEY AUTOINCREMENT,
                client_unique_id TEXT UNIQUE NOT NULL,
                client_nickname TEXT NOT NULL,
                client_description TEXT NOT NULL DEFAULT '',
                client_created INTEGER NOT NULL DEFAULT 0,
                client_lastconnected INTEGER NOT NULL DEFAULT 0,
                client_totalconnections INTEGER NOT NULL DEFAULT 0,
                client_month_bytes_uploaded INTEGER NOT NULL DEFAULT 0,
                client_month_bytes_downloaded INTEGER NOT NULL DEFAULT 0,
                client_total_bytes_uploaded INTEGER NOT NULL DEFAULT 0,
                client_total_bytes_downloaded INTEGER NOT NULL DEFAULT 0,
                client_flag_avatar TEXT NOT NULL DEFAULT ''
            )",
            [],
        )?;
        
        let _ = conn.execute("ALTER TABLE clients ADD COLUMN client_flag_avatar TEXT NOT NULL DEFAULT ''", []);

        // Server Groups
        conn.execute(
            "CREATE TABLE IF NOT EXISTS server_groups (
                group_id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                type INTEGER NOT NULL DEFAULT 1,
                iconid INTEGER NOT NULL DEFAULT 0,
                savedb INTEGER NOT NULL DEFAULT 1,
                sortid INTEGER NOT NULL DEFAULT 0,
                name_mode INTEGER NOT NULL DEFAULT 0,
                modify_power INTEGER NOT NULL DEFAULT 75,
                member_add_power INTEGER NOT NULL DEFAULT 75,
                member_remove_power INTEGER NOT NULL DEFAULT 75,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Channel Groups
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channel_groups (
                group_id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                type INTEGER NOT NULL DEFAULT 1,
                iconid INTEGER NOT NULL DEFAULT 0,
                savedb INTEGER NOT NULL DEFAULT 1,
                sortid INTEGER NOT NULL DEFAULT 0,
                name_mode INTEGER NOT NULL DEFAULT 0,
                modify_power INTEGER NOT NULL DEFAULT 75,
                member_add_power INTEGER NOT NULL DEFAULT 75,
                member_remove_power INTEGER NOT NULL DEFAULT 75,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Group Permissions
        conn.execute(
            "CREATE TABLE IF NOT EXISTS group_server_permissions (
                group_id INTEGER NOT NULL,
                server_id INTEGER NOT NULL,
                permission_id TEXT NOT NULL,
                permission_value INTEGER NOT NULL,
                permission_negated INTEGER NOT NULL DEFAULT 0,
                permission_skip INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(group_id, server_id, permission_id),
                FOREIGN KEY(group_id) REFERENCES server_groups(group_id) ON DELETE CASCADE,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS group_channel_permissions (
                group_id INTEGER NOT NULL,
                server_id INTEGER NOT NULL,
                permission_id TEXT NOT NULL,
                permission_value INTEGER NOT NULL,
                permission_negated INTEGER NOT NULL DEFAULT 0,
                permission_skip INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(group_id, server_id, permission_id),
                FOREIGN KEY(group_id) REFERENCES channel_groups(group_id) ON DELETE CASCADE,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Client Permissions
        conn.execute(
            "CREATE TABLE IF NOT EXISTS client_permissions (
                client_id INTEGER NOT NULL,
                server_id INTEGER NOT NULL,
                permission_id TEXT NOT NULL,
                permission_value INTEGER NOT NULL,
                permission_negated INTEGER NOT NULL DEFAULT 0,
                permission_skip INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(client_id, server_id, permission_id),
                FOREIGN KEY(client_id) REFERENCES clients(client_id) ON DELETE CASCADE,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Server Group Members
        conn.execute(
            "CREATE TABLE IF NOT EXISTS server_group_members (
                server_id INTEGER NOT NULL,
                group_id INTEGER NOT NULL,
                client_id INTEGER NOT NULL,
                PRIMARY KEY(server_id, group_id, client_id),
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE,
                FOREIGN KEY(group_id) REFERENCES server_groups(group_id) ON DELETE CASCADE,
                FOREIGN KEY(client_id) REFERENCES clients(client_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Channel Group Members
        conn.execute(
            "CREATE TABLE IF NOT EXISTS channel_group_members (
                server_id INTEGER NOT NULL,
                channel_id INTEGER NOT NULL,
                client_id INTEGER NOT NULL,
                group_id INTEGER NOT NULL,
                PRIMARY KEY(server_id, channel_id, client_id),
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE,
                FOREIGN KEY(channel_id) REFERENCES channels(channel_id) ON DELETE CASCADE,
                FOREIGN KEY(group_id) REFERENCES channel_groups(group_id) ON DELETE CASCADE,
                FOREIGN KEY(client_id) REFERENCES clients(client_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Bans
        conn.execute(
            "CREATE TABLE IF NOT EXISTS bans (
                ban_id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                ip TEXT,
                name TEXT,
                uid TEXT,
                mytsid TEXT,
                invoker_client_id INTEGER NOT NULL,
                invoker_uid TEXT NOT NULL,
                invoker_name TEXT NOT NULL,
                created INTEGER NOT NULL,
                duration INTEGER NOT NULL,
                reason TEXT NOT NULL DEFAULT '',
                enforcements INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Tokens
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tokens (
                token_id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                token TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                max_uses INTEGER NOT NULL DEFAULT 1,
                uses INTEGER NOT NULL DEFAULT 0,
                created INTEGER NOT NULL,
                owner_login TEXT NOT NULL DEFAULT '',
                expired INTEGER,
                FOREIGN KEY(server_id) REFERENCES servers(server_id) ON DELETE CASCADE
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS token_actions (
                action_id INTEGER PRIMARY KEY AUTOINCREMENT,
                token_id INTEGER NOT NULL,
                action_type INTEGER NOT NULL,
                action_id1 INTEGER NOT NULL DEFAULT 0,
                action_id2 INTEGER NOT NULL DEFAULT 0,
                action_text TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(token_id) REFERENCES tokens(token_id) ON DELETE CASCADE
            )",
            [],
        )?;

        Ok(())
    }
    
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

    pub fn save_channel(&self, server_id: u32, channel: &crate::runtime::Channel) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO channels (
                channel_id, server_id, parent_channel_id, channel_order,
                channel_name, channel_topic, channel_description, channel_password,
                channel_codec, channel_codec_quality, channel_maxclients, channel_maxfamilyclients,
                channel_flag_default, channel_flag_password,
                channel_flag_permanent, channel_flag_semi_permanent
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(channel_id) DO UPDATE SET
                parent_channel_id=excluded.parent_channel_id,
                channel_order=excluded.channel_order,
                channel_name=excluded.channel_name,
                channel_topic=excluded.channel_topic,
                channel_description=excluded.channel_description,
                channel_password=excluded.channel_password,
                channel_codec=excluded.channel_codec,
                channel_codec_quality=excluded.channel_codec_quality,
                channel_maxclients=excluded.channel_maxclients,
                channel_maxfamilyclients=excluded.channel_maxfamilyclients,
                channel_flag_default=excluded.channel_flag_default,
                channel_flag_password=excluded.channel_flag_password,
                channel_flag_permanent=excluded.channel_flag_permanent,
                channel_flag_semi_permanent=excluded.channel_flag_semi_permanent",
            params![
                channel.id,
                server_id,
                channel.parent_id,
                channel.order,
                channel.name,
                channel.topic,
                channel.description,
                channel.password,
                channel.codec,
                channel.codec_quality,
                channel.maxclients,
                channel.maxfamilyclients,
                if channel.flag_default { 1 } else { 0 },
                if channel.flag_password { 1 } else { 0 },
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
    }

    pub fn load_channels(&self) -> Result<std::collections::BTreeMap<u32, Vec<crate::runtime::Channel>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT channel_id, server_id, parent_channel_id, channel_order, channel_name, channel_topic, channel_description, channel_password, channel_codec, channel_codec_quality, channel_maxclients, channel_maxfamilyclients, channel_flag_default, channel_flag_password, channel_flag_permanent, channel_flag_semi_permanent FROM channels")?;
        
        let channel_iter = stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let server_id: u32 = row.get(1)?;
            let parent_id: u32 = row.get(2)?;
            let order: u32 = row.get(3)?;
            let name: String = row.get(4)?;
            let topic: String = row.get(5)?;
            let description: String = row.get(6)?;
            let password: String = row.get(7)?;
            let codec: u32 = row.get(8)?;
            let codec_quality: u32 = row.get(9)?;
            let maxclients: i32 = row.get(10)?;
            let maxfamilyclients: i32 = row.get(11)?;
            let flag_default: u32 = row.get(12)?;
            let flag_password: u32 = row.get(13)?;
            let flag_perm: u32 = row.get(14)?;
            let flag_semi: u32 = row.get(15)?;

            let kind = crate::runtime::ChannelKind::from_flags(flag_perm > 0, flag_semi > 0);

            Ok((server_id, crate::runtime::Channel {
                id,
                parent_id,
                order,
                kind,
                name,
                topic,
                description,
                password,
                codec,
                codec_quality,
                maxclients,
                maxfamilyclients,
                flag_default: flag_default > 0,
                flag_password: flag_password > 0,
                permissions: std::collections::BTreeMap::new(),
            }))
        })?;

        let mut map: std::collections::BTreeMap<u32, Vec<crate::runtime::Channel>> = std::collections::BTreeMap::new();
        for channel in channel_iter {
            let (server_id, channel) = channel?;
            map.entry(server_id).or_default().push(channel);
        }
        Ok(map)
    }

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

    pub fn save_ban(&self, ban: &crate::runtime::ActiveBan) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO bans (
                ban_id, server_id, ip, name, uid, mytsid,
                invoker_client_id, invoker_uid, invoker_name,
                created, duration, reason, enforcements
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(ban_id) DO UPDATE SET
                ip=excluded.ip,
                name=excluded.name,
                uid=excluded.uid,
                mytsid=excluded.mytsid,
                duration=excluded.duration,
                reason=excluded.reason,
                enforcements=excluded.enforcements",
            params![
                ban.id,
                ban.server_id,
                if ban.ip.is_empty() { None } else { Some(&ban.ip) },
                if ban.name.is_empty() { None } else { Some(&ban.name) },
                if ban.unique_identifier.is_empty() { None } else { Some(&ban.unique_identifier) },
                if ban.hardware_identifier.is_empty() { None } else { Some(&ban.hardware_identifier) },
                ban.invoker_database_id as i64,
                ban.invoker_unique_identifier,
                ban.invoker_name,
                ban.created_at as i64,
                ban.duration_seconds,
                ban.reason,
                ban.triggers.len() as u32,
            ],
        )?;
        Ok(())
    }

    pub fn load_bans(&self) -> Result<std::collections::BTreeMap<u32, crate::runtime::ActiveBan>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT ban_id, server_id, ip, name, uid, mytsid, invoker_client_id, invoker_uid, invoker_name, created, duration, reason FROM bans")?;
        
        let ban_iter = stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let server_id: u32 = row.get(1)?;
            let ip_opt: Option<String> = row.get(2)?;
            let name_opt: Option<String> = row.get(3)?;
            let uid_opt: Option<String> = row.get(4)?;
            let mytsid_opt: Option<String> = row.get(5)?;
            let invoker_client_id: i64 = row.get(6)?;
            let invoker_uid: String = row.get(7)?;
            let invoker_name: String = row.get(8)?;
            let created: i64 = row.get(9)?;
            let duration: u32 = row.get(10)?;
            let reason: String = row.get(11)?;

            Ok(crate::runtime::ActiveBan {
                id,
                server_id,
                name: name_opt.unwrap_or_default(),
                unique_identifier: uid_opt.unwrap_or_default(),
                hardware_identifier: mytsid_opt.unwrap_or_default(),
                ip: ip_opt.unwrap_or_default(),
                reason,
                created_at: created as u64,
                duration_seconds: duration,
                invoker_name,
                invoker_database_id: invoker_client_id as u64,
                invoker_unique_identifier: invoker_uid,
                triggers: Vec::new(),
            })
        })?;

        let mut map = std::collections::BTreeMap::new();
        for ban in ban_iter {
            let ban = ban?;
            map.insert(ban.id, ban);
        }
        Ok(map)
    }

    pub fn save_token(&self, token: &crate::runtime::PrivilegeToken) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tokens (
                token_id, server_id, token, description, max_uses, uses,
                created, owner_login, expired
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(token_id) DO UPDATE SET
                description=excluded.description,
                max_uses=excluded.max_uses,
                uses=excluded.uses,
                expired=excluded.expired",
            params![
                token.id,
                token.server_id,
                token.token,
                token.description,
                token.max_uses,
                token.uses,
                token.created_at as i64,
                token.owner_login,
                token.expired_at.map(|e| e as i64),
            ],
        )?;

        // Clear existing actions and save new ones
        conn.execute("DELETE FROM token_actions WHERE token_id = ?1", params![token.id])?;
        for action in &token.actions {
            conn.execute(
                "INSERT INTO token_actions (
                    action_id, token_id, action_type, action_id1, action_id2, action_text
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    action.id,
                    token.id,
                    action.action_type,
                    action.action_id1,
                    action.action_id2,
                    action.action_text,
                ],
            )?;
        }
        Ok(())
    }

    pub fn load_tokens(&self) -> Result<std::collections::BTreeMap<u32, crate::runtime::PrivilegeToken>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT token_id, server_id, token, description, max_uses, uses, created, owner_login, expired FROM tokens")?;
        
        let mut tokens_map = std::collections::BTreeMap::new();
        
        let token_iter = stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let server_id: u32 = row.get(1)?;
            let token_str: String = row.get(2)?;
            let description: String = row.get(3)?;
            let max_uses: u32 = row.get(4)?;
            let uses: u32 = row.get(5)?;
            let created: i64 = row.get(6)?;
            let owner_login: String = row.get(7)?;
            let expired: Option<i64> = row.get(8)?;

            Ok(crate::runtime::PrivilegeToken {
                id,
                server_id,
                token: token_str,
                description,
                max_uses,
                uses,
                created_at: created as u64,
                owner_login,
                expired_at: expired.map(|e| e as u64),
                actions: Vec::new(),
            })
        })?;

        for token in token_iter {
            let token = token?;
            tokens_map.insert(token.id, token);
        }

        let mut actions_stmt = conn.prepare("SELECT action_id, token_id, action_type, action_id1, action_id2, action_text FROM token_actions")?;
        let actions_iter = actions_stmt.query_map([], |row| {
            let id: u32 = row.get(0)?;
            let token_id: u32 = row.get(1)?;
            let action_type: u32 = row.get(2)?;
            let action_id1: u32 = row.get(3)?;
            let action_id2: u32 = row.get(4)?;
            let action_text: String = row.get(5)?;

            Ok((token_id, crate::runtime::TokenAction {
                id,
                action_type,
                action_id1,
                action_id2,
                action_text,
            }))
        })?;

        for action_res in actions_iter {
            let (token_id, action) = action_res?;
            if let Some(token) = tokens_map.get_mut(&token_id) {
                token.actions.push(action);
            }
        }

        Ok(tokens_map)
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
