use std::collections::BTreeMap;

use crate::protocols::tsdns::resolver::{
    TSDNS_RULE_PREFIX, list_tsdns_rules, normalize_hostname, resolve_tsdns_hostname,
};
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::permissions::session_has_permission_actor;
use crate::runtime::{BaselineRuntime, ERROR_DATABASE_EMPTY_RESULT, QuerySessionState};

impl BaselineRuntime {
    pub(crate) fn handle_tsdnsadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let domain = request
            .named_args
            .get("domain")
            .or_else(|| request.named_args.get("pattern"))
            .map(|s| normalize_hostname(s))
            .unwrap_or_default();
        let target = request
            .named_args
            .get("target")
            .or_else(|| request.named_args.get("address"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if domain.is_empty() || target.is_empty() {
            return QueryResponse::error(1538, "invalid parameter: domain and target required");
        }

        let key = format!("{TSDNS_RULE_PREFIX}{domain}");
        self.store.instance_properties.insert(key, target);
        self.persist_state_best_effort();
        QueryResponse::ok()
    }

    pub(crate) fn handle_tsdnsdel(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let domain = request
            .named_args
            .get("domain")
            .or_else(|| request.named_args.get("pattern"))
            .map(|s| normalize_hostname(s))
            .unwrap_or_default();
        if domain.is_empty() {
            return QueryResponse::error(1538, "invalid parameter: domain required");
        }

        let key = format!("{TSDNS_RULE_PREFIX}{domain}");
        if self.store.instance_properties.remove(&key).is_none() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }
        self.persist_state_best_effort();
        QueryResponse::ok()
    }

    pub(crate) fn handle_tsdnslist(
        &self,
        _request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let rules = list_tsdns_rules(self);
        if rules.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }
        let rows = rules
            .into_iter()
            .map(|rule| {
                let mut row = BTreeMap::new();
                row.insert(String::from("domain"), rule.domain_pattern);
                row.insert(String::from("target"), rule.target);
                row
            })
            .collect();
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_tsdnsresolve(
        &self,
        request: &CommandRequest,
        _session: &QuerySessionState,
    ) -> QueryResponse {
        let domain = request
            .named_args
            .get("domain")
            .or_else(|| request.named_args.get("hostname"))
            .map(|s| s.trim())
            .unwrap_or_default();
        if domain.is_empty() {
            return QueryResponse::error(1538, "invalid parameter: domain required");
        }

        let resolved = resolve_tsdns_hostname(self, domain).unwrap_or_else(|| String::from("404"));
        let mut row = BTreeMap::new();
        row.insert(String::from("domain"), normalize_hostname(domain));
        row.insert(String::from("target"), resolved);
        QueryResponse::ok_row(row)
    }
}
