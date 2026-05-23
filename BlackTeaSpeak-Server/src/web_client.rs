use anyhow::Result;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Redirect, Response};
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

use crate::web_transport::load_server_tls_config;

pub const DEFAULT_WEB_CLIENT_BIND: &str = "127.0.0.1:8080";

pub struct WebClientServer {
    bind_addr: SocketAddr,
    static_dir: PathBuf,
    tls_config: Arc<ServerConfig>,
}

impl WebClientServer {
    pub fn bind(
        bind_addr: &str,
        static_dir: PathBuf,
        certificate_path: impl AsRef<Path>,
        private_key_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let addr: SocketAddr = bind_addr.parse()?;
        Ok(Self {
            bind_addr: addr,
            static_dir,
            tls_config: load_server_tls_config(certificate_path, private_key_path)?,
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

async fn run_web_client_server(server: WebClientServer) -> Result<()> {
    let state = WebClientState {
        static_dir: server.static_dir.clone(),
    };
    let app = Router::new()
        .route("/", get(serve_index_html))
        .route("/index.html", get(serve_index_html))
        .route("/manifest.json", get(serve_manifest))
        .fallback_service(ServeDir::new(&server.static_dir))
        .with_state(state);
    let tls_acceptor = TlsAcceptor::from(Arc::clone(&server.tls_config));
    let listener = tokio::net::TcpListener::bind(server.bind_addr).await?;

    let https_port = server.bind_addr.port();
    let http_port = if https_port == 443 { 80 } else { 8080 };
    let http_addr = SocketAddr::new(server.bind_addr.ip(), http_port);
    
    tokio::spawn(async move {
        let redirect_app = Router::new().fallback(move |headers: HeaderMap, uri: Uri| async move {
            let host = headers
                .get(axum::http::header::HOST)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("localhost");
            let mut parts = uri.into_parts();
            parts.scheme = Some(axum::http::uri::Scheme::HTTPS);
            if parts.path_and_query.is_none() {
                parts.path_and_query = Some("/".parse().unwrap());
            }
            let https_host = if https_port != 443 {
                let host = host.split(':').next().unwrap_or(&host);
                format!("{}:{}", host, https_port)
            } else {
                host.split(':').next().unwrap_or(&host).to_string()
            };
            parts.authority = Some(https_host.parse().unwrap());
            if let Ok(uri) = Uri::from_parts(parts) {
                Ok::<_, StatusCode>(Redirect::permanent(&uri.to_string()))
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        });
        
        if let Ok(listener) = tokio::net::TcpListener::bind(http_addr).await {
            println!("web client HTTP redirect listening on http://{}", http_addr);
            let _ = axum::serve(listener, redirect_app).await;
        } else {
            eprintln!("failed to bind HTTP redirect on {}", http_addr);
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
