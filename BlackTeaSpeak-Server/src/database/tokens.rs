use super::*;
use rusqlite::params;

impl Database {
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
}
