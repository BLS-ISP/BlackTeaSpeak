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
pub struct FileTransferRegistry {
    pub(crate) repository_root: PathBuf,
    pub(crate) next_server_transfer_id: AtomicU64,
    pub(crate) transfers: Mutex<HashMap<String, PendingTransfer>>,
    pub(crate) endpoint: Mutex<FileTransferEndpoint>,
    pub(crate) notifiers: Mutex<Vec<FileTransferNotifier>>,
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

    pub(crate) fn register_transfer(&self, transfer: PendingTransfer) -> PreparedFileTransfer {
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

    pub(crate) fn resolve_named_path(
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

    pub(crate) fn materialize_directory_path(
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

    pub(crate) fn materialize_named_path(
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

    pub(crate) fn area_root(&self, area: FileArea) -> PathBuf {
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

    pub(crate) fn take_transfer(&self, transfer_key: &str) -> Option<PendingTransfer> {
        self.transfers
            .lock()
            .ok()
            .and_then(|mut transfers| transfers.remove(transfer_key))
    }

    pub(crate) fn emit_started(&self, transfer: &PendingTransfer) {
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

    pub(crate) fn emit_progress(&self, transfer: &PendingTransfer, bytes_transferred: u64) {
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

    pub(crate) fn emit_status(&self, transfer: &PendingTransfer, status: u32, message: impl Into<String>) {
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

    pub(crate) fn emit_event(&self, event: FileTransferEvent) {
        if let Ok(notifiers) = self.notifiers.lock() {
            for notifier in notifiers.iter() {
                notifier(&event);
            }
        }
    }
}
