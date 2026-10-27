//! L3 — Backend PostgreSQL (D13): el mismo modelo que `store-sqlite`, con el
//! cliente síncrono `postgres`. Se elige con una cadena de conexión
//! `postgres://…` donde se acepta la ruta de un workspace.

mod pg;
mod rows;
mod tx;

use std::cell::RefCell;

use model::{Settings, Timestamp, WorkspaceId};
use postgres::{Client, IsolationLevel};
use store::{Reader, Store, StoreError, StoreResult, Transaction};

use crate::pg::p;
use crate::tx::PgTx;

/// Migraciones; el índice + 1 es la versión que deja. Nunca se edita una
/// publicada: se añade otra.
const MIGRATIONS: &[&str] = &[
    include_str!("sql/v1.sql"),
    include_str!("sql/v2.sql"),
    include_str!("sql/v3.sql"),
    include_str!("sql/v4.sql"),
    include_str!("sql/v5.sql"),
    include_str!("sql/v6.sql"),
    include_str!("sql/v7.sql"),
    include_str!("sql/v8.sql"),
    include_str!("sql/v9.sql"),
    include_str!("sql/v10.sql"),
    include_str!("sql/v11.sql"),
    include_str!("sql/v12.sql"),
];

/// ¿Es una cadena de conexión de PostgreSQL?
pub fn is_url(text: &str) -> bool {
    text.starts_with("postgres://") || text.starts_with("postgresql://")
}

/// `RefCell`: `change_token` lee con `&self` y el cliente pide `&mut`.
/// `Option`: al soltarlo, el cliente se cierra en otro hilo (ver `Drop`).
pub struct PostgresStore {
    client: RefCell<Option<Client>>,
}

/// El cliente síncrono bloquea en su propio runtime: no puede hacerlo desde
/// un hilo que ya mueve tareas de tokio (el daemon, D08). Conectar y cerrar
/// van en un hilo aparte; el resto lo llama el engine desde el pool de bloqueo.
fn off_runtime<T: Send>(f: impl FnOnce() -> T + Send) -> StoreResult<T> {
    std::thread::scope(|s| {
        s.spawn(f)
            .join()
            .map_err(|_| StoreError::Backend("postgres thread panicked".into()))
    })
}

impl Drop for PostgresStore {
    fn drop(&mut self) {
        if let Some(client) = self.client.get_mut().take() {
            let _ = off_runtime(move || drop(client));
        }
    }
}

impl PostgresStore {
    /// Conecta, aplica las migraciones pendientes y crea el workspace si no hay.
    pub fn connect(url: &str) -> StoreResult<Self> {
        Self::open(url, None)
    }

    /// Workspace aislado en su propio esquema (tests de conformidad, D13).
    pub fn connect_isolated(url: &str, schema: &str) -> StoreResult<Self> {
        Self::open(url, Some(schema))
    }

    fn open(url: &str, schema: Option<&str>) -> StoreResult<Self> {
        let client = off_runtime(|| Self::prepare(url, schema))??;
        Ok(Self {
            client: RefCell::new(Some(client)),
        })
    }

    fn prepare(url: &str, schema: Option<&str>) -> StoreResult<Client> {
        let mut client = Client::connect(url, tls()?).map_err(db)?;
        if let Some(schema) = schema {
            if !schema
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(StoreError::Backend(format!("invalid schema `{schema}`")));
            }
            client
                .batch_execute(&format!(
                    "CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}"
                ))
                .map_err(db)?;
        }
        migrate(&mut client)?;
        ensure_workspace(&mut client)?;
        Ok(client)
    }

    fn client(&mut self) -> StoreResult<&mut Client> {
        self.client
            .get_mut()
            .as_mut()
            .ok_or_else(|| StoreError::Backend("postgres connection closed".into()))
    }
}

/// TLS con rustls y las raíces de webpki (D13). `sslmode` de la URL decide:
/// `prefer` (por defecto) lo usa si el servidor lo ofrece, `require` lo exige
/// y `disable` no lo intenta.
fn tls() -> StoreResult<tokio_postgres_rustls::MakeRustlsConnect> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| StoreError::Backend(format!("tls: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

fn migrate(client: &mut Client) -> StoreResult<()> {
    let mut tx = client.transaction().map_err(db)?;
    // Una sola migración a la vez aunque arranquen dos daemons.
    tx.batch_execute(
        "SELECT pg_advisory_xact_lock(hashtext(current_schema() || ':migrate'));
         CREATE TABLE IF NOT EXISTS schema_version (version BIGINT NOT NULL)",
    )
    .map_err(db)?;
    let current: i64 = tx
        .query_opt("SELECT version FROM schema_version", &[])
        .map_err(db)?
        .map(|r| r.get(0))
        .unwrap_or(0);
    let supported = i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX);
    if current > supported {
        return Err(StoreError::NewerSchema {
            found: u32::try_from(current).unwrap_or(u32::MAX),
            supported: u32::try_from(supported).unwrap_or(u32::MAX),
        });
    }
    for (version, sql) in (1i64..)
        .zip(MIGRATIONS)
        .skip(usize::try_from(current).unwrap_or(0))
    {
        tx.batch_execute(sql).map_err(db)?;
        tx.execute("DELETE FROM schema_version", &[]).map_err(db)?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES ($1)",
            &[&version],
        )
        .map_err(db)?;
        tracing::info!(version, "postgres schema migrated");
    }
    tx.commit().map_err(db)
}

fn ensure_workspace(client: &mut Client) -> StoreResult<()> {
    let defaults = Settings::default();
    let params = p![
        WorkspaceId::generate().to_string(),
        "postgres",
        Timestamp::now().to_string(),
        defaults.max_depth,
        defaults.trash_days,
    ];
    client
        .execute(
            "INSERT INTO workspace (id, name, created_at, max_depth, trash_days)
             SELECT $1::text, $2::text, $3::text, $4::bigint, $5::bigint
             WHERE NOT EXISTS (SELECT 1 FROM workspace)",
            &pg::refs(&params),
        )
        .map_err(db)?;
    Ok(())
}

impl Store for PostgresStore {
    fn begin(&mut self) -> StoreResult<Box<dyn Transaction + '_>> {
        let mut tx = self.client()?.transaction().map_err(db)?;
        // Un escritor a la vez por workspace, como en SQLite (D13).
        tx.batch_execute("SELECT pg_advisory_xact_lock(hashtext(current_schema()))")
            .map_err(db)?;
        Ok(Box::new(PgTx::new(tx, true)))
    }

    fn read(&mut self) -> StoreResult<Box<dyn Reader + '_>> {
        let tx = self
            .client()?
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .map_err(db)?;
        Ok(Box::new(PgTx::new(tx, false)))
    }

    fn change_token(&self) -> StoreResult<u64> {
        let mut client = self.client.borrow_mut();
        let client = client
            .as_mut()
            .ok_or_else(|| StoreError::Backend("postgres connection closed".into()))?;
        let row = client
            .query_one("SELECT value FROM change_counter WHERE id = 1", &[])
            .map_err(db)?;
        let value: i64 = row.get(0);
        Ok(u64::try_from(value).unwrap_or(0))
    }
}

pub(crate) fn db(e: postgres::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

#[cfg(test)]
mod tests;
