use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub const DEFAULT_TSDNS_PORT: u16 = 41144;
pub const DEFAULT_VOICE_PORT: u16 = 9987;

/// Resolves a server address using TeamSpeak 3 TSDNS protocol (TCP port 41144) if no explicit
/// `:port` is present in `raw_address`.
///
/// Resolution order when no `:port` is given:
/// 1. Query `<hostname>:41144` with `<hostname>\r\n` (800ms timeout)
/// 2. If `<hostname>` has 3+ labels (e.g. `sub.clan.de`), also try parent `<clan.de>:41144`
/// 3. Fallback to `<hostname>:9987`
pub async fn resolve_server_address_with_tsdns(raw_address: &str) -> String {
    let trimmed = raw_address.trim();
    if trimmed.is_empty() {
        return format!("127.0.0.1:{DEFAULT_VOICE_PORT}");
    }

    // Check if an explicit port is already specified (IPv4/domain `host:port` or IPv6 `[::1]:port`)
    if has_explicit_port(trimmed) {
        return trimmed.to_string();
    }

    let hostname = trimmed.trim_end_matches('.');

    // Skip TSDNS lookup for localhost or raw IPv4/IPv6 literals to avoid unnecessary 800ms wait
    if !is_ip_or_localhost(hostname) {
        if let Some(resolved) = query_tsdns_with_hierarchy(hostname, DEFAULT_TSDNS_PORT).await {
            println!("TSDNS resolved '{}' -> '{}'", hostname, resolved);
            return resolved;
        }
    }

    format!("{hostname}:{DEFAULT_VOICE_PORT}")
}

/// Queries a specific TSDNS server (`tsdns_server_addr`, e.g. `127.0.0.1:41144`) for `hostname`.
pub async fn query_tsdns_server(tsdns_server_addr: &str, hostname: &str) -> Option<String> {
    let connect_fut = TcpStream::connect(tsdns_server_addr);
    let mut stream = tokio::time::timeout(Duration::from_millis(800), connect_fut)
        .await
        .ok()?
        .ok()?;

    let payload = format!("{}\r\n", hostname.trim());
    tokio::time::timeout(Duration::from_millis(800), stream.write_all(payload.as_bytes()))
        .await
        .ok()?
        .ok()?;
    let _ = stream.flush().await;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes_read = tokio::time::timeout(Duration::from_millis(800), reader.read_line(&mut line))
        .await
        .ok()?
        .ok()?;
    if bytes_read == 0 {
        return None;
    }

    parse_tsdns_response(&line)
}

async fn query_tsdns_with_hierarchy(hostname: &str, tsdns_port: u16) -> Option<String> {
    let candidates = build_tsdns_host_candidates(hostname);
    for candidate_host in candidates {
        let server_addr = format!("{candidate_host}:{tsdns_port}");
        if let Some(target) = query_tsdns_server(&server_addr, hostname).await {
            return Some(target);
        }
    }
    None
}

pub fn build_tsdns_host_candidates(hostname: &str) -> Vec<String> {
    let clean = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = clean.split('.').filter(|s| !s.is_empty()).collect();
    let mut candidates = Vec::new();
    if labels.is_empty() {
        return candidates;
    }
    candidates.push(clean.clone());
    // Walk up subdomain hierarchy down to 2-label base domain (e.g. a.b.example.com -> b.example.com -> example.com)
    for start_idx in 1..labels.len().saturating_sub(1) {
        candidates.push(labels[start_idx..].join("."));
    }
    candidates
}

pub fn parse_tsdns_response(raw_line: &str) -> Option<String> {
    let trimmed = raw_line.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("404")
        || trimmed.eq_ignore_ascii_case("NORESPONSE")
    {
        return None;
    }
    // Response may be "<ip>:<port>" or "<ip>"
    if has_explicit_port(trimmed) {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}:{DEFAULT_VOICE_PORT}"))
    }
}

pub fn has_explicit_port(addr: &str) -> bool {
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some((_, after_bracket)) = rest.split_once(']') {
            return after_bracket
                .strip_prefix(':')
                .and_then(|p| p.parse::<u16>().ok())
                .is_some();
        }
        return false;
    }
    if let Some((_, port_str)) = addr.rsplit_once(':') {
        // Ensure it's not an unbracketed IPv6 literal with multiple colons
        if addr.matches(':').count() == 1 {
            return port_str.parse::<u16>().is_ok();
        }
    }
    false
}

fn is_ip_or_localhost(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    if lower == "localhost" || lower.starts_with('[') {
        return true;
    }
    host.parse::<std::net::IpAddr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_explicit_ports_and_builds_candidates() {
        assert!(has_explicit_port("127.0.0.1:9987"));
        assert!(has_explicit_port("voice.clan.de:9988"));
        assert!(has_explicit_port("[::1]:9987"));
        assert!(!has_explicit_port("voice.clan.de"));
        assert!(!has_explicit_port("127.0.0.1"));

        let cands = build_tsdns_host_candidates("eu.voice.clan.de");
        assert_eq!(cands, vec!["eu.voice.clan.de", "voice.clan.de", "clan.de"]);

        assert_eq!(
            parse_tsdns_response("198.51.100.42:9987\r\n"),
            Some("198.51.100.42:9987".to_string())
        );
        assert_eq!(parse_tsdns_response("404\r\n"), None);
    }
}
