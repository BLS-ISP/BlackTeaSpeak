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

pub const DEFAULT_FILE_TRANSFER_BIND: &str = "127.0.0.1:30303";
pub const FILE_TRANSFER_STATUS_COMPLETE: u32 = 0x811;

const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";
const HTTP_HEADER_LIMIT: usize = 1024 * 1024;
const RESPONSE_COPY_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone)]
pub struct PreparedFileTransfer {
    pub transfer_key: String,
    pub server_transfer_id: u64,
    pub direction: FileTransferDirection,
    pub port: u16,
    pub ip: Option<String>,
    pub seek_position: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct FileEntryInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub datetime: u64,
    pub entry_type: u32,
    pub empty: bool,
}

#[derive(Debug, Clone)]
pub enum FileTransferEvent {
    Started {
        client_id: u64,
        client_transfer_id: String,
    },
    Progress {
        client_id: u64,
        client_transfer_id: String,
        file_bytes_transferred: u64,
        file_current_offset: u64,
        file_start_offset: u64,
        file_total_size: u64,
        network_bytes_received: u64,
        network_bytes_send: u64,
        network_current_speed: u64,
        network_average_speed: u64,
    },
    Status {
        client_id: u64,
        client_transfer_id: String,
        status: u32,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTransferError {
    NotFound,
    AlreadyExists,
    InvalidPath,
    InvalidPayload,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileArea {
    Channel(u32),
    Icons,
    Avatars,
    Music,
}

#[derive(Debug, Clone)]
struct PendingTransfer {
    server_transfer_id: u64,
    direction: FileTransferDirection,
    file_path: PathBuf,
    seek_position: u64,
    size: u64,
    notify_client_events: bool,
    client_id: Option<u64>,
    client_transfer_id: Option<String>,
}

impl PendingTransfer {
    fn should_emit_client_events(&self) -> bool {
        self.notify_client_events
            || (self.direction == FileTransferDirection::Upload
                && self.client_id.is_some()
                && self.client_transfer_id.is_some())
    }
}

type FileTransferNotifier = Arc<dyn Fn(&FileTransferEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone)]
struct FileTransferEndpoint {
    ip: Option<String>,
    port: u16,
}

#[derive(Debug, Clone)]
struct ParsedHttpRequest {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

pub struct FileTransferRegistry {
    repository_root: PathBuf,
    next_server_transfer_id: AtomicU64,
    transfers: Mutex<HashMap<String, PendingTransfer>>,
    endpoint: Mutex<FileTransferEndpoint>,
    notifiers: Mutex<Vec<FileTransferNotifier>>,
}

pub struct FileTransferServer {
    listener: TcpListener,
    tls_config: Arc<ServerConfig>,
    shutdown: Arc<AtomicBool>,
    registry: Arc<FileTransferRegistry>,
}

impl std::fmt::Debug for FileTransferRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let endpoint = self.endpoint.lock().ok().map(|endpoint| endpoint.clone());
        let has_notifiers = self
            .notifiers
            .lock()
            .ok()
            .map(|notifiers| !notifiers.is_empty())
            .unwrap_or(false);

        formatter
            .debug_struct("FileTransferRegistry")
            .field("repository_root", &self.repository_root)
            .field(
                "next_server_transfer_id",
                &self.next_server_transfer_id.load(Ordering::SeqCst),
            )
            .field("endpoint", &endpoint)
            .field("has_notifiers", &has_notifiers)
            .finish()
    }
}

impl FileTransferRegistry {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let port = DEFAULT_FILE_TRANSFER_BIND
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(30303);
        Self {
            repository_root: workspace_root
                .as_ref()
                .join("BlackTeaSpeak-Server")
                .join("data")
                .join("file-repositories"),
            next_server_transfer_id: AtomicU64::new(1),
            transfers: Mutex::new(HashMap::new()),
            endpoint: Mutex::new(FileTransferEndpoint { ip: None, port }),
            notifiers: Mutex::new(Vec::new()),
        }
    }

    pub fn add_notifier(&self, notifier: FileTransferNotifier) {
        if let Ok(mut sink) = self.notifiers.lock() {
            sink.push(notifier);
        }
    }

