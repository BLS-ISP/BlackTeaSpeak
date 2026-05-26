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
