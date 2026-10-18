use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_logadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let loglevel = if let Some(level_str) = request.named_args.get("loglevel") {
            let Ok(parsed) = level_str.parse::<u32>() else {
                return QueryResponse::error(512, "loglevel must be numeric (1..4)");
            };
            if !(1..=4).contains(&parsed) {
                return QueryResponse::error(512, "loglevel must be between 1 and 4");
            }
            parsed
        } else {
            4
        };

        let Some(logmsg) = request
            .named_args
            .get("logmsg")
            .or_else(|| request.named_args.get("msg"))
            .cloned()
            .filter(|s| !s.trim().is_empty())
        else {
            return QueryResponse::error(512, "logmsg is required");
        };

        let is_instance = request.flags.contains("instance")
            || request
                .named_args
                .get("instance")
                .is_some_and(|v| runtime_bool_flag(v))
            || session.selected_virtual_server_id.is_none();

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let required_perms: &[&str] = if is_instance {
            &["b_serverinstance_log_add", "b_virtualserver_log_add"]
        } else {
            &["b_virtualserver_log_add", "b_serverinstance_log_add"]
        };
        if let Err(permission_name) =
            check_required_permission(&actor_permissions, required_perms, required_perms[0])
        {
            return self.insufficient_permission_response(permission_name);
        }

        let target_sid = if is_instance {
            0
        } else {
            session.selected_virtual_server_id.unwrap_or(1)
        };
        self.append_server_log(target_sid, loglevel, "Query", logmsg);
        QueryResponse::ok()
    }

    pub(crate) fn handle_logview(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let is_instance = request.flags.contains("instance")
            || request
                .named_args
                .get("instance")
                .is_some_and(|v| runtime_bool_flag(v))
            || session.selected_virtual_server_id.is_none();

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let required_perms: &[&str] = if is_instance {
            &["b_serverinstance_log_view", "b_virtualserver_log_view"]
        } else {
            &["b_virtualserver_log_view", "b_serverinstance_log_view"]
        };
        if let Err(permission_name) =
            check_required_permission(&actor_permissions, required_perms, required_perms[0])
        {
            return self.insufficient_permission_response(permission_name);
        }

        let target_sid = if is_instance {
            0
        } else {
            session.selected_virtual_server_id.unwrap_or(1)
        };

        let limit = request
            .named_args
            .get("lines")
            .or_else(|| request.named_args.get("limit"))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100)
            .clamp(1, 500);
        let reverse = request
            .named_args
            .get("reverse")
            .is_some_and(|v| runtime_bool_flag(v));

        let mut matching = self
            .store
            .server_logs
            .iter()
            .filter(|entry| entry.server_id == target_sid)
            .cloned()
            .collect::<Vec<_>>();

        if reverse {
            matching.reverse();
        }
        if matching.len() > limit {
            matching.truncate(limit);
        }

        if matching.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        let file_size = matching
            .iter()
            .map(|e| e.message.len() + 32)
            .sum::<usize>()
            .to_string();

        let rows = matching
            .into_iter()
            .map(|entry| {
                let level_name = match entry.level {
                    1 => "ERROR",
                    2 => "WARNING",
                    3 => "DEBUG",
                    _ => "INFO",
                };
                let formatted_line = format!(
                    "{}|{}|{}|{}|{}",
                    entry.timestamp, level_name, entry.channel, entry.server_id, entry.message
                );
                let mut row = BTreeMap::new();
                row.insert(String::from("last_pos"), String::from("0"));
                row.insert(String::from("file_size"), file_size.clone());
                row.insert(String::from("l"), formatted_line);
                row.insert(String::from("timestamp"), entry.timestamp.to_string());
                row.insert(String::from("loglevel"), entry.level.to_string());
                row.insert(String::from("channel"), entry.channel);
                row.insert(String::from("sid"), entry.server_id.to_string());
                row.insert(String::from("msg"), entry.message);
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_apikeyadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let scope = request
            .named_args
            .get("scope")
            .cloned()
            .unwrap_or_else(|| String::from("manage"));
        if !matches!(scope.as_str(), "manage" | "write" | "read") {
            return QueryResponse::error(512, "invalid scope");
        }
        let lifetime_days = request
            .named_args
            .get("lifetime")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(14);
        let cldbid = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(actor.client_database_id);
        let sid = session.selected_virtual_server_id.unwrap_or(0);

        let next_id = self
            .store
            .instance_properties
            .get("apikey:next_id")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1);
        self.store
            .instance_properties
            .insert(String::from("apikey:next_id"), (next_id + 1).to_string());

        let created_at = current_unix_timestamp();
        let expires_at = if lifetime_days == 0 {
            0
        } else {
            created_at.saturating_add(lifetime_days.saturating_mul(86_400))
        };
        let apikey = format!("BTEA-APIKEY-{next_id}-{created_at}-{cldbid}");

        let entry = serde_json::json!({
            "id": next_id,
            "sid": sid,
            "cldbid": cldbid,
            "scope": scope,
            "time_created": created_at,
            "time_expire": expires_at,
            "apikey": apikey,
        });
        self.store
            .instance_properties
            .insert(format!("apikey:item:{next_id}"), entry.to_string());

        let mut row = BTreeMap::new();
        row.insert(String::from("apikey"), apikey);
        row.insert(String::from("id"), next_id.to_string());
        row.insert(String::from("sid"), sid.to_string());
        row.insert(String::from("cldbid"), cldbid.to_string());
        row.insert(String::from("scope"), scope);
        row.insert(String::from("time_created"), created_at.to_string());
        row.insert(String::from("time_expire"), expires_at.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_apikeylist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let filter_cldbid = match request.named_args.get("cldbid").map(String::as_str) {
            Some("*") => None,
            Some(v) => v.parse::<u64>().ok(),
            None => Some(actor.client_database_id),
        };

        let mut rows = Vec::new();
        for (key, value) in &self.store.instance_properties {
            if !key.starts_with("apikey:item:") {
                continue;
            }
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(value) else {
                continue;
            };
            let cldbid = parsed.get("cldbid").and_then(|v| v.as_u64()).unwrap_or(0);
            if filter_cldbid.is_some_and(|expected| expected != cldbid) {
                continue;
            }
            let id = parsed.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let sid = parsed.get("sid").and_then(|v| v.as_u64()).unwrap_or(0);
            let scope = parsed
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("manage");
            let time_created = parsed
                .get("time_created")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let time_expire = parsed
                .get("time_expire")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let mut row = BTreeMap::new();
            row.insert(String::from("id"), id.to_string());
            row.insert(String::from("sid"), sid.to_string());
            row.insert(String::from("cldbid"), cldbid.to_string());
            row.insert(String::from("scope"), scope.to_string());
            row.insert(String::from("time_created"), time_created.to_string());
            row.insert(String::from("time_expire"), time_expire.to_string());
            rows.push(row);
        }

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_apikeydel(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(id) = request
            .named_args
            .get("id")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "id is required");
        };

        let key = format!("apikey:item:{id}");
        if self.store.instance_properties.remove(&key).is_none() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }
        QueryResponse::ok()
    }
}