    pub fn set_public_endpoint(&self, local_addr: SocketAddr) {
        let ip = if local_addr.ip().is_unspecified() {
            None
        } else {
            Some(local_addr.ip().to_string())
        };

        if let Ok(mut endpoint) = self.endpoint.lock() {
            endpoint.ip = ip;
            endpoint.port = local_addr.port();
        }
    }

    pub fn music_download_path(&self, filename: &str) -> io::Result<PathBuf> {
        self.materialize_named_path(FileArea::Music, filename, true)
    }

    pub fn list_entries(&self, cid: u32, path: &str) -> Result<Vec<FileEntryInfo>, FileTransferError> {
        let (area, relative_path) = resolve_list_area(cid, path).map_err(|_| FileTransferError::InvalidPath)?;
        let directory_path = self.materialize_directory_path(area, &relative_path, relative_path == "/")
            .map_err(map_io_error)?;

        if !directory_path.exists() {
            return Err(FileTransferError::NotFound);
        }

        let mut entries = fs::read_dir(&directory_path)
            .map_err(map_io_error)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| build_entry_info(&relative_path, &entry.path()).ok())
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            (left.entry_type, left.name.to_lowercase()).cmp(&(right.entry_type, right.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub fn stat_entry(&self, cid: u32, name: &str, actor_avatar_id: Option<&str>) -> Result<FileEntryInfo, FileTransferError> {
        let (area, relative_path) = self
            .resolve_named_path(cid, "/", name, actor_avatar_id)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let full_path = self
            .materialize_named_path(area, &relative_path, relative_path == "/")
            .map_err(map_io_error)?;
        if !full_path.exists() {
            return Err(FileTransferError::NotFound);
        }
        build_entry_info(&relative_path, &full_path).map_err(map_io_error)
    }

    pub fn create_directory(&self, cid: u32, dirname: &str) -> Result<(), FileTransferError> {
        let (area, relative_path) = self
            .resolve_named_path(cid, "/", dirname, None)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let directory_path = self
            .materialize_named_path(area, &relative_path, false)
            .map_err(map_io_error)?;
        if directory_path.exists() {
            return Err(FileTransferError::AlreadyExists);
        }
        fs::create_dir_all(&directory_path).map_err(map_io_error)
    }

    pub fn delete_entry(
        &self,
        cid: u32,
        path: &str,
        name: &str,
        actor_avatar_id: Option<&str>,
    ) -> Result<(), FileTransferError> {
        let (area, relative_path) = self
            .resolve_named_path(cid, path, name, actor_avatar_id)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let full_path = self
            .materialize_named_path(area, &relative_path, false)
            .map_err(map_io_error)?;
        if !full_path.exists() {
            return Err(FileTransferError::NotFound);
        }
        if full_path.is_dir() {
            fs::remove_dir_all(full_path).map_err(map_io_error)
        } else {
            fs::remove_file(full_path).map_err(map_io_error)
        }
    }

    pub fn rename_entry(
        &self,
        source_cid: u32,
        oldname: &str,
        target_cid: u32,
        newname: &str,
    ) -> Result<(), FileTransferError> {
        let (source_area, source_relative_path) = self
            .resolve_named_path(source_cid, "/", oldname, None)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let (target_area, target_relative_path) = self
            .resolve_named_path(target_cid, "/", newname, None)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let source_path = self
            .materialize_named_path(source_area, &source_relative_path, false)
            .map_err(map_io_error)?;
        let target_path = self
            .materialize_named_path(target_area, &target_relative_path, false)
            .map_err(map_io_error)?;

        if !source_path.exists() {
            return Err(FileTransferError::NotFound);
        }
        if target_path.exists() {
            return Err(FileTransferError::AlreadyExists);
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(map_io_error)?;
        }
        fs::rename(source_path, target_path).map_err(map_io_error)
    }

    pub fn prepare_download(
        &self,
        cid: u32,
        path: &str,
        name: &str,
        seek_position: u64,
        notify_client_events: bool,
        client_id: Option<u64>,
        client_transfer_id: Option<&str>,
        actor_avatar_id: Option<&str>,
    ) -> Result<PreparedFileTransfer, FileTransferError> {
        let (area, relative_path) = self
            .resolve_named_path(cid, path, name, actor_avatar_id)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let file_path = self
            .materialize_named_path(area, &relative_path, false)
            .map_err(map_io_error)?;
        let metadata = fs::metadata(&file_path).map_err(map_io_error)?;
        if !metadata.is_file() {
            return Err(FileTransferError::NotFound);
        }
        let total_size = metadata.len();
        if seek_position > total_size {
            return Err(FileTransferError::InvalidPayload);
        }

        let prepared = self.register_transfer(PendingTransfer {
            server_transfer_id: self.next_server_transfer_id.fetch_add(1, Ordering::SeqCst),
            direction: FileTransferDirection::Download,
            file_path,
            seek_position,
            size: total_size.saturating_sub(seek_position),
            notify_client_events,
            client_id,
            client_transfer_id: client_transfer_id.map(str::to_string),
        });
        Ok(prepared)
    }

    pub fn prepare_upload(
        &self,
        cid: u32,
        path: &str,
        name: &str,
        size: u64,
        overwrite: bool,
        notify_client_events: bool,
        client_id: Option<u64>,
        client_transfer_id: Option<&str>,
        actor_avatar_id: Option<&str>,
    ) -> Result<PreparedFileTransfer, FileTransferError> {
        let (area, relative_path) = self
            .resolve_named_path(cid, path, name, actor_avatar_id)
            .map_err(|_| FileTransferError::InvalidPath)?;
        let file_path = self
            .materialize_named_path(area, &relative_path, false)
            .map_err(map_io_error)?;
        if file_path.exists() && !overwrite {
            return Err(FileTransferError::AlreadyExists);
        }
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(map_io_error)?;
        }

        let prepared = self.register_transfer(PendingTransfer {
            server_transfer_id: self.next_server_transfer_id.fetch_add(1, Ordering::SeqCst),
            direction: FileTransferDirection::Upload,
            file_path,
            seek_position: 0,
            size,
            notify_client_events,
            client_id,
            client_transfer_id: client_transfer_id.map(str::to_string),
        });
        Ok(prepared)
    }

    fn register_transfer(&self, transfer: PendingTransfer) -> PreparedFileTransfer {
        let transfer_key = BASE64_STANDARD.encode(format!(
            "blackteaspeak-ft-{}-{}",
            transfer.server_transfer_id,
            transfer.size
        ));
        let endpoint = self
            .endpoint
            .lock()
            .map(|endpoint| endpoint.clone())
            .unwrap_or(FileTransferEndpoint { ip: None, port: 30303 });

        self.transfers
            .lock()
            .expect("file transfer registry lock poisoned")
            .insert(transfer_key.clone(), transfer.clone());

        PreparedFileTransfer {
            transfer_key,
            server_transfer_id: transfer.server_transfer_id,
            direction: transfer.direction,
            port: endpoint.port,
            ip: endpoint.ip,
            seek_position: transfer.seek_position,
            size: transfer.size,
        }
    }

    fn resolve_named_path(
        &self,
        cid: u32,
        path: &str,
        name: &str,
        actor_avatar_id: Option<&str>,
    ) -> Result<(FileArea, String)> {
        if cid == 0 {
            let normalized_name = normalize_absolute_path(name)?;
            if normalized_name == "/avatar" || normalized_name == "/avatar_" {
                let Some(actor_avatar_id) = actor_avatar_id else {
                    return Err(anyhow!("missing actor avatar id"));
                };
                return Ok((FileArea::Avatars, format!("/avatar_{actor_avatar_id}")));
            }
            if normalized_name.starts_with("/avatar_") {
                return Ok((FileArea::Avatars, normalized_name));
            }
            if normalized_name.starts_with("/icon_") {
                return Ok((FileArea::Icons, normalized_name));
            }
            if normalized_name.starts_with("/music_") {
                return Ok((FileArea::Music, normalized_name));
            }

            let (area, directory_path) = resolve_list_area(cid, path)?;
            let joined_path = join_virtual_path(&directory_path, name)?;
            let relative_path = if area == FileArea::Icons && joined_path.starts_with("/icons/") {
                strip_global_prefix(&joined_path, "/icons")
            } else {
                joined_path
            };
            return Ok((area, relative_path));
        }

        Ok((FileArea::Channel(cid), join_virtual_path(path, name)?))
    }

    fn materialize_directory_path(
        &self,
        area: FileArea,
        relative_path: &str,
        create_if_missing: bool,
    ) -> io::Result<PathBuf> {
        let full_path = self.area_root(area).join(relative_path.trim_start_matches('/'));
        if create_if_missing {
            fs::create_dir_all(&full_path)?;
        }
        Ok(full_path)
    }

    fn materialize_named_path(
        &self,
        area: FileArea,
        relative_path: &str,
        create_if_missing: bool,
    ) -> io::Result<PathBuf> {
        let root = self.area_root(area);
        if create_if_missing {
            fs::create_dir_all(&root)?;
        } else if !root.exists() {
            fs::create_dir_all(&root)?;
        }
        Ok(root.join(relative_path.trim_start_matches('/')))
    }

    fn area_root(&self, area: FileArea) -> PathBuf {
        match area {
            FileArea::Channel(channel_id) => self
                .repository_root
                .join("channels")
                .join(channel_id.to_string()),
            FileArea::Icons => self.repository_root.join("global").join("icons"),
            FileArea::Avatars => self.repository_root.join("global").join("avatars"),
            FileArea::Music => self.repository_root.join("global").join("music"),
        }
    }

    fn take_transfer(&self, transfer_key: &str) -> Option<PendingTransfer> {
        self.transfers
            .lock()
            .ok()
            .and_then(|mut transfers| transfers.remove(transfer_key))
    }

    fn emit_started(&self, transfer: &PendingTransfer) {
        if !transfer.should_emit_client_events() {
            return;
        }
        let (Some(client_id), Some(client_transfer_id)) =
            (transfer.client_id, transfer.client_transfer_id.as_ref())
        else {
            return;
        };
        self.emit_event(FileTransferEvent::Started {
            client_id,
            client_transfer_id: client_transfer_id.clone(),
        });
    }

    fn emit_progress(&self, transfer: &PendingTransfer, bytes_transferred: u64) {
        if !transfer.should_emit_client_events() {
            return;
        }
        let (Some(client_id), Some(client_transfer_id)) =
            (transfer.client_id, transfer.client_transfer_id.as_ref())
        else {
            return;
        };
        let file_start_offset = transfer.seek_position;
        let file_current_offset = transfer.seek_position.saturating_add(bytes_transferred);
        let file_total_size = transfer.seek_position.saturating_add(transfer.size);
        self.emit_event(FileTransferEvent::Progress {
            client_id,
            client_transfer_id: client_transfer_id.clone(),
            file_bytes_transferred: bytes_transferred,
            file_current_offset,
            file_start_offset,
            file_total_size,
            network_bytes_received: bytes_transferred,
            network_bytes_send: bytes_transferred,
            network_current_speed: 0,
            network_average_speed: 0,
        });
    }

    fn emit_status(&self, transfer: &PendingTransfer, status: u32, message: impl Into<String>) {
        if !transfer.should_emit_client_events() {
            return;
        }
        let (Some(client_id), Some(client_transfer_id)) =
            (transfer.client_id, transfer.client_transfer_id.as_ref())
        else {
            return;
        };
        self.emit_event(FileTransferEvent::Status {
            client_id,
            client_transfer_id: client_transfer_id.clone(),
            status,
            message: message.into(),
        });
    }

    fn emit_event(&self, event: FileTransferEvent) {
        if let Ok(notifiers) = self.notifiers.lock() {
            for notifier in notifiers.iter() {
                notifier(&event);
            }
        }
    }
}

impl FileTransferServer {
    pub fn bind(
        registry: Arc<FileTransferRegistry>,
        bind_addr: &str,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr)
            .with_context(|| format!("failed to bind file transfer server to {bind_addr}"))?;
        listener
            .set_nonblocking(true)
            .context("failed to switch file transfer listener to nonblocking mode")?;

