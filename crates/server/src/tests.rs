//! Daemon + clientes reales por el socket (M2.8): lock exclusivo, identidad
//! (D09), sesiones (RF-ACT-03), eventos (RNF-08), tareas periódicas
//! (RF-COO-03) y 20 agentes concurrentes (RNF-07).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use client::{Client, Connect};
use engine::{Args, Context};
use i18n::Lang;
use model::{Actor, Timestamp};
use protocol::ArgMap;
use serde_json::Value;

use crate::{Daemon, Options, ServeError};

fn tempdb() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("taskcraft-srv-{}", model::TaskId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("workspace.db")
}

/// Daemon corriendo en su hilo; se detiene al soltarlo.
struct Running {
    db: PathBuf,
    stop: tokio::sync::watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn start_with(db: PathBuf, before: impl FnOnce(&Daemon)) -> Running {
    let daemon = Daemon::open(Options {
        db: db.clone(),
        idle_exit: None,
    })
    .unwrap();
    before(&daemon);
    let stop = daemon.shared.shutdown.clone();
    let socket = daemon.socket().to_path_buf();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(daemon.run()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    Running {
        db,
        stop,
        thread: Some(thread),
    }
}

fn start() -> Running {
    start_with(tempdb(), |_| {})
}

fn opts(db: &Path) -> Connect {
    Connect {
        autostart: false,
        ..Connect::new(db.to_path_buf(), "test")
    }
}

fn args(pairs: &[(&str, &str)]) -> ArgMap {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn run(c: &mut Client, id: &str, pairs: &[(&str, &str)]) -> Value {
    c.request(id, &args(pairs), Lang::En)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn supervisor(d: &Running) -> Client {
    Client::connect(&opts(&d.db)).unwrap()
}

/// Registra una interfaz con un agente dentro (D22) y devuelve su token.
fn register(sup: &mut Client, name: &str) -> String {
    let token = run(sup, "interface.register", &[("name", name)])["token"]
        .as_str()
        .unwrap()
        .to_owned();
    run(
        sup,
        "agent.register",
        &[("name", name), ("interface", name)],
    );
    token
}

#[test]
fn only_one_daemon_per_workspace() {
    let d = start();
    match Daemon::open(Options {
        db: d.db.clone(),
        idle_exit: None,
    }) {
        Err(ServeError::AlreadyRunning(_)) => {}
        other => panic!("debía estar tomado: {:?}", other.err()),
    }
    assert!(client::ping(&d.db).is_some());
}

#[test]
fn stopping_the_daemon_frees_the_workspace() {
    let mut d = start();
    assert!(matches!(
        crate::lock_workspace(&d.db),
        Err(ServeError::AlreadyRunning(_))
    ));
    client::stop_daemon(&d.db).unwrap();
    if let Some(t) = d.thread.take() {
        t.join().unwrap();
    }
    assert!(client::ping(&d.db).is_none());
    crate::lock_workspace(&d.db).expect("libre tras detenerlo");
}

#[test]
fn identity_comes_from_the_connection() {
    let d = start();
    let mut sup = supervisor(&d);
    assert_eq!(sup.actor(), "supervisor");
    let token = register(&mut sup, "claude-1");

    let as_agent = Client::connect(&Connect {
        token: Some(token),
        ..opts(&d.db)
    })
    .unwrap();
    assert!(as_agent.actor().starts_with("agent:"));
    assert!(as_agent.session().is_some(), "abre su sesión");

    let bad = Client::connect(&Connect {
        token: Some("tc_falso".into()),
        ..opts(&d.db)
    });
    match bad {
        Err(client::ClientError::Rejected(e)) => assert_eq!(e.code, "FORBIDDEN"),
        other => panic!("debía rechazarse: {:?}", other.err()),
    }
    let by_name = Client::connect(&Connect {
        agent: Some("claude-1".into()),
        ..opts(&d.db)
    })
    .unwrap();
    assert_eq!(by_name.actor(), as_agent.actor());
}

#[test]
fn sessions_follow_the_connection() {
    let d = start();
    let mut sup = supervisor(&d);
    let token = register(&mut sup, "claude-1");
    let connected =
        |sup: &mut Client| run(sup, "agent.get", &[("agent", "claude-1")])["connected"].clone();
    assert_eq!(connected(&mut sup), false);
    let agent = Client::connect(&Connect {
        token: Some(token),
        ..opts(&d.db)
    })
    .unwrap();
    assert_eq!(connected(&mut sup), true);
    drop(agent);
    let deadline = Instant::now() + Duration::from_secs(3);
    while connected(&mut sup) == true && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(connected(&mut sup), false, "al desconectarse se cierra");
}

#[test]
fn errors_arrive_translated_with_code_and_key() {
    let d = start();
    let mut sup = supervisor(&d);
    let e = sup
        .request("task.get", &args(&[("task", "NOPE-1")]), Lang::Es)
        .unwrap_err();
    assert_eq!(e.code, "NOT_FOUND");
    assert!(e.message.contains("No existe"), "{}", e.message);
    let e = sup
        .request("task.get", &args(&[("task", "NOPE-1")]), Lang::En)
        .unwrap_err();
    assert!(e.message.contains("does not exist"), "{}", e.message);
}

#[test]
fn writes_reach_subscribers_in_under_a_second() {
    let d = start();
    let (tx, rx) = mpsc::channel();
    let _events = client::subscribe(&opts(&d.db), move |token| tx.send(token).is_ok()).unwrap();
    // Las tareas periódicas del arranque también escriben: esperar a que pasen.
    while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
    let mut sup = supervisor(&d);
    let sent = Instant::now();
    run(&mut sup, "project.create", &[("key", "API")]);
    rx.recv_timeout(Duration::from_secs(1))
        .expect("evento en menos de 1 s (RNF-08)");
    assert!(sent.elapsed() < Duration::from_secs(1));
    run(&mut sup, "project.list", &[]);
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "leer no genera eventos"
    );
}

#[test]
fn only_the_supervisor_stops_the_daemon() {
    let d = start();
    let mut sup = supervisor(&d);
    let token = register(&mut sup, "claude-1");
    let mut agent = Client::connect(&Connect {
        token: Some(token),
        ..opts(&d.db)
    })
    .unwrap();
    assert!(agent.shutdown().is_err());
    sup.shutdown().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while client::ping(&d.db).is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(client::ping(&d.db).is_none());
}

#[test]
fn expired_leases_are_released_by_the_system_at_startup() {
    let db = tempdb();
    let d = start_with(db, |daemon| {
        let engine = daemon.engine();
        let sup = Context::supervisor();
        engine
            .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
            .unwrap();
        engine
            .dispatch(
                &sup,
                "task.create",
                &Args::none().with("project", "API").with("title", "x"),
            )
            .unwrap();
        engine
            .dispatch(
                &sup,
                "interface.register",
                &Args::none().with("name", "lento"),
            )
            .unwrap();
        let agent = engine
            .dispatch(
                &sup,
                "agent.register",
                &Args::none()
                    .with("name", "lento")
                    .with("interface", "lento"),
            )
            .unwrap();
        let actor = Actor::Agent(agent["id"].as_str().unwrap().parse().unwrap());
        let long_ago = Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(1))
            .unwrap();
        engine
            .dispatch_at(
                &Context::new(actor),
                "task.claim",
                &Args::none().with("task", "API-1"),
                long_ago,
            )
            .unwrap();
    });
    let mut sup = supervisor(&d);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let task = run(&mut sup, "task.get", &[("task", "API-1")]);
        if task["status"] == "todo" {
            assert!(task["claim"].is_null());
            break;
        }
        assert!(Instant::now() < deadline, "el sistema debía liberarla");
        std::thread::sleep(Duration::from_millis(20));
    }
    let history = run(&mut sup, "activity.list", &[("actor", "system")]);
    assert!(history["count"].as_u64().unwrap() > 0);
}

#[test]
fn twenty_agents_take_distinct_tasks_concurrently() {
    let d = start();
    let mut sup = supervisor(&d);
    run(&mut sup, "project.create", &[("key", "API")]);
    for i in 0..30 {
        run(
            &mut sup,
            "task.create",
            &[("project", "API"), ("title", &format!("t{i}"))],
        );
    }
    let tokens: Vec<String> = (0..20)
        .map(|i| register(&mut sup, &format!("agente-{i}")))
        .collect();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(tokens.len()));
    let handles: Vec<_> = tokens
        .into_iter()
        .map(|token| {
            let barrier = barrier.clone();
            let opts = Connect {
                token: Some(token),
                ..opts(&d.db)
            };
            std::thread::spawn(move || {
                let mut c = Client::connect(&opts).unwrap();
                barrier.wait();
                let got = c
                    .request("task.next", &args(&[("claim", "true")]), Lang::En)
                    .unwrap();
                got["task"]["key"].as_str().map(str::to_owned)
            })
        })
        .collect();
    let keys: Vec<String> = handles
        .into_iter()
        .map(|h| h.join().unwrap().expect("cada agente toma una"))
        .collect();
    let distinct: HashSet<&String> = keys.iter().collect();
    assert_eq!(distinct.len(), 20, "ninguna tarea se toma dos veces");
}

