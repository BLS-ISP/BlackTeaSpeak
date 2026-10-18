use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_custominfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(cldbid) = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };

        let Some(props) = self
            .store
            .custom_client_properties
            .get(&(server_id, cldbid))
            .filter(|p| !p.is_empty())
        else {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        };

        let rows = props
            .iter()
            .map(|(ident, value)| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cldbid"), cldbid.to_string());
                row.insert(String::from("ident"), ident.clone());
                row.insert(String::from("value"), value.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_customsearch(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(ident) = request
            .named_args
            .get("ident")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "ident is required");
        };
        let Some(pattern) = request.named_args.get("pattern") else {
            return QueryResponse::error(512, "pattern is required");
        };

        let rows = self
            .store
            .custom_client_properties
            .iter()
            .filter(|((s_id, _), _)| *s_id == server_id)
            .filter_map(|((_, cldbid), props)| {
                let val = props.get(ident)?;
                if sql_like_matches(val, pattern) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("cldbid"), cldbid.to_string());
                    row.insert(String::from("ident"), ident.to_string());
                    row.insert(String::from("value"), val.clone());
                    Some(row)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_customset(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let cldbid = request
            .named_args
            .get("cldbid")
            .or_else(|| request.option_groups.first().and_then(|g| g.get("cldbid")))
            .and_then(|v| v.parse::<u64>().ok());
        let Some(cldbid) = cldbid else {
            return QueryResponse::error(512, "cldbid is required");
        };

        let mut pairs = Vec::new();
        if !request.option_groups.is_empty() {
            for group in &request.option_groups {
                if let (Some(ident), Some(value)) = (group.get("ident"), group.get("value")) {
                    if !ident.trim().is_empty() {
                        pairs.push((ident.trim().to_string(), value.clone()));
                    }
                }
            }
        } else if let (Some(ident), Some(value)) = (
            request.named_args.get("ident"),
            request.named_args.get("value"),
        ) {
            if !ident.trim().is_empty() {
                pairs.push((ident.trim().to_string(), value.clone()));
            }
        }

        if pairs.is_empty() {
            return QueryResponse::error(512, "ident and value are required");
        }

        let entry = self
            .store
            .custom_client_properties
            .entry((server_id, cldbid))
            .or_default();
        for (ident, value) in pairs {
            entry.insert(ident, value);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_customdelete(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(cldbid) = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let Some(ident) = request
            .named_args
            .get("ident")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "ident is required");
        };

        let removed = self
            .store
            .custom_client_properties
            .get_mut(&(server_id, cldbid))
            .and_then(|props| props.remove(ident))
            .is_some();
        if !removed {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientdblist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let start = request
            .named_args
            .get("start")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let duration = request
            .named_args
            .get("duration")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(25);
        let include_count = request.flags.contains("count");

        let total_count = self.store.clients.len();
        let mut rows = self
            .store
            .clients
            .values()
            .skip(start)
            .take(duration)
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cldbid"), client.database_id.to_string());
                row.insert(
                    String::from("client_unique_identifier"),
                    client.unique_identifier.clone(),
                );
                row.insert(String::from("client_nickname"), client.nickname.clone());
                row.insert(String::from("client_created"), client.created_at.to_string());
                row.insert(
                    String::from("client_lastconnected"),
                    client.last_connected_at.to_string(),
                );
                row.insert(
                    String::from("client_totalconnections"),
                    client.total_connections.to_string(),
                );
                row.insert(
                    String::from("client_description"),
                    client.description.clone(),
                );
                row.insert(
                    String::from("client_lastip"),
                    self.store
                        .online_clients
                        .values()
                        .find(|c| c.database_id == client.database_id)
                        .map(|c| c.connection_ip.clone())
                        .unwrap_or_else(|| String::from("127.0.0.1")),
                );
                row
            })
            .collect::<Vec<_>>();

        if include_count && let Some(first) = rows.first_mut() {
            first.insert(String::from("count"), total_count.to_string());
        }

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientdbinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(cldbid) = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let Some(client) = self.store.clients.get(&cldbid) else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("client_database_id"), client.database_id.to_string());
        row.insert(
            String::from("client_unique_identifier"),
            client.unique_identifier.clone(),
        );
        row.insert(String::from("client_nickname"), client.nickname.clone());
        row.insert(String::from("client_created"), client.created_at.to_string());
        row.insert(
            String::from("client_lastconnected"),
            client.last_connected_at.to_string(),
        );
        row.insert(
            String::from("client_totalconnections"),
            client.total_connections.to_string(),
        );
        row.insert(
            String::from("client_description"),
            client.description.clone(),
        );
        row.insert(
            String::from("client_month_bytes_uploaded"),
            client.month_bytes_uploaded.to_string(),
        );
        row.insert(
            String::from("client_month_bytes_downloaded"),
            client.month_bytes_downloaded.to_string(),
        );
        row.insert(
            String::from("client_total_bytes_uploaded"),
            client.total_bytes_uploaded.to_string(),
        );
        row.insert(
            String::from("client_total_bytes_downloaded"),
            client.total_bytes_downloaded.to_string(),
        );
        row.insert(
            String::from("client_flag_avatar"),
            client.client_flag_avatar.clone(),
        );
        row.insert(
            String::from("client_lastip"),
            self.store
                .online_clients
                .values()
                .find(|c| c.database_id == client.database_id)
                .map(|c| c.connection_ip.clone())
                .unwrap_or_else(|| String::from("127.0.0.1")),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientdbedit(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(cldbid) = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_modify_dbproperties", "b_client_modify_description"],
            "b_client_modify_dbproperties",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(client) = self.store.clients.get_mut(&cldbid) else {
            return QueryResponse::error(768, "client not found");
        };
        if let Some(description) = request.named_args.get("client_description") {
            client.description = description.clone();
            let _ = self.db.save_client(client);
            for online in self.store.online_clients.values_mut() {
                if online.database_id == cldbid {
                    online
                        .extra_properties
                        .insert(String::from("client_description"), description.clone());
                }
            }
        }
        QueryResponse::ok()
    }

    pub(crate) fn handle_clientdbdelete(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(cldbid) = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_delete_dbproperties"],
            "b_client_delete_dbproperties",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if self
            .store
            .online_clients
            .values()
            .any(|c| c.server_id == server_id && c.database_id == cldbid)
        {
            return QueryResponse::error(512, "cannot delete currently online client from database");
        }
        if self.store.clients.remove(&cldbid).is_none() {
            return QueryResponse::error(768, "client not found");
        }
        self.store
            .custom_client_properties
            .remove(&(server_id, cldbid));
        QueryResponse::ok()
    }

    pub(crate) fn handle_clientdbfind(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(pattern) = request.named_args.get("pattern") else {
            return QueryResponse::error(512, "pattern is required");
        };
        let search_uid = request.flags.contains("uid");

        let rows = self
            .store
            .clients
            .values()
            .filter(|client| {
                if search_uid {
                    client.unique_identifier == *pattern
                        || sql_like_matches(&client.unique_identifier, pattern)
                } else {
                    sql_like_matches(&client.nickname, pattern)
                }
            })
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cldbid"), client.database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok_rows(rows)
    }
}

fn sql_like_matches(value: &str, pattern: &str) -> bool {
    let val_lower = value.to_ascii_lowercase();
    let pat_lower = pattern.to_ascii_lowercase();
    if !pat_lower.contains('%') {
        return val_lower.contains(&pat_lower);
    }
    let parts: Vec<&str> = pat_lower.split('%').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }
    let mut pos = 0;
    for (idx, part) in parts.iter().enumerate() {
        if idx == 0 && !pat_lower.starts_with('%') {
            if !val_lower.starts_with(part) {
                return false;
            }
            pos = part.len();
            continue;
        }
        let Some(found) = val_lower[pos..].find(part) else {
            return false;
        };
        pos += found + part.len();
    }
    if !pat_lower.ends_with('%') && let Some(last) = parts.last() {
        return val_lower.ends_with(last);
    }
    true
}
