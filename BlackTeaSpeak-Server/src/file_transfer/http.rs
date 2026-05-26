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
pub(crate) const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";
pub(crate) const HTTP_HEADER_LIMIT: usize = 1024 * 1024;
pub(crate) const RESPONSE_COPY_BUFFER_SIZE: usize = 64 * 1024;
pub(crate) fn write_download_response(
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    request: &ParsedHttpRequest,
    transfer: &PendingTransfer,
) -> Result<u64> {
    let mut file = File::open(&transfer.file_path).context("failed to open download file")?;
    let metadata = file.metadata().context("failed to stat download file")?;
    if !metadata.is_file() {
        return write_simple_response(stream, 404, "Not Found", &[], b"file not found")
            .map(|_| 0);
    }
    if transfer.seek_position > metadata.len() {
        return write_simple_response(stream, 400, "Bad Request", &[], b"invalid seek position")
            .map(|_| 0);
    }

    let content_length = metadata.len().saturating_sub(transfer.seek_position);
    let download_name = requested_download_name(request, transfer);
    let content_type = guess_content_type(&download_name);
    let content_disposition = content_disposition_attachment(&download_name);
    file.seek(SeekFrom::Start(transfer.seek_position))
        .context("failed to seek download file")?;
    let media_header = media_bytes_header(&mut file).context("failed to inspect media bytes")?;
    file.seek(SeekFrom::Start(transfer.seek_position))
        .context("failed to reset download file offset")?;

    write_response_headers(
        stream,
        200,
        "OK",
        &[
            ("Content-Type", content_type),
            ("Content-Length", content_length.to_string()),
            ("Content-Disposition", content_disposition),
            ("X-media-bytes", media_header),
        ],
    )?;

    let mut buffer = [0_u8; RESPONSE_COPY_BUFFER_SIZE];
    let mut bytes_transferred = 0_u64;
    loop {
        let read = file.read(&mut buffer).context("failed to read download file")?;
        if read == 0 {
            break;
        }
        bytes_transferred = bytes_transferred.saturating_add(read as u64);
        stream
            .write_all(&buffer[..read])
            .context("failed to write download response body")?;
    }
    stream.flush().context("failed to flush download response")?;
    Ok(bytes_transferred)
}
pub(crate) fn store_upload_payload(request: &ParsedHttpRequest, transfer: &PendingTransfer) -> Result<u64> {
    let content_type = request
        .headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or_default();
    let payload = extract_upload_payload(content_type, &request.body)
        .map_err(|_| anyhow!("invalid upload payload"))?;

    if let Some(parent) = transfer.file_path.parent() {
        fs::create_dir_all(parent).context("failed to create upload target directory")?;
    }
    let mut file = File::create(&transfer.file_path).context("failed to create upload file")?;
    file.write_all(&payload)
        .context("failed to write upload file")?;
    file.flush().context("failed to flush upload file")?;

    Ok(payload.len() as u64)
}
pub(crate) fn write_simple_response(
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    status: u16,
    reason: &str,
    extra_headers: &[(&str, String)],
    body: &[u8],
) -> Result<()> {
    let mut headers = vec![
        ("Content-Type", "text/plain; charset=utf-8".to_string()),
        ("Content-Length", body.len().to_string()),
    ];
    headers.extend(extra_headers.iter().map(|(key, value)| (*key, value.clone())));
    write_response_headers(stream, status, reason, &headers)?;
    stream
        .write_all(body)
        .context("failed to write HTTP response body")?;
    stream.flush().context("failed to flush HTTP response")?;
    Ok(())
}
pub(crate) fn write_response_headers(
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    status: u16,
    reason: &str,
    extra_headers: &[(&str, String)],
) -> Result<()> {
    let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
    response.push_str("Connection: close\r\n");
    response.push_str("Access-Control-Allow-Origin: *\r\n");
    response.push_str("Access-Control-Allow-Headers: *\r\n");
    response.push_str("Access-Control-Expose-Headers: *\r\n");
    response.push_str("Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n");
    for (key, value) in extra_headers {
        response.push_str(key);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    stream
        .write_all(response.as_bytes())
        .context("failed to write HTTP response headers")?;
    Ok(())
}
pub(crate) fn read_http_request(stream: &mut StreamOwned<ServerConnection, TcpStream>) -> Result<ParsedHttpRequest> {
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut scratch).context("failed to read HTTP request")?;
        if read == 0 {
            return Err(anyhow!("unexpected end of stream while reading HTTP headers"));
        }
        buffer.extend_from_slice(&scratch[..read]);
        if buffer.len() > HTTP_HEADER_LIMIT {
            return Err(anyhow!("HTTP request headers exceed limit"));
        }
        if let Some(position) = find_subslice(&buffer, HEADER_TERMINATOR) {
            break position + HEADER_TERMINATOR.len();
        }
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.split("\r\n").filter(|line| !line.is_empty());
    let request_line = lines.next().ok_or_else(|| anyhow!("missing HTTP request line"))?;
    let mut request_line_parts = request_line.split_whitespace();
    let method = request_line_parts
        .next()
        .ok_or_else(|| anyhow!("missing HTTP method"))?
        .to_string();
    let target = request_line_parts
        .next()
        .ok_or_else(|| anyhow!("missing HTTP request target"))?
        .to_string();

    let mut headers = HashMap::new();
    for line in lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut scratch).context("failed to read HTTP request body")?;
        if read == 0 {
            return Err(anyhow!("unexpected end of stream while reading HTTP body"));
        }
        buffer.extend_from_slice(&scratch[..read]);
    }

    Ok(ParsedHttpRequest {
        method,
        target,
        headers,
        body: buffer[header_end..header_end + content_length].to_vec(),
    })
}
pub(crate) fn request_transfer_key(request: &ParsedHttpRequest) -> Option<String> {
    extract_query_parameter(&request.target, "transfer-key")
        .or_else(|| request.headers.get("transfer-key").cloned())
}
pub(crate) fn requested_download_name(request: &ParsedHttpRequest, transfer: &PendingTransfer) -> String {
    extract_query_parameter(&request.target, "file-name")
        .and_then(|value| sanitize_download_name(&value))
        .or_else(|| {
            transfer
                .file_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .and_then(|value| sanitize_download_name(&value))
        })
        .unwrap_or_else(|| String::from("download"))
}
pub(crate) fn sanitize_download_name(value: &str) -> Option<String> {
    let candidate = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('.');
    if candidate.is_empty() {
        return None;
    }

    let sanitized: String = candidate
        .chars()
        .filter(|character| *character != '\r' && *character != '\n')
        .collect();
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}
pub(crate) fn guess_content_type(file_name: &str) -> String {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("txt") | Some("log") => String::from("text/plain; charset=utf-8"),
        Some("md") => String::from("text/markdown; charset=utf-8"),
        Some("html") | Some("htm") => String::from("text/html; charset=utf-8"),
        Some("css") => String::from("text/css; charset=utf-8"),
        Some("js") | Some("mjs") => String::from("application/javascript; charset=utf-8"),
        Some("json") => String::from("application/json; charset=utf-8"),
        Some("svg") => String::from("image/svg+xml"),
        Some("png") => String::from("image/png"),
        Some("jpg") | Some("jpeg") => String::from("image/jpeg"),
        Some("gif") => String::from("image/gif"),
        Some("webp") => String::from("image/webp"),
        Some("ico") => String::from("image/x-icon"),
        Some("pdf") => String::from("application/pdf"),
        Some("zip") => String::from("application/zip"),
        Some("mp3") => String::from("audio/mpeg"),
        Some("ogg") => String::from("audio/ogg"),
        Some("wav") => String::from("audio/wav"),
        Some("flac") => String::from("audio/flac"),
        Some("mp4") => String::from("video/mp4"),
        Some("webm") => String::from("video/webm"),
        Some("mov") => String::from("video/quicktime"),
        Some("avi") => String::from("video/x-msvideo"),
        _ => String::from("application/octet-stream"),
    }
}
pub(crate) fn content_disposition_attachment(file_name: &str) -> String {
    let fallback = ascii_fallback_file_name(file_name);
    let encoded = rfc5987_percent_encode(file_name);
    format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    )
}
pub(crate) fn ascii_fallback_file_name(file_name: &str) -> String {
    let fallback: String = file_name
        .chars()
        .map(|character| match character {
            '"' | '\\' | '/' | '\r' | '\n' => '_',
            character if character.is_ascii() && !character.is_ascii_control() => character,
            _ => '_',
        })
        .collect();
    let fallback = fallback.trim().trim_matches('.');
    if fallback.is_empty() {
        String::from("download")
    } else {
        fallback.to_string()
    }
}
pub(crate) fn rfc5987_percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~' => encoded.push(*byte as char),
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}
pub(crate) fn extract_query_parameter(target: &str, key: &str) -> Option<String> {
    let (_, query) = target.split_once('?')?;
    query.split('&').find_map(|segment| {
        let (current_key, current_value) = segment.split_once('=')?;
        (current_key == key).then(|| percent_decode(current_value))
    })
}
pub(crate) fn extract_upload_payload(content_type: &str, body: &[u8]) -> Result<Vec<u8>> {
    let boundary = content_type
        .split(';')
        .find_map(|segment| segment.trim().strip_prefix("boundary="))
        .ok_or_else(|| anyhow!("missing multipart boundary"))?;
    let boundary = boundary.trim_matches('"');
    let delimiter = format!("--{boundary}").into_bytes();
    if !body.starts_with(&delimiter) {
        return Err(anyhow!("invalid multipart boundary prefix"));
    }
    let data_start = find_subslice(body, HEADER_TERMINATOR)
        .map(|position| position + HEADER_TERMINATOR.len())
        .ok_or_else(|| anyhow!("missing multipart payload header terminator"))?;
    let data_end_marker = format!("\r\n--{boundary}").into_bytes();
    let data_end = find_subslice(&body[data_start..], &data_end_marker)
        .map(|position| data_start + position)
        .ok_or_else(|| anyhow!("missing multipart payload terminator"))?;
    Ok(body[data_start..data_end].to_vec())
}