/// Servidor HTTP mínimo: responde `status` a una petición y devuelve lo recibido.
fn http_once(status: u16) -> (String, mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        // Cabeceras y cuerpo (Content-Length).
        loop {
            let n = s.read(&mut chunk).unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let len = head
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if body.len() >= len {
                    break;
                }
            }
        }
        let reply =
            format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        s.write_all(reply.as_bytes()).unwrap();
        tx.send(String::from_utf8_lossy(&buf).to_string()).unwrap();
    });
    (url, rx)
}

#[test]
fn webhooks_are_signed_and_failures_retry() {
    let engine = engine::SharedEngine::new(engine::Engine::new(
        store_sqlite::SqliteStore::open_in_memory().unwrap(),
    ));
    let sup = Context::supervisor();
    let run = |id: &str, pairs: &[(&str, &str)]| {
        let a = pairs.iter().fold(Args::none(), |a, (k, v)| a.with(*k, *v));
        engine.dispatch(&sup, id, &a).unwrap()
    };
    run("project.create", &[("key", "API")]);
    run("task.create", &[("project", "API"), ("title", "t")]);

    let (url, received) = http_once(200);
    let hook = run(
        "webhook.create",
        &[("url", &url), ("triggers", "status_changed")],
    );
    let secret = hook["secret"].as_str().unwrap().to_owned();
    run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    crate::webhooks::deliver_due(&engine);
    let request = received.recv_timeout(Duration::from_secs(5)).unwrap();
    let (head, body) = request.split_once("\r\n\r\n").unwrap();
    let expected = foundation::secret::hmac_sha256_hex(secret.as_bytes(), body.as_bytes());
    assert!(
        head.to_lowercase()
            .contains(&format!("x-taskcraft-signature: sha256={expected}")),
        "{head}"
    );
    assert!(body.contains("\"event\":\"status_changed\""), "{body}");
    let list = run("webhook.deliveries", &[]);
    assert_eq!(list["items"][0]["status"], "delivered");

    // Un destino que falla: se reintenta más tarde.
    run("webhook.delete", &[("webhook", &url)]);
    let (url, received) = http_once(503);
    run("webhook.create", &[("url", &url)]);
    run("task.move", &[("task", "API-1"), ("status", "todo")]);
    crate::webhooks::deliver_due(&engine);
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    let list = run("webhook.deliveries", &[]);
    let item = &list["items"][0];
    assert_eq!(item["status"], "pending", "{list}");
    assert_eq!(item["attempts"], 1);
    assert!(item["last_error"].as_str().unwrap().contains("503"));
}

