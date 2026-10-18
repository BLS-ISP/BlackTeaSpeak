use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use anyhow::{Context, Result, anyhow};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use super::*;
pub struct FileTransferServer {
    pub(crate) listener: TcpListener,
    pub(crate) tls_config: Arc<ServerConfig>,
    pub(crate) shutdown: Arc<AtomicBool>,
    pub(crate) registry: Arc<FileTransferRegistry>,
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
pub(crate) fn handle_transfer_client(
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

    let mut first_byte = [0u8; 1];
    if stream.peek(&mut first_byte).unwrap_or(0) == 1 && first_byte[0] != 0x16 {
        return handle_raw_tcp_transfer(stream, registry);
    }

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

fn handle_raw_tcp_transfer(
    mut stream: TcpStream,
    registry: Arc<FileTransferRegistry>,
) -> Result<()> {
    let mut header_buf = Vec::with_capacity(256);
    let mut scratch = [0u8; 4096];
    let (transfer, mut remaining_payload) = loop {
        let n = stream
            .read(&mut scratch)
            .context("failed to read raw TCP transfer key")?;
        if n == 0 {
            return Err(anyhow!("unexpected EOF while reading raw TCP transfer key"));
        }
        header_buf.extend_from_slice(&scratch[..n]);
        if let Some((_key, transfer, consumed)) =
            registry.take_transfer_matching_prefix(&header_buf)
        {
            let rest = header_buf[consumed..].to_vec();
            break (transfer, rest);
        }
        if header_buf.len() > 1024 {
            return Err(anyhow!("invalid or unknown raw TCP transfer key"));
        }
    };

    registry.emit_started(&transfer);
    match transfer.direction {
        FileTransferDirection::Download => {
            let mut file =
                File::open(&transfer.file_path).context("failed to open raw download file")?;
            file.seek(SeekFrom::Start(transfer.seek_position))
                .context("failed to seek raw download file")?;
            let mut bytes_transferred = 0u64;
            loop {
                let n = file
                    .read(&mut scratch)
                    .context("failed to read raw download file")?;
                if n == 0 {
                    break;
                }
                stream
                    .write_all(&scratch[..n])
                    .context("failed to write raw download stream")?;
                bytes_transferred = bytes_transferred.saturating_add(n as u64);
            }
            stream.flush().ok();
            registry.emit_progress(&transfer, bytes_transferred);
            registry.emit_status(&transfer, FILE_TRANSFER_STATUS_COMPLETE, "ok");
            Ok(())
        }
        FileTransferDirection::Upload => {
            if remaining_payload.len() > transfer.size as usize
                && (remaining_payload.starts_with(b"\r\n") || remaining_payload.starts_with(b"\n"))
            {
                if remaining_payload.starts_with(b"\r\n") {
                    remaining_payload.drain(..2);
                } else {
                    remaining_payload.drain(..1);
                }
            }
            if let Some(parent) = transfer.file_path.parent() {
                fs::create_dir_all(parent)
                    .context("failed to create raw upload parent directory")?;
            }
            let mut file =
                File::create(&transfer.file_path).context("failed to create raw upload file")?;
            if transfer.seek_position > 0 {
                file.seek(SeekFrom::Start(transfer.seek_position))
                    .context("failed to seek raw upload file")?;
            }
            let mut bytes_transferred = 0u64;
            if !remaining_payload.is_empty() {
                let to_write =
                    remaining_payload.len().min(transfer.size as usize);
                file.write_all(&remaining_payload[..to_write])
                    .context("failed to write initial raw upload bytes")?;
                bytes_transferred = bytes_transferred.saturating_add(to_write as u64);
            }
            while bytes_transferred < transfer.size {
                let want = ((transfer.size - bytes_transferred) as usize).min(scratch.len());
                let n = stream
                    .read(&mut scratch[..want])
                    .context("failed to read raw upload stream")?;
                if n == 0 {
                    break;
                }
                file.write_all(&scratch[..n])
                    .context("failed to write raw upload file")?;
                bytes_transferred = bytes_transferred.saturating_add(n as u64);
            }
            file.flush().context("failed to flush raw upload file")?;
            registry.emit_progress(&transfer, bytes_transferred);
            registry.emit_status(&transfer, FILE_TRANSFER_STATUS_COMPLETE, "ok");
            Ok(())
        }
    }
}
