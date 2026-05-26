use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_tokenadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(owner_login) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[
                "b_virtualserver_token_add",
                "b_virtualserver_token_limit",
                "b_virtualserver_token_edit_all",
            ],
            "b_virtualserver_token_add",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let max_uses = if let Some(value) = request.named_args.get("token_max_uses") {
            let Some(parsed) = value.parse::<u32>().ok() else {
                return QueryResponse::error(512, "token_max_uses must be an integer");
            };
            parsed
        } else {
            0
        };
        let expired_at = if let Some(value) = request.named_args.get("token_expired") {
            let Some(parsed) = value.parse::<u64>().ok() else {
                return QueryResponse::error(512, "token_expired must be an integer");
            };
            (parsed != 0).then_some(parsed)
        } else {
            None
        };

        let action_mutations = match self.parse_token_action_mutations(request) {
            Ok(mutations) => mutations,
            Err(response) => return response,
        };
        if action_mutations
            .iter()
            .any(|mutation| !matches!(mutation, ParsedTokenActionMutation::Add { .. }))
        {
            return QueryResponse::error(512, "tokenadd only supports new actions");
        }

        let token_id = self.next_token_id();
        let created_at = current_unix_timestamp();
        let token_value = format!("compat{:08x}{:016x}", token_id, created_at);
        let mut next_action_ids = (0..action_mutations.len())
            .map(|_| self.next_token_action_id())
            .collect::<Vec<_>>()
            .into_iter();
        let mut actions = Vec::new();
        let mut rows = Vec::new();

        for mutation in action_mutations {
            if let ParsedTokenActionMutation::Add {
                action_type,
                action_id1,
                action_id2,
                action_text,
            } = mutation
            {
                let action_id = next_action_ids.next().unwrap_or(0);
                actions.push(TokenAction {
                    id: action_id,
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                });

                let mut row = BTreeMap::new();
                row.insert(String::from("action_id"), action_id.to_string());
                if rows.is_empty() {
                    row.insert(String::from("token"), token_value.clone());
                    row.insert(String::from("token_id"), token_id.to_string());
                }
                rows.push(row);
            }
        }

        self.store.tokens.insert(
            token_id,
            PrivilegeToken {
                id: token_id,
                server_id,
                token: token_value.clone(),
                description: request
                    .named_args
                    .get("token_description")
                    .cloned()
                    .unwrap_or_default(),
                max_uses,
                uses: 0,
                created_at,
                owner_login: owner_login.clone(),
                expired_at,
                actions,
            },
        );

        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("token"), token_value);
            row.insert(String::from("token_id"), token_id.to_string());
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_tokendelete(
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
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_token_delete_all"],
            "b_virtualserver_token_delete_all",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let token_id = match self.resolve_token_id(request, server_id) {
            Ok(token_id) => token_id,
            Err(response) => return response,
        };
        self.store.tokens.remove(&token_id);
        QueryResponse::ok_rows(Vec::new())
    }

    pub(crate) fn handle_tokenedit(
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
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_token_edit_all"],
            "b_virtualserver_token_edit_all",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let token_id = match self.resolve_token_id(request, server_id) {
            Ok(token_id) => token_id,
            Err(response) => return response,
        };
        let max_uses_update = if let Some(value) = request.named_args.get("token_max_uses") {
            let Some(parsed) = value.parse::<u32>().ok() else {
                return QueryResponse::error(512, "token_max_uses must be an integer");
            };
            Some(parsed)
        } else {
            None
        };
        let expired_at_update = if let Some(value) = request.named_args.get("token_expired") {
            let Some(parsed) = value.parse::<u64>().ok() else {
                return QueryResponse::error(512, "token_expired must be an integer");
            };
            Some((parsed != 0).then_some(parsed))
        } else {
            None
        };
        let action_mutations = match self.parse_token_action_mutations(request) {
            Ok(mutations) => mutations,
            Err(response) => return response,
        };
        let mut next_action_ids = (0..action_mutations
            .iter()
            .filter(|mutation| matches!(mutation, ParsedTokenActionMutation::Add { .. }))
            .count())
            .map(|_| self.next_token_action_id())
            .collect::<Vec<_>>()
            .into_iter();

        let Some(token) = self.store.tokens.get_mut(&token_id) else {
            return QueryResponse::error(768, "token not found");
        };

        if let Some(description) = request.named_args.get("token_description") {
            token.description = description.clone();
        }
        if let Some(max_uses) = max_uses_update {
            token.max_uses = max_uses;
        }
        if let Some(expired_at) = expired_at_update {
            token.expired_at = expired_at;
        }

        let mut rows = Vec::new();
        for mutation in action_mutations {
            match mutation {
                ParsedTokenActionMutation::Add {
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                } => {
                    let action_id = next_action_ids.next().unwrap_or(0);
                    token.actions.push(TokenAction {
                        id: action_id,
                        action_type,
                        action_id1,
                        action_id2,
                        action_text,
                    });

                    let mut row = BTreeMap::new();
                    row.insert(String::from("action_id"), action_id.to_string());
                    rows.push(row);
                }
                ParsedTokenActionMutation::Update {
                    action_id,
                    action_type,
                    action_id1,
                    action_id2,
                    action_text,
                } => {
                    let Some(action) = token
                        .actions
                        .iter_mut()
                        .find(|action| action.id == action_id)
                    else {
                        return QueryResponse::error(768, "token action not found");
                    };
                    action.action_type = action_type;
                    action.action_id1 = action_id1;
                    action.action_id2 = action_id2;
                    action.action_text = action_text;
                }
                ParsedTokenActionMutation::Remove { action_id } => {
                    let Some(position) = token
                        .actions
                        .iter()
                        .position(|action| action.id == action_id)
                    else {
                        return QueryResponse::error(768, "token action not found");
                    };
                    token.actions.remove(position);
                }
            }
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_tokenactionlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let token_id = match self.resolve_token_id(request, server_id) {
            Ok(token_id) => token_id,
            Err(response) => return response,
        };
        let Some(token) = self.store.tokens.get(&token_id) else {
            return QueryResponse::error(768, "token not found");
        };
        if token.owner_login != *login_name
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_virtualserver_token_list_all"],
                "b_virtualserver_token_list_all",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let rows = token
            .actions
            .iter()
            .map(|action| {
                let mut row = BTreeMap::new();
                row.insert(String::from("action_id"), action.id.to_string());
                row.insert(String::from("action_type"), action.action_type.to_string());
                row.insert(String::from("action_id1"), action.action_id1.to_string());
                row.insert(String::from("action_id2"), action.action_id2.to_string());
                row.insert(String::from("action_text"), action.action_text.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_tokenlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let offset = if let Some(value) = request.named_args.get("offset") {
            let Some(offset) = value.parse::<usize>().ok() else {
                return QueryResponse::error(512, "offset must be an integer");
            };
            offset
        } else {
            0
        };
        let limit = if let Some(value) = request.named_args.get("limit") {
            let Some(limit) = value.parse::<usize>().ok() else {
                return QueryResponse::error(512, "limit must be an integer");
            };
            Some(limit)
        } else {
            None
        };
        let list_all_tokens = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_token_list_all"],
            "b_virtualserver_token_list_all",
        )
        .is_ok();
        let own_only = request.flags.contains("own-only") || !list_all_tokens;

        let tokens = self
            .store
            .tokens
            .values()
            .filter(|token| token.server_id == server_id)
            .filter(|token| !own_only || token.owner_login == *login_name)
            .collect::<Vec<_>>();
        let token_count = tokens.len();
        let rows = tokens
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .map(|token| {
                let mut row = BTreeMap::new();
                row.insert(String::from("token_count"), token_count.to_string());
                row.insert(String::from("token_created"), token.created_at.to_string());
                row.insert(String::from("token_description"), token.description.clone());
                row.insert(
                    String::from("token_expired"),
                    token.expired_at.unwrap_or(0).to_string(),
                );
                row.insert(String::from("token_id"), token.id.to_string());
                row.insert(String::from("token_max_uses"), token.max_uses.to_string());
                row.insert(String::from("token"), token.token.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_tokenuse(
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
        let Some(token_value) = request.named_args.get("token") else {
            return QueryResponse::error(512, "token is required");
        };

        let Some(token_id) = self.store.tokens.iter().find_map(|(token_id, token)| {
            (token.server_id == server_id && token.token == *token_value).then_some(*token_id)
        }) else {
            return QueryResponse::error(768, "token not found");
        };

        let current_timestamp = current_unix_timestamp();
        let (expired_at, max_uses, uses, actions) = match self.store.tokens.get(&token_id) {
            Some(token) => (
                token.expired_at,
                token.max_uses,
                token.uses,
                token.actions.clone(),
            ),
            None => return QueryResponse::error(768, "token not found"),
        };

        if expired_at.is_some_and(|timestamp| timestamp <= current_timestamp) {
            return QueryResponse::error(768, "token expired");
        }
        if max_uses != 0 && uses >= max_uses {
            return QueryResponse::error(768, "token exhausted");
        }

        let mut server_groups_to_add = Vec::new();
        for action in &actions {
            match action.action_type {
                2 => {
                    if !self.store.server_groups.contains_key(&action.action_id1) {
                        return QueryResponse::error(4864, "Invalid group id");
                    }
                    server_groups_to_add.push(action.action_id1);
                }
                1 => {}
                other => {
                    return QueryResponse::error(
                        512,
                        format!("token action type {} not supported in baseline", other),
                    );
                }
            }
        }

        if let Some(login_name) = session.authenticated_login.as_ref() {
            let Some(account) = self.store.query_accounts.get_mut(login_name) else {
                return QueryResponse::error(768, "query account not found");
            };
            for group_id in &server_groups_to_add {
                if !account.server_groups.contains(group_id) {
                    account.server_groups.push(*group_id);
                }
            }
            account.server_groups.sort_unstable();
            account.server_groups.dedup();
        } else if let Some(actor_client_database_id) = session.actor_client_database_id_override {
            let Some(client) = self.store.online_clients.values_mut().find(|client| {
                client.server_id == server_id && client.database_id == actor_client_database_id
            }) else {
                return QueryResponse::error(768, "client not found");
            };
            for group_id in &server_groups_to_add {
                if !client.server_groups.contains(group_id) {
                    client.server_groups.push(*group_id);
                }
            }
            client.server_groups.sort_unstable();
            client.server_groups.dedup();
        }

        let should_delete = match self.store.tokens.get_mut(&token_id) {
            Some(token) => {
                token.uses = token.uses.saturating_add(1);
                token.max_uses != 0 && token.uses >= token.max_uses
            }
            None => false,
        };
        if should_delete {
            self.store.tokens.remove(&token_id);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_privilegekeylist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        self.handle_tokenlist(request, session)
    }

}
