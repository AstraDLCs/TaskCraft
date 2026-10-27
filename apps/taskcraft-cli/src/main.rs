//! `taskcraft-cli`: comandos del registro desde la terminal (RF-INT-01), el
//! daemon (`serve`, D01) y el proxy MCP por stdio (`mcp --stdio`, RF-MCP-01).
//!
//! Todo pasa por el daemon salvo con `--offline`, que abre la base en proceso
//! para depurar.

mod cli;
mod git;
mod output;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use engine::{Engine, LocalDispatch, SharedEngine};
use i18n::{Lang, Localize, Msg, t, tr};
use model::Actor;
use protocol::{ArgMap, Dispatch};

use crate::cli::{Global, Invocation};

fn main() -> ExitCode {
    let lang = cli::pre_lang();
    let matches = cli::command(lang).get_matches();
    let (global, invocation) = match cli::parse(&matches, lang) {
        Ok(parsed) => parsed,
        Err(msg) => return fail(&msg),
    };
    // El proxy MCP habla por stdout: los logs, siempre a stderr y discretos.
    engine::telemetry::init(match invocation {
        Invocation::Serve { .. } => "info",
        _ => "error",
    });
    let lang = global.lang;
    let db = match &global.workspace {
        Some(name) if global.db.is_none() => store_sqlite::workspace_path(name.trim()),
        _ => store_sqlite::resolve_path(global.db.clone()),
    };
    let db = match db {
        Ok(p) => p,
        Err(e) => return fail(&e.localize(lang)),
    };

    match invocation {
        Invocation::Where => {
            println!("{}", db.display());
            ExitCode::SUCCESS
        }
        Invocation::Workspaces => {
            for (name, path) in store_sqlite::list_workspaces() {
                let mark = if path == db { "*" } else { " " };
                println!("{mark} {name:<20} {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Invocation::Commands => {
            for c in engine::Registry::standard().iter() {
                println!("{:<24} {}", c.id(), t(lang, c.label()));
            }
            ExitCode::SUCCESS
        }
        Invocation::Status => status(&db, lang),
        Invocation::Serve {
            http,
            idle_exit,
            stop,
            web,
            tls,
        } => {
            if stop {
                stop_daemon(&db, lang)
            } else {
                serve(&db, http, idle_exit, web, tls, lang)
            }
        }
        Invocation::Mcp => mcp_stdio(&db, &global),
        Invocation::Migrate { from, to } => migrate(&from, &to, lang),
        Invocation::Run { command, args } => run(&db, &global, &command, &args),
        Invocation::GitHook { install, repo } => git::hook(&db, install, &repo, lang),
        Invocation::GitScan { repo, range, quiet } => match backend(&db, &global) {
            Ok(mut b) => git::scan(b.as_mut(), &repo, &range, quiet, lang),
            Err(msg) => fail(&msg),
        },
    }
}

/// Ejecuta un comando del registro e imprime el resultado.
fn run(db: &Path, global: &Global, command: &str, args: &ArgMap) -> ExitCode {
    let lang = global.lang;
    let mut backend = match backend(db, global) {
        Ok(b) => b,
        Err(msg) => return fail(&msg),
    };
    match backend.dispatch(command, args, lang) {
        Ok(value) => {
            if global.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                );
            } else {
                print!("{}", output::render(&value, lang));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            if global.json {
                let body = serde_json::json!({ "code": e.code, "key": e.key, "message": e.message, "current": e.current });
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
                ExitCode::FAILURE
            } else {
                fail(&format!("[{}] {}", e.code, e.message))
            }
        }
    }
}

/// Copia un workspace entero a otro backend (RF-EXP-08): exporta el origen
/// a un `.taskcraft` temporal y lo importa en el destino, que debe estar
/// vacío. El destino no puede tener un daemon abierto.
fn migrate(from: &Path, to: &Path, lang: Lang) -> ExitCode {
    let lock_path = protocol::lock_path(to);
    if let Some(dir) = lock_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path);
    let Ok(lock) = lock else {
        return fail(&format!("{}: lock", lock_path.display()));
    };
    if lock.try_lock().is_err() {
        return fail(&tr(
            lang,
            &Msg::new("cli-migrate-busy").arg("db", to.display().to_string()),
        ));
    }
    let tmp = std::env::temp_dir().join(format!(
        "taskcraft-migrate-{}.taskcraft",
        model::TaskId::generate()
    ));
    let path = tmp.display().to_string();
    let sup = engine::Context::supervisor();
    let step = |db: &Path, id: &str| -> Result<serde_json::Value, String> {
        let store = server::open_store(db).map_err(|e| e.localize(lang))?;
        let mut engine = Engine::new(store);
        engine
            .dispatch(&sup, id, &engine::Args::none().with("path", path.as_str()))
            .map_err(|e| e.localize(lang))
    };
    let result =
        step(from, "workspace.export").and_then(|out| step(to, "workspace.import").map(|_| out));
    let _ = std::fs::remove_file(&tmp);
    match result {
        Ok(out) => {
            println!(
                "{}",
                tr(
                    lang,
                    &Msg::new("cli-migrate-done")
                        .arg(
                            "tasks",
                            out.get("tasks")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0)
                        )
                        .arg("to", to.display().to_string())
                )
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

/// Cliente del daemon o, con `--offline`, el engine en este proceso.
fn backend(db: &Path, global: &Global) -> Result<Box<dyn Dispatch>, String> {
    let lang = global.lang;
    if !global.offline {
        let opts = connect_opts(db, global, "cli");
        return client::Client::connect(&opts)
            .map(|c| Box::new(c) as Box<dyn Dispatch>)
            .map_err(|e| e.localize(lang));
    }
    let mut engine = server::open_engine(db).map_err(|e| {
        tr(
            lang,
            &Msg::new("cli-open-failed")
                .arg("path", db.display().to_string())
                .arg("error", e.localize(lang)),
        )
    })?;
    // Igual que el daemon (D15): el token es de una interfaz y `--agent`
    // elige uno de sus agentes; sin token, el supervisor local.
    let actor = engine
        .read(|r| match (&global.token, &global.agent) {
            (Some(token), Some(name)) => {
                let i = engine::identity::interface_by_token(r, token)?;
                engine::identity::act_as(r, &Actor::Interface(i.id), name)
            }
            (Some(token), None) => engine::identity::from_token(r, token),
            (None, Some(name)) => engine::identity::act_as(r, &Actor::Supervisor, name),
            (None, None) => Ok(Actor::Supervisor),
        })
        .map_err(|e| e.localize(lang))?;
    Ok(Box::new(LocalDispatch::new(
        SharedEngine::new(engine),
        actor,
    )))
}

fn connect_opts(db: &Path, global: &Global, client: &str) -> client::Connect {
    client::Connect {
        lang: global.lang,
        token: global.token.clone(),
        agent: global.agent.clone(),
        ..client::Connect::new(db.to_path_buf(), client)
    }
}

fn status(db: &Path, lang: Lang) -> ExitCode {
    let socket = protocol::socket_path(db).display().to_string();
    match client::ping(db) {
        Some(pid) => {
            println!(
                "{}",
                tr(
                    lang,
                    &Msg::new("cli-daemon-running")
                        .arg("pid", pid)
                        .arg("socket", socket)
                )
            );
            ExitCode::SUCCESS
        }
        None => {
            println!(
                "{}",
                tr(
                    lang,
                    &Msg::new("cli-daemon-not-running").arg("socket", socket)
                )
            );
            ExitCode::FAILURE
        }
    }
}

fn stop_daemon(db: &Path, lang: Lang) -> ExitCode {
    let opts = client::Connect {
        autostart: false,
        lang,
        ..client::Connect::new(db.to_path_buf(), "cli")
    };
    match client::Client::connect(&opts).and_then(|mut c| c.shutdown()) {
        Ok(()) => {
            println!("{}", t(lang, "cli-daemon-stopped"));
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.localize(lang)),
    }
}

/// El daemon: socket local y, si se pide, MCP por HTTP (D08).
fn serve(
    db: &Path,
    http: Option<String>,
    idle_exit: Option<u64>,
    web: Option<PathBuf>,
    tls: Option<(PathBuf, PathBuf)>,
    lang: Lang,
) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return fail(&e.to_string()),
    };
    let opts = server::Options {
        db: db.to_path_buf(),
        idle_exit: idle_exit.map(|m| Duration::from_secs(m.saturating_mul(60))),
    };
    let db_path = db.to_path_buf();
    let result: Result<(), String> = runtime.block_on(async move {
        let daemon = server::Daemon::open(opts).map_err(|e| e.localize(lang))?;
        if let Some(addr) = http {
            let addr = addr.parse().map_err(|e| format!("--http {addr}: {e}"))?;
            let backend =
                Arc::new(mcp::LocalBackend::new(daemon.engine()).with_changes(daemon.changes()));
            // API y app web (D14) en el mismo puerto; fuera de loopback, con TLS.
            let options = mcp::http::Options {
                web: Some(mcp::web::Web {
                    engine: daemon.engine(),
                    changes: daemon.changes(),
                    home: server::home(&db_path),
                    dir: web,
                }),
                tls: tls.map(|(cert, key)| mcp::http::Tls { cert, key }),
            };
            let scheme = if options.tls.is_some() {
                "https"
            } else {
                "http"
            };
            let (local, _task) =
                mcp::http::serve_with(addr, backend, daemon.shutdown_signal(), options)
                    .await
                    .map_err(|e| e.to_string())?;
            eprintln!(
                "{}",
                tr(
                    lang,
                    &Msg::new("cli-mcp-http").arg("url", format!("{scheme}://{local}/mcp"))
                )
            );
        }
        eprintln!(
            "{}",
            tr(
                lang,
                &Msg::new("cli-daemon-listening")
                    .arg("socket", daemon.socket().display().to_string())
            )
        );
        daemon.run().await.map_err(|e| e.localize(lang))
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => fail(&msg),
    }
}

/// Proxy MCP por stdio hacia el daemon (D01): la identidad es la de
/// `--token`/`TASKCRAFT_AGENT_TOKEN` o `--agent` (D09).
fn mcp_stdio(db: &Path, global: &Global) -> ExitCode {
    let mut opts = connect_opts(db, global, "mcp-stdio");
    opts.lang = Lang::En;
    let backend = match mcp::RemoteBackend::connect(opts) {
        Ok(b) => Arc::new(b),
        Err(e) => return fail(&e.localize(global.lang)),
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return fail(&e.to_string()),
    };
    match runtime.block_on(mcp::serve_stdio(backend)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}
