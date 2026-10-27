//! L6 — Daemon: único dueño del engine de un workspace (D01, D08).
//!
//! Escucha en un socket Unix privado, atribuye cada conexión a un actor
//! (D09), ejecuta los comandos sobre un único [`SharedEngine`], publica un
//! evento por cada escritura y corre las tareas periódicas del actor System.

mod conn;
mod periodic;
mod transport;
pub mod webhooks;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use engine::{Engine, SharedEngine};
use i18n::{Localize, Msg};
use store::StoreError;
use store_sqlite::SqliteStore;
use tokio::sync::{broadcast, watch};

/// Variable de entorno que exige token también por el socket local (D09).
pub const REQUIRE_TOKEN_ENV: &str = "TASKCRAFT_REQUIRE_TOKEN";

pub struct Options {
    pub db: PathBuf,
    /// Se detiene solo tras este tiempo sin clientes (autoarranque).
    pub idle_exit: Option<Duration>,
}

#[derive(Debug)]
pub enum ServeError {
    /// Otro daemon ya tiene el workspace.
    AlreadyRunning(PathBuf),
    Store(StoreError),
    Io(String),
}

impl Localize for ServeError {
    fn msg(&self) -> Msg {
        match self {
            Self::AlreadyRunning(db) => {
                Msg::new("serve-already-running").arg("db", db.display().to_string())
            }
            Self::Store(e) => e.msg(),
            Self::Io(detail) => Msg::new("serve-io").arg("detail", detail),
        }
    }
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.msg().fmt(f)
    }
}

impl std::error::Error for ServeError {}

pub(crate) fn io(context: &Path, e: std::io::Error) -> ServeError {
    ServeError::Io(format!("{}: {e}", context.display()))
}

/// Estado compartido por todas las conexiones.
/// Abre el workspace: una ruta es SQLite; `postgres://…`, PostgreSQL (D13).
pub fn open_store(db: &Path) -> Result<Box<dyn store::Store>, store::StoreError> {
    match db.to_str().filter(|s| store_postgres::is_url(s)) {
        Some(url) => Ok(Box::new(store_postgres::PostgresStore::connect(url)?)),
        None => Ok(Box::new(SqliteStore::open(db)?)),
    }
}

/// El engine del workspace, listo para trabajar: con su carpeta de copias y
/// el recorrido de las tareas antiguas reconstruido (D24). Lo usan el daemon
/// y los modos sin daemon de la app y de la CLI.
pub fn open_engine(db: &Path) -> Result<Engine, store::StoreError> {
    let mut engine = Engine::new(open_store(db)?);
    if let Some(home) = home(db) {
        engine.set_home(home);
    }
    // Si falla, se sigue sin él: lo notarán las métricas, no el trabajo.
    if let Err(e) = engine.backfill_transitions() {
        tracing::warn!(error = %e, "task journeys not rebuilt");
    }
    Ok(engine)
}

/// Carpeta del workspace (copias de seguridad): la de su archivo o, en
/// PostgreSQL, `<datos>/taskcraft/postgres/<base>`.
pub fn home(db: &Path) -> Option<PathBuf> {
    if protocol::is_url(db) {
        let name = db.file_name()?.to_str()?.split('?').next()?.to_owned();
        return store_sqlite::workspaces_root()
            .ok()
            .map(|root| root.join("postgres").join(name));
    }
    db.parent().map(Path::to_path_buf)
}

#[derive(Clone)]
pub(crate) struct Shared {
    pub engine: SharedEngine,
    pub events: broadcast::Sender<u64>,
    pub shutdown: watch::Sender<bool>,
    pub clients: Arc<AtomicUsize>,
    pub require_token: bool,
}

/// Un daemon abierto (con el lock y la base), listo para [`Daemon::run`].
pub struct Daemon {
    shared: Shared,
    socket: PathBuf,
    idle_exit: Option<Duration>,
    // Mientras viva, ningún otro daemon abre este workspace.
    _lock: File,
}

