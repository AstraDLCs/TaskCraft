//! API web (D14): tokens de supervisor y de agente, comandos, sondeo largo,
//! archivos de la app y TLS.

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine::{Args, Context, Engine, SharedEngine};
use store_sqlite::SqliteStore;
use tokio::sync::{broadcast, watch};

use super::Web;
use crate::LocalBackend;
use crate::http::{Options, Tls, serve_with};

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tc-web-{}", model::TaskId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    engine: SharedEngine,
    changes: broadcast::Sender<u64>,
    home: PathBuf,
    supervisor: String,
    agent: String,
}

fn fixture() -> Fixture {
    let home = tempdir();
    let mut engine = Engine::new(SqliteStore::open_in_memory().unwrap());
    engine.set_home(&home);
    let engine = SharedEngine::new(engine);
    let (changes, _) = broadcast::channel(16);
    let sender = changes.clone();
    engine.set_on_commit(Box::new(move |t| {
        let _ = sender.send(t);
    }));
    let sup = Context::supervisor();
    let run = |id: &str, a: Args| engine.dispatch(&sup, id, &a).unwrap();
    run("project.create", Args::none().with("key", "WEB"));
    run(
        "task.create",
        Args::none().with("project", "WEB").with("title", "remota"),
    );
    let supervisor = run("workspace.web_token", Args::none())["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let agent = run("interface.register", Args::none().with("name", "robot_web"))["token"]
        .as_str()
        .unwrap()
        .to_owned();
    run(
        "agent.register",
        Args::none()
            .with("name", "robot_web")
            .with("interface", "robot_web"),
    );
    Fixture {
        engine,
        changes,
        home,
        supervisor,
        agent,
    }
}

fn options(f: &Fixture, dir: Option<PathBuf>, tls: Option<Tls>) -> Options {
    Options {
        web: Some(Web {
            engine: f.engine.clone(),
            changes: f.changes.clone(),
            home: Some(f.home.clone()),
            dir,
        }),
        tls,
    }
}

/// Petición HTTP/1.1 por un flujo cualquiera; devuelve (estado, cabeceras, cuerpo).
fn http(
    stream: &mut impl ReadWrite,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> (u16, String, String) {
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: x\r\n{auth}Content-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    (status, head.to_owned(), body.to_owned())
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn plain(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> (u16, String, String) {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    http(&mut s, method, path, token, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_web_api_runs_commands_as_supervisor_or_agent() {
    let f = fixture();
    let dir = tempdir();
    std::fs::write(dir.join("index.html"), "<html>tc</html>").unwrap();
    std::fs::write(dir.join("app_bg.wasm"), b"\0asm").unwrap();
    let backend = Arc::new(LocalBackend::new(f.engine.clone()));
    let (_tx, rx) = watch::channel(false);
    let (addr, _task) = serve_with(
        "127.0.0.1:0".parse().unwrap(),
        backend,
        rx,
        options(&f, Some(dir), None),
    )
    .await
    .unwrap();
    let (sup, agent) = (f.supervisor.clone(), f.agent.clone());
    let list = r#"{"command": "task.list", "args": {"filter": "title~remota"}}"#;
    let checks = tokio::task::spawn_blocking(move || {
        let (status, _, _) = plain(addr, "POST", "/api/command", None, list);
        assert_eq!(status, 401);
        let (status, _, _) = plain(addr, "POST", "/api/command", Some("tc_malo"), list);
        assert_eq!(status, 401);
        let (status, _, body) = plain(addr, "POST", "/api/command", Some(&sup), list);
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("WEB-1"), "{body}");
        let (_, _, body) = plain(addr, "GET", "/api/whoami", Some(&agent), "");
        assert!(body.contains("robot_web"), "{body}");
        // Lo exclusivo del supervisor sigue siéndolo por la web.
        let token = r#"{"command": "workspace.web_token"}"#;
        let (status, _, body) = plain(addr, "POST", "/api/command", Some(&agent), token);
        assert_eq!(status, 403, "{body}");
        assert!(body.contains("FORBIDDEN"));
        // Archivos de la app, con su tipo; nada fuera de la carpeta.
        let (status, head, body) = plain(addr, "GET", "/", None, "");
        assert_eq!((status, body.as_str()), (200, "<html>tc</html>"));
        assert!(head.to_lowercase().contains("text/html"));
        let (_, head, _) = plain(addr, "GET", "/app_bg.wasm", None, "");
        assert!(head.to_lowercase().contains("application/wasm"), "{head}");
        let (status, _, _) = plain(addr, "GET", "/../secret", None, "");
        assert!(status == 400 || status == 404, "{status}");
    });
    checks.await.unwrap();

    // Sondeo largo: con el token al día espera; una escritura lo despierta.
    let current = f.engine.change_token().unwrap();
    let sup = f.supervisor.clone();
    let waiter = tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let (_, _, body) = plain(
            addr,
            "GET",
            &format!("/api/changes?since={current}"),
            Some(&sup),
            "",
        );
        (started.elapsed(), body)
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let engine = f.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .dispatch(
                &Context::supervisor(),
                "task.create",
                &Args::none().with("project", "WEB").with("title", "otra"),
            )
            .unwrap();
    })
    .await
    .unwrap();
    let (waited, body) = waiter.await.unwrap();
    assert!(waited < Duration::from_secs(5), "{waited:?}");
    assert!(!body.contains(&format!("\"token\":{current}")), "{body}");

    // Revocado, el token del supervisor deja de valer.
    let engine = f.engine.clone();
    let sup = f.supervisor.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .dispatch(
                &Context::supervisor(),
                "workspace.web_token",
                &Args::none().with("revoke", "true"),
            )
            .unwrap();
        let (status, _, _) = plain(addr, "POST", "/api/command", Some(&sup), list);
        assert_eq!(status, 401);
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn outside_loopback_tls_is_required_and_works() {
    let f = fixture();
    let backend = Arc::new(LocalBackend::new(f.engine.clone()));
    let (_tx, rx) = watch::channel(false);
    let refused = serve_with(
        "0.0.0.0:0".parse().unwrap(),
        backend.clone(),
        rx.clone(),
        options(&f, None, None),
    )
    .await;
    assert!(matches!(
        refused,
        Err(crate::http::HttpError::NotLoopback(_))
    ));

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let dir = tempdir();
    let (cert_path, key_path) = (dir.join("cert.pem"), dir.join("key.pem"));
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
    let tls = Tls {
        cert: cert_path,
        key: key_path,
    };
    let (addr, _task) = serve_with(
        "127.0.0.1:0".parse().unwrap(),
        backend,
        rx,
        options(&f, None, Some(tls)),
    )
    .await
    .unwrap();

    let der = cert.cert.der().clone();
    let sup = f.supervisor.clone();
    tokio::task::spawn_blocking(move || {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(der).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
        let conn = rustls::ClientConnection::new(Arc::new(config), name).unwrap();
        let tcp = std::net::TcpStream::connect(addr).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let body = r#"{"command": "task.list"}"#;
        let (status, _, body) = http(&mut tls, "POST", "/api/command", Some(&sup), body);
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("WEB-1"));
        // Sin TLS, el mismo puerto no responde HTTP en claro.
        let (status, _, _) = plain(addr, "GET", "/health", None, "");
        assert_ne!(status, 200);
    })
    .await
    .unwrap();
}

/// Petición WebDAV con autenticación `Basic` (el token como contraseña).
fn dav(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    depth: &str,
    body: &str,
) -> (u16, String, String) {
    use base64::Engine as _;
    let auth = token
        .map(|t| {
            let pair = base64::engine::general_purpose::STANDARD.encode(format!("yo:{t}"));
            format!("Authorization: Basic {pair}\r\n")
        })
        .unwrap_or_default();
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: x\r\n{auth}Depth: {depth}\r\n\
         Content-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    (status, head.to_owned(), body.to_owned())
}

/// RF-INT-07 (D18): CalDAV de solo lectura y suscripción `.ics`, con el
/// alcance del token.
#[tokio::test(flavor = "multi_thread")]
async fn caldav_serves_tasks_with_due_dates_read_only() {
    let f = fixture();
    let sup_ctx = Context::supervisor();
    let run = |id: &str, a: Args| f.engine.dispatch(&sup_ctx, id, &a).unwrap();
    run(
        "task.update",
        Args::none().with("task", "WEB-1").with("due", "2026-11-01"),
    );
    run("project.create", Args::none().with("key", "OPS"));
    run(
        "task.create",
        Args::none()
            .with("project", "OPS")
            .with("title", "secreta; con, comas")
            .with("due", "2026-11-02"),
    );
    let scoped = run(
        "interface.register",
        Args::none().with("name", "solo-web").with("scope", "WEB"),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let backend = Arc::new(LocalBackend::new(f.engine.clone()));
    let (_tx, rx) = watch::channel(false);
    let (addr, _task) = serve_with(
        "127.0.0.1:0".parse().unwrap(),
        backend,
        rx,
        options(&f, None, None),
    )
    .await
    .unwrap();
    let sup = f.supervisor.clone();
    tokio::task::spawn_blocking(move || {
        let (status, head, _) = dav(addr, "OPTIONS", "/caldav/", None, "0", "");
        assert_eq!(status, 200);
        assert!(head.to_lowercase().contains("calendar-access"), "{head}");
        let (status, head, _) = dav(addr, "GET", "/.well-known/caldav", None, "0", "");
        assert_eq!(status, 301);
        assert!(head.contains("/caldav/"));
        let (status, head, _) = dav(addr, "PROPFIND", "/caldav/", None, "1", "");
        assert_eq!(status, 401);
        assert!(
            head.to_lowercase().contains("www-authenticate: basic"),
            "{head}"
        );

        let (status, _, body) = dav(addr, "PROPFIND", "/caldav/", Some(&sup), "1", "");
        assert_eq!(status, 207, "{body}");
        assert!(body.contains("calendar-home-set"));
        assert!(body.contains("/caldav/WEB/") && body.contains("/caldav/OPS/"));
        let (status, _, body) = dav(addr, "PROPFIND", "/caldav/WEB/", Some(&sup), "1", "");
        assert_eq!(status, 207);
        assert!(body.contains("/caldav/WEB/WEB-1.ics"), "{body}");
        assert!(body.contains("getctag"));

        let multiget = "<C:calendar-multiget xmlns:D=\"DAV:\" \
                        xmlns:C=\"urn:ietf:params:xml:ns:caldav\"><D:prop><D:getetag/>\
                        <C:calendar-data/></D:prop><D:href>/caldav/OPS/OPS-1.ics</D:href>\
                        </C:calendar-multiget>";
        let (status, _, body) = dav(addr, "REPORT", "/caldav/OPS/", Some(&sup), "1", multiget);
        assert_eq!(status, 207, "{body}");
        assert!(body.contains("BEGIN:VTODO"), "{body}");
        assert!(body.contains("DUE;VALUE=DATE:20261102"));
        assert!(body.contains(r"secreta\; con\, comas"), "escapado: {body}");

        let (status, head, body) = dav(addr, "GET", "/caldav/WEB/WEB-1.ics", Some(&sup), "0", "");
        assert_eq!(status, 200);
        assert!(head.to_lowercase().contains("text/calendar"));
        assert!(
            body.contains("UID:") && body.contains("SUMMARY:WEB-1 remota"),
            "{body}"
        );
        let (status, _, _) = dav(addr, "PUT", "/caldav/WEB/WEB-1.ics", Some(&sup), "0", "x");
        assert_eq!(status, 403, "solo lectura");

        // Suscripción con el token en la URL: eventos de día completo.
        let (status, _, body) = plain(addr, "GET", &format!("/cal/WEB.ics?token={sup}"), None, "");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("BEGIN:VEVENT") && body.contains("DTSTART;VALUE=DATE:20261101"));
        assert!(body.contains("DTEND;VALUE=DATE:20261102"));

        // Una interfaz limitada a WEB no ve OPS.
        let (status, _, body) = dav(addr, "PROPFIND", "/caldav/", Some(&scoped), "1", "");
        assert_eq!(status, 207);
        assert!(
            body.contains("/caldav/WEB/") && !body.contains("/caldav/OPS/"),
            "{body}"
        );
        let (status, _, _) = dav(addr, "GET", "/caldav/OPS/OPS-1.ics", Some(&scoped), "0", "");
        assert_eq!(status, 404);
    })
    .await
    .unwrap();
}
