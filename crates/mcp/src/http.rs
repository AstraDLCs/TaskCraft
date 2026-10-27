//! MCP por Streamable HTTP (RF-MCP-01), servido por el daemon, y la API de la
//! app web (D14). Cada petición debe traer `Authorization: Bearer <token>`
//! (D09, RNF-09). Fuera de loopback, solo con TLS.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::json;
use tokio::sync::watch;

use crate::{Caller, LocalBackend, TaskcraftMcp};

/// Puerto por defecto del MCP HTTP.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7417";

#[derive(Debug)]
pub enum HttpError {
    /// Fuera de loopback hace falta TLS (RNF-09, D14).
    NotLoopback(SocketAddr),
    Tls(String),
    Io(String),
}

/// Certificado y clave en PEM para servir con TLS (D14).
#[derive(Debug, Clone)]
pub struct Tls {
    pub cert: std::path::PathBuf,
    pub key: std::path::PathBuf,
}

/// Opciones del servidor HTTP del daemon.
#[derive(Clone, Default)]
pub struct Options {
    /// API y archivos de la app web (D14).
    pub web: Option<crate::web::Web>,
    pub tls: Option<Tls>,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotLoopback(addr) => write!(
                f,
                "refusing to listen on {addr} without TLS: use --tls-cert and --tls-key, or a loopback address"
            ),
            Self::Tls(e) => write!(f, "TLS: {e}"),
            Self::Io(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for HttpError {}

/// Abre el puerto y devuelve la dirección real (útil con el puerto 0) y la
/// tarea que sirve hasta que `shutdown` pase a `true`. Solo MCP, sin TLS.
pub async fn serve(
    addr: SocketAddr,
    backend: Arc<LocalBackend>,
    shutdown: watch::Receiver<bool>,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), HttpError> {
    serve_with(addr, backend, shutdown, Options::default()).await
}

/// Como [`serve`], con la app web y TLS (D14).
pub async fn serve_with(
    addr: SocketAddr,
    backend: Arc<LocalBackend>,
    mut shutdown: watch::Receiver<bool>,
    options: Options,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), HttpError> {
    if !addr.ip().is_loopback() && options.tls.is_none() {
        return Err(HttpError::NotLoopback(addr));
    }
    let tls = options.tls.as_ref().map(acceptor).transpose()?;
    let config = StreamableHttpServerConfig::default();
    let sessions_backend = backend.clone();
    // Un manejador por sesión MCP: cada una elige su agente (D15).
    let mcp = StreamableHttpService::new(
        move || Ok(TaskcraftMcp::new(sessions_backend.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let mut router = axum::Router::new()
        .nest_service("/mcp", mcp)
        .layer(axum::middleware::from_fn_with_state(backend, authenticate))
        .route("/health", axum::routing::get(health));
    if let Some(web) = options.web {
        router = router.merge(crate::web::router(web));
    }
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| HttpError::Io(format!("{addr}: {e}")))?;
    let local = listener
        .local_addr()
        .map_err(|e| HttpError::Io(e.to_string()))?;
    tracing::info!(%local, tls = tls.is_some(), "HTTP listening");
    let task = tokio::spawn(async move {
        let stop = async move {
            while !*shutdown.borrow() {
                if shutdown.changed().await.is_err() {
                    return;
                }
            }
        };
        let served = match tls {
            Some(acceptor) => {
                axum::serve(
                    TlsListener {
                        tcp: listener,
                        acceptor,
                    },
                    router,
                )
                .with_graceful_shutdown(stop)
                .await
            }
            None => {
                axum::serve(listener, router)
                    .with_graceful_shutdown(stop)
                    .await
            }
        };
        if let Err(e) = served {
            tracing::error!(error = %e, "HTTP stopped");
        }
    });
    Ok((local, task))
}

/// Configuración rustls a partir del certificado y la clave PEM.
fn acceptor(tls: &Tls) -> Result<tokio_rustls::TlsAcceptor, HttpError> {
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, PrivateKeyDer};
    let certs = CertificateDer::pem_file_iter(&tls.cert)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| HttpError::Tls(format!("{}: {e}", tls.cert.display())))?;
    let key = PrivateKeyDer::from_pem_file(&tls.key)
        .map_err(|e| HttpError::Tls(format!("{}: {e}", tls.key.display())))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| HttpError::Tls(e.to_string()))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| HttpError::Tls(e.to_string()))?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}