        let server = Self {
            registry,
            listener,
            tls_config: load_tls_config(certificate_path.as_ref(), private_key_path.as_ref())?,
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        server.registry.set_public_endpoint(server.local_addr()?);
        Ok(server)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener
            .local_addr()
            .context("failed to read file transfer listener address")
    }

    pub fn run(self) -> Result<()> {
        while !self.shutdown.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let tls_config = Arc::clone(&self.tls_config);
                    let registry = Arc::clone(&self.registry);
                    thread::spawn(move || {
                        if let Err(error) = handle_transfer_client(stream, tls_config, registry) {
                            eprintln!("file transfer client error: {error:#}");
                        }
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error).context("file transfer listener accept failed"),
            }
        }

        Ok(())
    }
}

fn handle_transfer_client(
    stream: TcpStream,
    tls_config: Arc<ServerConfig>,
    registry: Arc<FileTransferRegistry>,
) -> Result<()> {
    stream
        .set_nonblocking(false)
        .context("failed to switch file transfer client socket to blocking mode")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .context("failed to set file transfer read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .context("failed to set file transfer write timeout")?;

    let mut tls_stream = StreamOwned::new(
        ServerConnection::new(tls_config).context("failed to create file transfer TLS server connection")?,
        stream,
    );
    let request = read_http_request(&mut tls_stream)?;

    if request.method.eq_ignore_ascii_case("OPTIONS") {
        return write_simple_response(&mut tls_stream, 204, "No Content", &[], &[]);
    }

    let transfer_key = request_transfer_key(&request)
        .ok_or_else(|| anyhow!("missing transfer key"))?;
    let Some(transfer) = registry.take_transfer(&transfer_key) else {
        return write_simple_response(&mut tls_stream, 404, "Not Found", &[], b"missing transfer");
    };

    match transfer.direction {
        FileTransferDirection::Download => {
            if request.method != "GET" && request.method != "POST" {
                return write_simple_response(
                    &mut tls_stream,
                    405,
                    "Method Not Allowed",
                    &[("Allow", "GET, POST, OPTIONS".to_string())],
                    &[],
                );
            }
            registry.emit_started(&transfer);
            match write_download_response(&mut tls_stream, &request, &transfer) {
                Ok(bytes_transferred) => {
                    registry.emit_progress(&transfer, bytes_transferred);
                    registry.emit_status(&transfer, FILE_TRANSFER_STATUS_COMPLETE, "ok");
                    Ok(())
                }
                Err(error) => {
                    registry.emit_status(&transfer, 1, error.to_string());
                    Err(error)
                }
            }
        }
        FileTransferDirection::Upload => {
            if request.method != "POST" {
                return write_simple_response(
                    &mut tls_stream,
                    405,
                    "Method Not Allowed",
                    &[("Allow", "POST, OPTIONS".to_string())],
                    &[],
                );
            }
            registry.emit_started(&transfer);
            match store_upload_payload(&request, &transfer) {
                Ok(bytes_transferred) => {
                    registry.emit_progress(&transfer, bytes_transferred);
                    registry.emit_status(&transfer, FILE_TRANSFER_STATUS_COMPLETE, "ok");
                    write_simple_response(&mut tls_stream, 200, "OK", &[], b"ok")?;
                    Ok(())
                }
                Err(error) => {
                    registry.emit_status(&transfer, 1, error.to_string());
                    Err(error)
                }
            }
        }
    }
}

fn write_download_response(
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

fn store_upload_payload(request: &ParsedHttpRequest, transfer: &PendingTransfer) -> Result<u64> {
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

fn write_simple_response(
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

fn write_response_headers(
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

fn read_http_request(stream: &mut StreamOwned<ServerConnection, TcpStream>) -> Result<ParsedHttpRequest> {
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

fn request_transfer_key(request: &ParsedHttpRequest) -> Option<String> {
    extract_query_parameter(&request.target, "transfer-key")
        .or_else(|| request.headers.get("transfer-key").cloned())
}

fn requested_download_name(request: &ParsedHttpRequest, transfer: &PendingTransfer) -> String {
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

fn sanitize_download_name(value: &str) -> Option<String> {
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

fn guess_content_type(file_name: &str) -> String {
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

fn content_disposition_attachment(file_name: &str) -> String {
    let fallback = ascii_fallback_file_name(file_name);
    let encoded = rfc5987_percent_encode(file_name);
    format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    )
}

fn ascii_fallback_file_name(file_name: &str) -> String {
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

fn rfc5987_percent_encode(value: &str) -> String {
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

fn extract_query_parameter(target: &str, key: &str) -> Option<String> {
    let (_, query) = target.split_once('?')?;
    query.split('&').find_map(|segment| {
        let (current_key, current_value) = segment.split_once('=')?;
        (current_key == key).then(|| percent_decode(current_value))
    })
}

fn percent_decode(value: &str) -> String {
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

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn extract_upload_payload(content_type: &str, body: &[u8]) -> Result<Vec<u8>> {
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

fn resolve_list_area(cid: u32, path: &str) -> Result<(FileArea, String)> {
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

fn strip_global_prefix(path: &str, prefix: &str) -> String {
    let stripped = path.strip_prefix(prefix).unwrap_or(path);
    if stripped.is_empty() {
        String::from("/")
    } else {
        normalize_absolute_path(stripped).unwrap_or_else(|_| String::from("/"))
    }
}

fn join_virtual_path(base_path: &str, leaf: &str) -> Result<String> {
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

fn normalize_absolute_path(path: &str) -> Result<String> {
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

fn validate_path_segment(segment: &str) -> Result<()> {
    if segment == "." || segment == ".." {
        return Err(anyhow!("path traversal is not allowed"));
    }
    if segment.contains(':') {
        return Err(anyhow!("invalid path segment"));
    }
    Ok(())
}

fn path_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
        .collect()
}

fn build_absolute_path(segments: &[String]) -> String {
    if segments.is_empty() {
        String::from("/")
    } else {
        format!("/{}", segments.join("/"))
    }
}

fn build_entry_info(request_path: &str, full_path: &Path) -> io::Result<FileEntryInfo> {
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

fn media_bytes_header(file: &mut File) -> io::Result<String> {
    let mut bytes = [0_u8; 10];
    let read = file.read(&mut bytes)?;
    Ok(BASE64_STANDARD.encode(&bytes[..read]))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn map_io_error(error: io::Error) -> FileTransferError {
    match error.kind() {
        io::ErrorKind::NotFound => FileTransferError::NotFound,
        io::ErrorKind::AlreadyExists => FileTransferError::AlreadyExists,
        _ => FileTransferError::Io,
    }
}

fn load_tls_config(certificate_path: &Path, private_key_path: &Path) -> Result<Arc<ServerConfig>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workspace_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "BlackTeaSpeak-Server-ft-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be monotonic enough for tests")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("temporary test directory should be creatable");
        root
    }

    #[test]
    fn registry_separates_channels_icons_and_avatars() {
        let workspace_root = test_workspace_root();
        let registry = FileTransferRegistry::new(&workspace_root);

        let avatar = registry
            .prepare_upload(0, "", "/avatar", 16, true, false, None, None, Some("avatarid"))
            .expect("avatar upload should be prepared");
        let icon = registry
            .prepare_upload(0, "", "/icon_42", 16, true, false, None, None, None)
            .expect("icon upload should be prepared");
        let channel = registry
            .prepare_upload(7, "/docs", "readme.txt", 16, true, false, None, None, None)
            .expect("channel upload should be prepared");

        let avatar_transfer = registry
            .take_transfer(&avatar.transfer_key)
            .expect("avatar transfer should be registered");
        let icon_transfer = registry
            .take_transfer(&icon.transfer_key)
            .expect("icon transfer should be registered");
        let channel_transfer = registry
            .take_transfer(&channel.transfer_key)
            .expect("channel transfer should be registered");

        assert!(avatar_transfer.file_path.ends_with(Path::new("global/avatars/avatar_avatarid")));
        assert!(icon_transfer.file_path.ends_with(Path::new("global/icons/icon_42")));
        assert!(channel_transfer.file_path.ends_with(Path::new("channels/7/docs/readme.txt")));

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn list_and_delete_channel_entries_roundtrip() {
        let workspace_root = test_workspace_root();
        let registry = FileTransferRegistry::new(&workspace_root);
        registry
            .create_directory(2, "/assets")
            .expect("channel directory should be created");

        let upload = registry
            .prepare_upload(2, "/assets", "logo.png", 4, true, false, None, None, None)
            .expect("channel upload should be prepared");
        let transfer = registry
            .take_transfer(&upload.transfer_key)
            .expect("prepared upload should be registered");
        if let Some(parent) = transfer.file_path.parent() {
            fs::create_dir_all(parent).expect("upload directory should be created");
        }
        fs::write(&transfer.file_path, [1_u8, 2, 3, 4]).expect("upload file should be writable");

        let entries = registry
            .list_entries(2, "/assets")
            .expect("channel directory should list entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "logo.png");
        assert_eq!(entries[0].entry_type, 1);

        registry
            .delete_entry(2, "/assets", "logo.png", None)
            .expect("channel file should delete");
        let entries = registry
            .list_entries(2, "/assets")
            .expect("channel directory should still exist after file deletion");
        assert!(entries.is_empty());

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn upload_events_are_emitted_without_notify_flag_for_web_client_context() {
        let workspace_root = test_workspace_root();
        let registry = FileTransferRegistry::new(&workspace_root);
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&events);
        registry.add_notifier(Arc::new(move |event| {
            captured_events
                .lock()
                .expect("event sink lock should not be poisoned")
                .push(event);
        }));

        let upload = registry
            .prepare_upload(0, "", "/avatar", 4, true, false, Some(42), Some("7"), Some("avatarid"))
            .expect("upload should be prepared");
        let transfer = registry
            .take_transfer(&upload.transfer_key)
            .expect("prepared upload should be registered");

        registry.emit_started(&transfer);
        registry.emit_progress(&transfer, 4);
        registry.emit_status(&transfer, FILE_TRANSFER_STATUS_COMPLETE, "ok");

        let events = events
            .lock()
            .expect("event sink lock should not be poisoned")
            .clone();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events.first(),
            Some(FileTransferEvent::Started {
                client_id,
                client_transfer_id,
            }) if *client_id == 42 && client_transfer_id == "7"
        ));
        assert!(matches!(
            events.get(1),
            Some(FileTransferEvent::Progress {
                client_id,
                client_transfer_id,
                file_bytes_transferred,
                file_current_offset,
                file_total_size,
                ..
            }) if *client_id == 42
                && client_transfer_id == "7"
                && *file_bytes_transferred == 4
                && *file_current_offset == 4
                && *file_total_size == 4
        ));
        assert!(matches!(
            events.get(2),
            Some(FileTransferEvent::Status {
                client_id,
                client_transfer_id,
                status,
                message,
            }) if *client_id == 42
                && client_transfer_id == "7"
                && *status == FILE_TRANSFER_STATUS_COMPLETE
                && message == "ok"
        ));

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn download_events_stay_suppressed_without_notify_flag() {
        let workspace_root = test_workspace_root();
        let registry = FileTransferRegistry::new(&workspace_root);
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&events);
        registry.add_notifier(Arc::new(move |event| {
            captured_events
                .lock()
                .expect("event sink lock should not be poisoned")
                .push(event);
        }));

        let file_path = workspace_root
            .join("BlackTeaSpeak-Server")
            .join("data")
            .join("file-repositories")
            .join("channels")
            .join("3")
            .join("readme.txt");
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("download test directory should be creatable");
        }
        fs::write(&file_path, b"test").expect("download test file should be writable");

        let download = registry
            .prepare_download(3, "", "readme.txt", 0, false, Some(42), Some("8"), None)
            .expect("download should be prepared");
        let transfer = registry
            .take_transfer(&download.transfer_key)
            .expect("prepared download should be registered");

        registry.emit_started(&transfer);
        registry.emit_progress(&transfer, 4);
        registry.emit_status(&transfer, FILE_TRANSFER_STATUS_COMPLETE, "ok");

        assert!(events
            .lock()
            .expect("event sink lock should not be poisoned")
            .is_empty());

        let _ = fs::remove_dir_all(workspace_root);
    }
}