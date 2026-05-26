use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use super::*;
pub(crate) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = from_hex(bytes[index + 1]);
            let low = from_hex(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}
pub(crate) fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
pub(crate) fn resolve_list_area(cid: u32, path: &str) -> Result<(FileArea, String)> {
    let normalized_path = normalize_absolute_path(path)?;
    if cid != 0 {
        return Ok((FileArea::Channel(cid), normalized_path));
    }
    if normalized_path == "/icons" || normalized_path == "/icons/" {
        return Ok((FileArea::Icons, String::from("/")));
    }
    if normalized_path.starts_with("/icons/") {
        return Ok((
            FileArea::Icons,
            strip_global_prefix(&normalized_path, "/icons"),
        ));
    }
    if normalized_path.starts_with("/music/") {
        return Ok((
            FileArea::Music,
            strip_global_prefix(&normalized_path, "/music"),
        ));
    }
    Ok((FileArea::Avatars, normalized_path))
}
pub(crate) fn strip_global_prefix(path: &str, prefix: &str) -> String {
    let stripped = path.strip_prefix(prefix).unwrap_or(path);
    if stripped.is_empty() {
        String::from("/")
    } else {
        normalize_absolute_path(stripped).unwrap_or_else(|_| String::from("/"))
    }
}
pub(crate) fn join_virtual_path(base_path: &str, leaf: &str) -> Result<String> {
    let leaf = leaf.replace('\\', "/");
    if leaf.starts_with('/') {
        return normalize_absolute_path(&leaf);
    }

    let mut segments = path_segments(&normalize_absolute_path(base_path)?);
    for segment in leaf.split('/') {
        if segment.is_empty() {
            continue;
        }
        validate_path_segment(segment)?;
        segments.push(segment.to_string());
    }
    Ok(build_absolute_path(&segments))
}
pub(crate) fn normalize_absolute_path(path: &str) -> Result<String> {
    let path = path.replace('\\', "/");
    if path.is_empty() || path == "/" {
        return Ok(String::from("/"));
    }
    let mut segments = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        validate_path_segment(segment)?;
        segments.push(segment.to_string());
    }
    Ok(build_absolute_path(&segments))
}
pub(crate) fn validate_path_segment(segment: &str) -> Result<()> {
    if segment == "." || segment == ".." {
        return Err(anyhow!("path traversal is not allowed"));
    }
    if segment.contains(':') {
        return Err(anyhow!("invalid path segment"));
    }
    Ok(())
}
pub(crate) fn path_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
        .collect()
}
pub(crate) fn build_absolute_path(segments: &[String]) -> String {
    if segments.is_empty() {
        String::from("/")
    } else {
        format!("/{}", segments.join("/"))
    }
}
pub(crate) fn build_entry_info(request_path: &str, full_path: &Path) -> io::Result<FileEntryInfo> {
    let metadata = fs::metadata(full_path)?;
    let name = full_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let datetime = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let entry_type = if metadata.is_dir() { 0 } else { 1 };
    let empty = if metadata.is_dir() {
        fs::read_dir(full_path)?.next().is_none()
    } else {
        false
    };

    Ok(FileEntryInfo {
        path: request_path.to_string(),
        name,
        size: if metadata.is_dir() { 0 } else { metadata.len() },
        datetime,
        entry_type,
        empty,
    })
}
pub(crate) fn media_bytes_header(file: &mut File) -> io::Result<String> {
    let mut bytes = [0_u8; 10];
    let read = file.read(&mut bytes)?;
    Ok(BASE64_STANDARD.encode(&bytes[..read]))
}
pub(crate) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
pub(crate) fn map_io_error(error: io::Error) -> FileTransferError {
    match error.kind() {
        io::ErrorKind::NotFound => FileTransferError::NotFound,
        io::ErrorKind::AlreadyExists => FileTransferError::AlreadyExists,
        _ => FileTransferError::Io,
    }
}
pub(crate) fn load_tls_config(certificate_path: &Path, private_key_path: &Path) -> Result<Arc<ServerConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let certificate_file = File::open(certificate_path)
        .with_context(|| format!("failed to open certificate {}", certificate_path.display()))?;
    let private_key_file = File::open(private_key_path)
        .with_context(|| format!("failed to open private key {}", private_key_path.display()))?;

    let mut cert_reader = BufReader::new(certificate_file);
    let certificates = rustls_pemfile::certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificate {}", certificate_path.display()))?;
    if certificates.is_empty() {
        return Err(anyhow!("certificate file {} contains no PEM certificates", certificate_path.display()));
    }

    let mut key_reader = BufReader::new(private_key_file);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .with_context(|| format!("failed to parse private key {}", private_key_path.display()))?
        .ok_or_else(|| anyhow!("private key file {} contains no supported private key", private_key_path.display()))?;

    let private_key: PrivateKeyDer<'static> = match private_key {
        PrivateKeyDer::Pkcs8(key) => PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.secret_pkcs8_der().to_vec())),
        other => other.clone_key(),
    };

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .context("failed to assemble rustls server config")?;

    Ok(Arc::new(config))
}
