use anyhow::Result;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tower_http::services::ServeDir;

use crate::web::load_server_tls_config;

pub const DEFAULT_WEB_CLIENT_BIND: &str = "127.0.0.1:8080";

fn get_cert_sha256(certificate_path: &Path) -> Option<String> {
    use base64::Engine;
    let file = std::fs::File::open(certificate_path).ok()?;
    let mut cert_reader = std::io::BufReader::new(file);
    let mut certs = rustls_pemfile::certs(&mut cert_reader);
    match certs.next() {
        Some(Ok(cert)) => {
            let hash = <sha2::Sha256 as sha2::Digest>::digest(&cert);
            let encoded = base64::engine::general_purpose::STANDARD.encode(hash);
            Some(encoded)
        }
        _ => None,
    }
}

pub struct WebClientServer {
    bind_addr: SocketAddr,
    static_dir: PathBuf,
    cert_path: PathBuf,
    tls_config: Arc<ServerConfig>,
    cert_hash: Option<String>,
    webtransport_port: Option<u16>,
}

impl WebClientServer {
    pub fn bind(
        bind_addr: &str,
        static_dir: PathBuf,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
        webtransport_port: Option<u16>,
    ) -> Result<Self> {
        let addr: SocketAddr = bind_addr.parse()?;
        let cert_path = certificate_path.as_ref().to_path_buf();
        let cert_hash = get_cert_sha256(&cert_path);
        Ok(Self {
            bind_addr: addr,
            static_dir,
            cert_path,
            tls_config: load_server_tls_config(certificate_path, private_key_path)?,
            cert_hash,
            webtransport_port,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.bind_addr)
    }

    pub fn run(self) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        rt.block_on(run_web_client_server(self))?;
        Ok(())
    }
}

#[derive(Clone)]
struct WebClientState {
    static_dir: PathBuf,
    cert_path: PathBuf,
    cert_hash: Option<String>,
    webtransport_port: Option<u16>,
}

async fn serve_no_cache_file(
    State(state): State<WebClientState>,
    relative_path: &'static str,
    content_type: &'static str,
) -> Response {
    let file_path = state.static_dir.join(relative_path);
    match tokio::fs::read(&file_path).await {
        Ok(body) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                (header::PRAGMA, "no-cache"),
                (header::EXPIRES, "0"),
            ],
            body,
        )
            .into_response(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            eprintln!(
                "web client failed to read {}: {error:#}",
                file_path.display()
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn serve_index_html(State(state): State<WebClientState>) -> Response {
    serve_no_cache_file(State(state), "index.html", "text/html; charset=utf-8").await
}

async fn serve_manifest(State(state): State<WebClientState>) -> Response {
    serve_no_cache_file(
        State(state),
        "manifest.json",
        "application/json; charset=utf-8",
    )
    .await
}

async fn get_cert_hash(State(state): State<WebClientState>) -> Response {
    let mut json = serde_json::json!({});
    if let Some(hash) = get_cert_sha256(&state.cert_path).or_else(|| state.cert_hash.clone()) {
        json["sha256"] = serde_json::Value::String(hash);
    }
    if let Some(port) = state.webtransport_port {
        json["webtransport_port"] = serde_json::Value::Number(port.into());
    }
    
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        json.to_string(),
    )
        .into_response()
}

async fn upload_music_options() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "POST, OPTIONS"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type, X-File-Name"),
        ],
    )
        .into_response()
}

fn decode_percent(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(val) = u8::from_str_radix(hex_str, 16)
        {
            out.push(val);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn upload_music_file(
    State(state): State<WebClientState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let raw_name = headers
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("uploaded_track.mp3");
    let decoded_name = decode_percent(raw_name);

    let safe_name: String = Path::new(&decoded_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("uploaded_track.mp3")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_string();

    let ext_ok = Path::new(&safe_name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp3" | "ogg" | "opus" | "wav" | "flac" | "m4a" | "aac" | "webm" | "mp4" | "mkv" | "mov" | "avi"
            )
        });

    if safe_name.is_empty() || !ext_ok || body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            serde_json::json!({
                "ok": false,
                "error": "Ungültiges Medienformat. Erlaubt: mp3, ogg, opus, wav, flac, m4a, aac, webm, mp4, mkv, mov, avi"
            })
            .to_string(),
        )
            .into_response();
    }

    // cert_path is `<server_root>/data/tls/blackteaweb-localhost-cert.pem`
    let data_dir = state
        .cert_path
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("data"));
    let music_dir = data_dir.join("music");
    if let Err(err) = tokio::fs::create_dir_all(&music_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            serde_json::json!({ "ok": false, "error": err.to_string() }).to_string(),
        )
            .into_response();
    }

    let target_path = music_dir.join(&safe_name);
    if let Err(err) = tokio::fs::write(&target_path, &body).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            serde_json::json!({ "ok": false, "error": err.to_string() }).to_string(),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        serde_json::json!({
            "ok": true,
            "name": safe_name,
            "url": format!("/music/{}", safe_name),
            "size": body.len(),
        })
        .to_string(),
    )
        .into_response()
}

async fn run_web_client_server(server: WebClientServer) -> Result<()> {
    let state = WebClientState {
        static_dir: server.static_dir.clone(),
        cert_path: server.cert_path.clone(),
        cert_hash: server.cert_hash.clone(),
        webtransport_port: server.webtransport_port,
    };
    let app = Router::new()
        .route("/", get(serve_index_html))
        .route("/index.html", get(serve_index_html))
        .route("/manifest.json", get(serve_manifest))
        .route("/cert-hash", get(get_cert_hash))
        .route(
            "/api/music/upload",
            axum::routing::post(upload_music_file).options(upload_music_options),
        )
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024))
        .fallback_service(ServeDir::new(&server.static_dir))
        .with_state(state);
    let tls_acceptor = TlsAcceptor::from(Arc::clone(&server.tls_config));
    let listener = tokio::net::TcpListener::bind(server.bind_addr).await?;

    let https_port = server.bind_addr.port();
    let http_port = if https_port == 443 {
        80
    } else if https_port == 8080 {
        8081
    } else {
        8080
    };
    let http_addr = SocketAddr::new(server.bind_addr.ip(), http_port);
    let http_app = app.clone();

    tokio::spawn(async move {
        if let Ok(listener) = tokio::net::TcpListener::bind(http_addr).await {
            println!("web client HTTP listening on http://{}", http_addr);
            let _ = axum::serve(listener, http_app).await;
        } else {
            eprintln!("failed to bind HTTP web client on {}", http_addr);
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        let app = app.clone();

        tokio::spawn(async move {
            let tls_stream = match tls_acceptor.accept(stream).await {
                Ok(stream) => stream,
                Err(error) => {
                    eprintln!("web client TLS handshake failed: {error:#}");
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);
            if let Err(error) = http1::Builder::new()
                .serve_connection(io, TowerToHyperService::new(app))
                .await
            {
                eprintln!("web client connection failed: {error:#}");
            }
        });
    }
}
