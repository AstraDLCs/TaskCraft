//! L3 — Backend SQLite (D02): un archivo por workspace, modo WAL.

mod migrate;
mod rows;
mod tx;

use std::path::{Path, PathBuf};
use std::time::Duration;

use model::{Settings, Timestamp, WorkspaceId};
use rusqlite::{Connection, TransactionBehavior};
use store::{Reader, Store, StoreError, StoreResult, Transaction};

use crate::tx::SqliteTx;

/// Variable de entorno que fuerza la ruta de la base de datos.
pub const DB_ENV: &str = "TASKCRAFT_DB";

/// Variable de entorno con el nombre del workspace (RF-ALM-08).
pub const WORKSPACE_ENV: &str = "TASKCRAFT_WORKSPACE";

/// Workspace que se abre si no se elige otro.
pub const DEFAULT_WORKSPACE: &str = "default";

/// `<datos del usuario>/taskcraft`: donde viven los workspaces locales.
pub fn workspaces_root() -> StoreResult<PathBuf> {
    dirs::data_dir()
        .map(|d| d.join("taskcraft"))
        .ok_or_else(|| StoreError::Io("user data directory not found".into()))
}

/// ¿Sirve como nombre de workspace? Minúsculas, cifras, `-` y `_`.
pub fn valid_workspace_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// `<datos del usuario>/taskcraft/<nombre>/workspace.db` (RF-ALM-08).
pub fn workspace_path(name: &str) -> StoreResult<PathBuf> {
    if !valid_workspace_name(name) {
        return Err(StoreError::Io(format!("invalid workspace name `{name}`")));
    }
    Ok(workspaces_root()?.join(name).join("workspace.db"))
}

/// Workspaces que ya existen en el directorio de datos, por nombre.
pub fn list_workspaces() -> Vec<(String, PathBuf)> {
    let Ok(root) = workspaces_root() else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            let db = e.path().join("workspace.db");
            (valid_workspace_name(&name) && db.is_file()).then_some((name, db))
        })
        .collect();
    out.sort();
    out
}

/// Ruta de la base: la explícita, o `TASKCRAFT_DB`, o la del workspace
/// `TASKCRAFT_WORKSPACE`, o `<datos del usuario>/taskcraft/default/workspace.db`
/// (XDG en Linux, `%APPDATA%` en Windows).
pub fn resolve_path(explicit: Option<PathBuf>) -> StoreResult<PathBuf> {
    if let Some(p) = explicit.or_else(|| std::env::var_os(DB_ENV).map(PathBuf::from)) {
        return Ok(p);
    }
    let name = std::env::var(WORKSPACE_ENV).unwrap_or_else(|_| DEFAULT_WORKSPACE.to_owned());
    workspace_path(name.trim())
}

pub struct SqliteStore {
    conn: Connection,
    /// Escrituras confirmadas por esta conexión; `data_version` solo ve las ajenas.
    commits: u64,
}

impl SqliteStore {
    /// Abre (o crea) el workspace en `path`, aplicando las migraciones pendientes.
    pub fn open(path: &Path) -> StoreResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| StoreError::Io(format!("{}: {e}", dir.display())))?;
        }
        let mut conn = Connection::open(path).map_err(db)?;
        configure(&conn)?;
        migrate::migrate(&mut conn, migrate::MIGRATIONS, Some(path))?;
        let name = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            .unwrap_or("default");
        ensure_workspace(&conn, name)?;
        Ok(Self { conn, commits: 0 })
    }

    /// Réplica de solo lectura de un workspace que otro proceso (el daemon)
    /// ya abrió y migró (D08). Rechaza esquemas de otra versión.
    pub fn open_read_only(path: &Path) -> StoreResult<Self> {
        use rusqlite::OpenFlags;
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db)?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(db)?;
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db)?;
        let supported = u32::try_from(migrate::MIGRATIONS.len()).unwrap_or(u32::MAX);
        if version > supported {
            return Err(StoreError::NewerSchema {
                found: version,
                supported,
            });
        }
        if version < supported {
            return Err(StoreError::OlderSchema {
                found: version,
                supported,
            });
        }
        Ok(Self { conn, commits: 0 })
    }

    /// Workspace efímero, para tests.
    pub fn open_in_memory() -> StoreResult<Self> {
        let mut conn = Connection::open_in_memory().map_err(db)?;
        configure(&conn)?;
        migrate::migrate(&mut conn, migrate::MIGRATIONS, None)?;
        ensure_workspace(&conn, "memoria")?;
        Ok(Self { conn, commits: 0 })
    }
}

impl Store for SqliteStore {
    fn begin(&mut self) -> StoreResult<Box<dyn Transaction + '_>> {
        let Self { conn, commits } = self;
        // IMMEDIATE toma el lock de escritura al empezar: evita el interbloqueo
        // de dos lectores que intentan pasar a escritores a la vez.
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db)?;
        Ok(Box::new(SqliteTx::new(tx, Some(commits))))
    }

    fn read(&mut self) -> StoreResult<Box<dyn Reader + '_>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(db)?;
        Ok(Box::new(SqliteTx::new(tx, None)))
    }

    fn change_token(&self) -> StoreResult<u64> {
        let others: u64 = self
            .conn
            .pragma_query_value(None, "data_version", |r| r.get(0))
            .map_err(db)?;
        Ok(others.wrapping_add(self.commits))
    }
}

fn configure(conn: &Connection) -> StoreResult<()> {
    let mode: String = conn
        .pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))
        .map_err(db)?;
    // Las bases en memoria no admiten WAL y responden "memory".
    if mode != "wal" && mode != "memory" {
        return Err(StoreError::Backend(format!(
            "could not enable WAL (mode {mode})"
        )));
    }
    // FULL: un commit confirmado sobrevive a un corte de luz (RNF-05).
    conn.pragma_update(None, "synchronous", "FULL")
        .map_err(db)?;
    conn.pragma_update(None, "foreign_keys", "ON").map_err(db)?;
    conn.busy_timeout(Duration::from_secs(5)).map_err(db)?;
    Ok(())
}

fn ensure_workspace(conn: &Connection, name: &str) -> StoreResult<()> {
    let defaults = Settings::default();
    conn.execute(
        "INSERT INTO workspace (id, name, created_at, max_depth, trash_days)
         SELECT ?1, ?2, ?3, ?4, ?5 WHERE NOT EXISTS (SELECT 1 FROM workspace)",
        rusqlite::params![
            WorkspaceId::generate().to_string(),
            name,
            Timestamp::now().to_string(),
            defaults.max_depth,
            defaults.trash_days,
        ],
    )
    .map_err(db)?;
    Ok(())
}

pub(crate) fn db(e: rusqlite::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;

    pub fn tempdir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("taskcraft-test-{}", foundation::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod tests;
