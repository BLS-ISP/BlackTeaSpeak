use crate::runtime::BaselineRuntime;

pub const TSDNS_RULE_PREFIX: &str = "tsdns_rule:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsdnsRule {
    pub domain_pattern: String,
    pub target: String,
}

/// Resolves a TSDNS hostname against configured rules in `store.instance_properties`
/// (`tsdns_rule:<pattern>` -> `<ip>:<port>`) and falls back to matching virtual servers
/// by `virtualserver_name` or slugified server name.
///
/// Returns `Some("<ip>:<port>")` on match, or `None` (which translates to `"404"` on the wire).
pub fn resolve_tsdns_hostname(runtime: &BaselineRuntime, raw_hostname: &str) -> Option<String> {
    let hostname = normalize_hostname(raw_hostname);
    if hostname.is_empty() {
        return None;
    }

    // 1. Exact rule match: tsdns_rule:<hostname>
    let exact_key = format!("{TSDNS_RULE_PREFIX}{hostname}");
    if let Some(target) = runtime.store.instance_properties.get(&exact_key) {
        let trimmed = target.trim();
        if trimmed.eq_ignore_ascii_case("404") || trimmed.eq_ignore_ascii_case("NORESPONSE") || trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }

    // 2. Wildcard rule match (e.g. *.example.com or *), preferring longest pattern
    let mut best_wildcard: Option<(usize, &str)> = None;
    for (key, value) in &runtime.store.instance_properties {
        if let Some(pattern) = key.strip_prefix(TSDNS_RULE_PREFIX) {
            let pattern_norm = normalize_hostname(pattern);
            if pattern_norm.contains('*') && wildcard_matches(&pattern_norm, &hostname) {
                let specificity = pattern_norm.len();
                if best_wildcard.map_or(true, |(best_len, _)| specificity > best_len) {
                    best_wildcard = Some((specificity, value.as_str()));
                }
            }
        }
    }
    if let Some((_, target)) = best_wildcard {
        let trimmed = target.trim();
        if trimmed.eq_ignore_ascii_case("404") || trimmed.eq_ignore_ascii_case("NORESPONSE") || trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }

    // 3. Fallback: match active virtual server's normalized name or slug
    let default_ip = runtime
        .store
        .instance_properties
        .get("serverinstance_tsdns_default_ip")
        .map(String::as_str);

    for server in runtime.store.virtual_servers.values() {
        if server.status != "online" {
            continue;
        }
        let vs_name_norm = normalize_hostname(&server.name);
        let vs_slug = slugify_server_name(&server.name);
        if (!vs_name_norm.is_empty() && vs_name_norm == hostname)
            || (!vs_slug.is_empty() && vs_slug == hostname)
        {
            return Some(format_virtual_server_target(default_ip, server.port()));
        }
    }

    None
}

pub fn list_tsdns_rules(runtime: &BaselineRuntime) -> Vec<TsdnsRule> {
    let mut rules: Vec<TsdnsRule> = runtime
        .store
        .instance_properties
        .iter()
        .filter_map(|(key, val)| {
            key.strip_prefix(TSDNS_RULE_PREFIX).map(|domain| TsdnsRule {
                domain_pattern: domain.to_string(),
                target: val.clone(),
            })
        })
        .collect();
    rules.sort_by(|a, b| a.domain_pattern.cmp(&b.domain_pattern));
    rules
}

pub fn normalize_hostname(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('.');
    // If someone passed host:port in query, strip the port part unless IPv6
    let host_part = if !trimmed.starts_with('[') {
        trimmed.split(':').next().unwrap_or(trimmed)
    } else {
        trimmed
    };
    host_part.trim().to_ascii_lowercase()
}

fn slugify_server_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn format_virtual_server_target(ip_prop: Option<&str>, port: u16) -> String {
    let ip = ip_prop
        .map(str::trim)
        .filter(|ip| !ip.is_empty() && *ip != "0.0.0.0" && *ip != "::")
        .unwrap_or("127.0.0.1");
    format!("{ip}:{port}")
}

fn wildcard_matches(pattern: &str, hostname: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        // "*.example.com" matches "sub.example.com" and "example.com"
        hostname == suffix || hostname.ends_with(&format!(".{suffix}"))
    } else if let Some(prefix) = pattern.strip_suffix(".*") {
        hostname.starts_with(&format!("{prefix}."))
    } else {
        pattern == hostname
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_matching_supports_subdomains_and_catchall() {
        assert!(wildcard_matches("*.example.com", "voice.example.com"));
        assert!(wildcard_matches("*.example.com", "eu.voice.example.com"));
        assert!(wildcard_matches("*.example.com", "example.com"));
        assert!(!wildcard_matches("*.example.com", "otherdomain.com"));
        assert!(wildcard_matches("*", "anything.local"));
    }
}
