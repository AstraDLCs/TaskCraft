// En Windows, la app no abre una consola (en depuración sí, para ver los logs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::Path;
use std::process::ExitCode;

use engine::Engine;
use i18n::{Lang, Localize, Msg};
use store_sqlite::SqliteStore;
use ui_egui::{Conn, TaskCraftApp};

/// `taskcraft [--workspace NOMBRE | --db RUTA] [CLAVE]`.
struct Launch {
    db: Option<std::path::PathBuf>,
    workspace: Option<String>,
    task: Option<String>,
}

fn launch_args() -> Launch {
    let mut out = Launch {
        db: None,
        workspace: None,
        task: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--workspace" | "-w" => out.workspace = args.next(),
            "--db" => out.db = args.next().map(Into::into),
            _ if a.starts_with("--workspace=") => {
                out.workspace = a.split_once('=').map(|(_, v)| v.to_owned());
            }
            _ if !a.starts_with('-') && out.task.is_none() => out.task = Some(a),
            _ => {}
        }
    }
    out
}

/// Abre otro workspace en una ventana nueva de la app (RF-ALM-08).
fn open_workspace(name: &str) -> Result<(), String> {
    if !store_sqlite::valid_workspace_name(name) {
        return Err(format!("invalid workspace name `{name}`"));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .arg("--workspace")
        .arg(name)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    engine::telemetry::init("info");
    let lang = Lang::detect();
    let launch = launch_args();
    // Mismo archivo que la CLI: `--db`, `--workspace`, TASKCRAFT_DB,
    // TASKCRAFT_WORKSPACE o el workspace `default`.
    let db = match (&launch.db, &launch.workspace) {
        (Some(db), _) => Ok(db.clone()),
        (None, Some(name)) => store_sqlite::workspace_path(name),
        (None, None) => store_sqlite::resolve_path(None),
    };
    let db = match db {
        Ok(p) => p,
        Err(e) => return open_failed(lang, &e.localize(lang)),
    };
    let current = db
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .unwrap_or(store_sqlite::DEFAULT_WORKSPACE)
        .to_owned();
    let (conn, events, warning, _lock) = match connect(&db, lang) {
        Ok(Started::Daemon(conn, events)) => (conn, Some(events), None, None),
        Ok(Started::Local(conn, why, lock)) => (conn, None, Some(why), Some(lock)),
        Err(e) => return open_failed(lang, &e),
    };
    let result = eframe::run_native(
        "TaskCraft",
        native_options(),
        Box::new(move |cc| {
            let saved = |key: &str| cc.storage.and_then(|s| s.get_string(key));
            let mut app = TaskCraftApp::new(conn, saved);
            app.set_workspaces(ui_egui::Workspaces {
                current,
                list: Box::new(|| {
                    store_sqlite::list_workspaces()
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect()
                }),
                open: Box::new(open_workspace),
            });
            let ctx = cc.egui_ctx.clone();
            app.set_waker(Box::new(move || ctx.request_repaint()));
            if let Some(why) = warning {
                app.set_status(Err(why));
            }
            // Cada escritura en el daemon despierta la ventana al instante (RNF-08).
            if let Some(opts) = events {
                let flag = app.notifier();
                let ctx = cc.egui_ctx.clone();
                let subscribed = client::subscribe(&opts, move |_| {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    ctx.request_repaint();
                    true
                });
                if let Err(e) = subscribed {
                    tracing::warn!(error = %e, "no live events; polling instead");
                }
            }
            // `taskcraft API-3` abre la app en esa tarea.
            if let Some(task) = &launch.task {
                app.open_task(task);
            }
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("taskcraft: {e}");
            ExitCode::FAILURE
        }
    }
}

enum Started {
    /// Escribe por el daemon y lee de una réplica; con las opciones para suscribirse.
    Daemon(Conn, client::Connect),
    /// Sin daemon: todo en este proceso, con el motivo para avisar al usuario
    /// y el lock del workspace, que se guarda mientras la app siga abierta.
    Local(Conn, String, std::fs::File),
}

/// Conecta con el daemon (arrancándolo si hace falta) y solo después abre la
/// réplica, para que el daemon haya migrado la base (D08). Si no hay daemon
/// posible, trabaja en proceso como antes.
fn connect(db: &Path, lang: Lang) -> Result<Started, String> {
    let opts = client::Connect {
        lang,
        ..client::Connect::new(db.to_path_buf(), "app")
    };
    let fallback = |why: String| -> Result<Started, String> {
        // Si otro proceso tiene la base (un daemon que no responde), abrirla
        // aquí la migraría bajo sus pies: mejor no arrancar.
        let lock = server::lock_workspace(db).map_err(|_| {
            let msg = Msg::new("ui-workspace-busy")
                .arg("db", db.display().to_string())
                .arg("reason", why.as_str());
            i18n::tr(lang, &msg)
        })?;
        let store = server::open_store(db).map_err(|e| e.localize(lang))?;
        let msg = Msg::new("ui-local-mode").arg("reason", why);
        let mut engine = Engine::new(store);
        if let Some(home) = server::home(db) {
            engine.set_home(home);
        }
        Ok(Started::Local(
            Conn::local(engine),
            i18n::tr(lang, &msg),
            lock,
        ))
    };
    let mut client = match client::Client::connect(&opts) {
        Ok(c) => c,
        Err(e) => return fallback(e.localize(lang)),
    };
    let mut replica = open_replica(db);
    if let Err(store::StoreError::OlderSchema { .. }) = replica {
        // Un daemon de una versión anterior: se sustituye por el de esta.
        tracing::info!("daemon runs an older schema; restarting it");
        drop(client);
        if let Err(e) = client::stop_daemon(db) {
            return fallback(e.localize(lang));
        }
        client = match client::Client::connect(&opts) {
            Ok(c) => c,
            Err(e) => return fallback(e.localize(lang)),
        };
        replica = open_replica(db);
    }
    match replica {
        Ok(replica) => {
            // Una segunda réplica para cargar en segundo plano (RNF-06).
            let path = db.to_path_buf();
            let open = move || open_replica(&path);
            let conn =
                Conn::remote(replica, Box::new(client)).with_background_loader(Box::new(open));
            Ok(Started::Daemon(conn, opts))
        }
        Err(e) => fallback(e.localize(lang)),
    }
}

/// Lectura sin escribir: la réplica SQLite de solo lectura (D08) o, en
/// PostgreSQL, otra conexión (D13).
fn open_replica(db: &Path) -> Result<Box<dyn store::Store>, store::StoreError> {
    match db.to_str().filter(|s| store_postgres::is_url(s)) {
        Some(url) => Ok(Box::new(store_postgres::PostgresStore::connect(url)?)),
        None => Ok(Box::new(SqliteStore::open_read_only(db)?)),
    }
}

fn open_failed(lang: Lang, error: &str) -> ExitCode {
    let msg = Msg::new("ui-open-failed").arg("error", error);
    eprintln!("taskcraft: {}", i18n::tr(lang, &msg));
    ExitCode::FAILURE
}

fn native_options() -> eframe::NativeOptions {
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("TaskCraft")
            // Igual que el `.desktop` y el icono (packaging/linux/common).
            .with_app_id("io.github.astradlcs.TaskCraft")
            .with_inner_size([1440.0, 860.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };

    // X11/XWayland por defecto (drag & drop fiable); Wayland nativo con TASKCRAFT_WAYLAND=1.
    // Backend gráfico: Vulkan con fallback a GL (default de egui-wgpu; WGPU_BACKEND lo fuerza).
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("TASKCRAFT_WAYLAND").is_some_and(|v| v == "1");
        options.event_loop_builder = Some(Box::new(move |builder| {
            use winit::platform::{
                wayland::EventLoopBuilderExtWayland, x11::EventLoopBuilderExtX11,
            };
            if wayland {
                builder.with_wayland();
            } else {
                builder.with_x11();
            }
        }));
    }

    options
}
