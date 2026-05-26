use super::*;
use rusqlite::params;

impl Database {
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
}
