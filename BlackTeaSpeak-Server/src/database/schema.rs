use super::*;
use rusqlite::params;

impl Database {
    pub(crate) fn initialize_schema(&self) -> Result<()> {
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
}