/// Acepta TCP y hace el apretón de manos TLS; los que fallan se descartan.
struct TlsListener {
    tcp: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = match self.tcp.accept().await {
                Ok(x) => x,
                Err(e) => {
                    tracing::warn!(error = %e, "accept failed");
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
            };
            match self.acceptor.accept(stream).await {
                Ok(tls) => return (tls, addr),
                Err(e) => tracing::debug!(%addr, error = %e, "TLS handshake failed"),
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

async fn health() -> impl IntoResponse {
    axum::Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Resuelve el token a un agente activo y lo deja en la petición.
async fn authenticate(
    State(backend): State<Arc<LocalBackend>>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }
    let Some(token) = bearer(request.headers()) else {
        return unauthorized("missing Authorization: Bearer <interface token>");
    };
    let engine = backend.engine().clone();
    let found = tokio::task::spawn_blocking(move || {
        engine.read(|r| engine::identity::from_token(r, &token))
    })
    .await;
    match found {
        Ok(Ok(actor)) => {
            request.extensions_mut().insert(Caller(actor));
            next.run(request).await
        }
        _ => unauthorized("invalid token"),
    }
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    Some(token.trim().to_owned())
}

pub(crate) fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({ "code": "FORBIDDEN", "message": message })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use engine::{Args, Context, Engine, SharedEngine};
    use store_sqlite::SqliteStore;

    use super::*;

    fn backend() -> (Arc<LocalBackend>, String) {
        let engine = SharedEngine::new(Engine::new(SqliteStore::open_in_memory().unwrap()));
        let interface = engine
            .dispatch(
                &Context::supervisor(),
                "interface.register",
                &Args::none().with("name", "claude-1"),
            )
            .unwrap();
        engine
            .dispatch(
                &Context::supervisor(),
                "agent.register",
                &Args::none()
                    .with("name", "claude-1")
                    .with("interface", "claude-1"),
            )
            .unwrap();
        let token = interface["token"].as_str().unwrap().to_owned();
        (Arc::new(LocalBackend::new(engine)), token)
    }

    /// Petición HTTP/1.1 mínima; devuelve (estado, cuerpo).
    fn post(addr: SocketAddr, token: Option<&str>, body: &str) -> (u16, String) {
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        write!(
            s,
            "POST /mcp HTTP/1.1\r\nHost: {addr}\r\n{auth}Content-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        // Con SSE el servidor puede mantener la conexión: basta con la primera respuesta.
        while let Ok(n) = s.read(&mut chunk) {
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if String::from_utf8_lossy(&buf).contains("\"result\"") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&buf).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        (status, text)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn requires_an_agent_token_and_only_listens_on_loopback() {
        let (backend, token) = backend();
        let (_tx, rx) = watch::channel(false);
        let refused = serve("0.0.0.0:0".parse().unwrap(), backend.clone(), rx.clone()).await;
        assert!(matches!(refused, Err(HttpError::NotLoopback(_))));

        let (addr, _task) = serve("127.0.0.1:0".parse().unwrap(), backend, rx)
            .await
            .unwrap();
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
        let (status, _) = tokio::task::spawn_blocking(move || post(addr, None, init))
            .await
            .unwrap();
        assert_eq!(status, 401);
        let (status, _) = tokio::task::spawn_blocking(move || post(addr, Some("tc_malo"), init))
            .await
            .unwrap();
        assert_eq!(status, 401);
        let (status, body) = tokio::task::spawn_blocking(move || post(addr, Some(&token), init))
            .await
            .unwrap();
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("\"taskcraft\""), "{body}");
    }
}