/// El daemon sobre PostgreSQL (D13): solo con `TASKCRAFT_TEST_PG`, en una
/// base nueva para no mezclarse con otras pruebas.
#[test]
fn the_daemon_serves_a_postgres_workspace() {
    let Ok(url) = std::env::var("TASKCRAFT_TEST_PG") else {
        return;
    };
    let name = format!(
        "tc_{}",
        model::TaskId::generate().to_string().to_lowercase()
    );
    std::thread::spawn({
        let url = url.clone();
        let name = name.clone();
        move || {
            let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();
            admin
                .batch_execute(&format!("CREATE DATABASE {name}"))
                .unwrap();
        }
    })
    .join()
    .unwrap();
    let base = url.rsplit_once('/').map_or(url.as_str(), |(b, _)| b);
    let db = PathBuf::from(format!("{base}/{name}"));
    let d = start_with(db.clone(), |_| {});
    let mut sup = supervisor(&d);
    run(&mut sup, "project.create", &[("key", "PG")]);
    run(
        &mut sup,
        "task.create",
        &[("project", "PG"), ("title", "en postgres")],
    );

    let (tx, rx) = mpsc::channel();
    let events = opts(&db);
    std::thread::spawn(move || {
        let _ = client::subscribe(&events, move |token| {
            let _ = tx.send(token);
            false
        });
    });
    std::thread::sleep(Duration::from_millis(200));
    run(
        &mut sup,
        "task.create",
        &[("project", "PG"), ("title", "otra")],
    );
    rx.recv_timeout(Duration::from_secs(2))
        .expect("evento en menos de 2 s");
    let found = run(&mut sup, "task.search", &[("query", "postgres")]);
    assert_eq!(found["items"][0]["key"], "PG-1");
}