/// Toma el lock exclusivo del workspace: solo un proceso escribe en la base
/// (el daemon o, sin él, la app en modo local). Se suelta al soltar el archivo.
pub fn lock_workspace(db: &Path) -> Result<File, ServeError> {
    let dir = if protocol::is_url(db) {
        Some(protocol::runtime_dir())
    } else {
        db.parent().map(Path::to_path_buf)
    };
    if let Some(dir) = dir {
        std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    }
    let lock_path = protocol::lock_path(db);
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| io(&lock_path, e))?;
    if lock.try_lock().is_err() {
        return Err(ServeError::AlreadyRunning(db.to_path_buf()));
    }
    Ok(lock)
}

impl Daemon {
    /// Toma el lock del workspace y abre (migrando si hace falta) la base.
    pub fn open(opts: Options) -> Result<Self, ServeError> {
        let lock = lock_workspace(&opts.db)?;
        let engine = open_engine(&opts.db).map_err(ServeError::Store)?;
        let engine = SharedEngine::new(engine);
        let (events, _) = broadcast::channel(256);
        let sender = events.clone();
        engine.set_on_commit(Box::new(move |token| {
            // Sin suscriptores no es un error: nadie escucha todavía.
            let _ = sender.send(token);
        }));
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            shared: Shared {
                engine,
                events,
                shutdown,
                clients: Arc::new(AtomicUsize::new(0)),
                require_token: std::env::var(REQUIRE_TOKEN_ENV).is_ok_and(|v| v == "1"),
            },
            socket: protocol::socket_path(&opts.db),
            idle_exit: opts.idle_exit,
            _lock: lock,
        })
    }

    /// El engine del daemon, para servicios que corren en el mismo proceso (MCP HTTP).
    pub fn engine(&self) -> SharedEngine {
        self.shared.engine.clone()
    }

    /// Cambia a `true` cuando el daemon se detiene.
    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shared.shutdown.subscribe()
    }

    /// Escrituras confirmadas (el `change_token` nuevo de cada una).
    pub fn events(&self) -> broadcast::Receiver<u64> {
        self.shared.events.subscribe()
    }

    /// El emisor de esos avisos, para quien necesite suscribirse más tarde (D14).
    pub fn changes(&self) -> broadcast::Sender<u64> {
        self.shared.events.clone()
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Atiende conexiones hasta `Shutdown`, una señal o el tiempo sin clientes.
    pub async fn run(self) -> Result<(), ServeError> {
        let mut listener = transport::Listener::bind(&self.socket)?;
        tracing::info!(socket = %self.socket.display(), "daemon listening");
        let mut shutdown = self.shared.shutdown.subscribe();
        periodic::spawn(self.shared.clone());
        webhooks::spawn(self.shared.clone());
        if let Some(idle) = self.idle_exit {
            spawn_idle_exit(self.shared.clone(), idle);
        }
        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok(stream) => {
                        tokio::spawn(conn::handle(stream, self.shared.clone()));
                    }
                    Err(e) => tracing::warn!(error = %e, "accept failed"),
                },
                _ = shutdown.changed() => break,
                _ = tokio::signal::ctrl_c() => {
                    let _ = self.shared.shutdown.send(true);
                    break;
                }
            }
        }
        transport::Listener::remove(&self.socket);
        tracing::info!("daemon stopped");
        Ok(())
    }
}

fn spawn_idle_exit(shared: Shared, idle: Duration) {
    tokio::spawn(async move {
        let step = Duration::from_secs(1).min(idle);
        let mut empty_for = Duration::ZERO;
        loop {
            tokio::time::sleep(step).await;
            if shared.clients.load(Ordering::SeqCst) == 0 {
                empty_for += step;
                if empty_for >= idle {
                    tracing::info!("no clients; stopping");
                    let _ = shared.shutdown.send(true);
                    return;
                }
            } else {
                empty_for = Duration::ZERO;
            }
        }
    });
}

#[cfg(test)]
mod tests;
