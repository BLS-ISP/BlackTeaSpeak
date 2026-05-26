use super::*;
use rusqlite::params;

impl Database {
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
}
