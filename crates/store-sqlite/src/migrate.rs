//! Migraciones de esquema versionadas con `PRAGMA user_version` (RF-ALM-05).

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use store::{StoreError, StoreResult};

use crate::db;

/// Una entrada por versión; el índice + 1 es la versión que deja.
/// Nunca se edita una migración publicada: se añade una nueva.
pub(crate) const MIGRATIONS: &[&str] = &[
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
    include_str!("sql/v13.sql"),
    include_str!("sql/v14.sql"),
    include_str!("sql/v15.sql"),
    include_str!("sql/v16.sql"),
    include_str!("sql/v17.sql"),
    include_str!("sql/v18.sql"),
    include_str!("sql/v19.sql"),
    include_str!("sql/v20.sql"),
    include_str!("sql/v21.sql"),
];

/// Lleva la base a la última versión. Si ya tenía datos de una versión
/// anterior, primero guarda una copia junto al archivo y devuelve su ruta.
pub(crate) fn migrate(
    conn: &mut Connection,
    migrations: &[&str],
    db_path: Option<&Path>,
) -> StoreResult<Option<PathBuf>> {
    let current: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(db)?;
    let latest = u32::try_from(migrations.len())
        .map_err(|_| StoreError::Backend("too many migrations".into()))?;

    if current > latest {
        return Err(StoreError::NewerSchema {
            found: current,
            supported: latest,
        });
    }
    if current == latest {
        return Ok(None);
    }

    let backup = match db_path {
        Some(path) if current > 0 => Some(backup(conn, path, current)?),
        _ => None,
    };

    for (version, sql) in (1u32..).zip(migrations).skip_while(|(v, _)| *v <= current) {
        let tx = conn.transaction().map_err(db)?;
        tx.execute_batch(sql).map_err(db)?;
        tx.pragma_update(None, "user_version", version)
            .map_err(db)?;
        tx.commit().map_err(db)?;
        tracing::info!(version, "schema migrated");
    }
    Ok(backup)
}

fn backup(conn: &Connection, path: &Path, version: u32) -> StoreResult<PathBuf> {
    let stamp = jiff::Timestamp::now().as_second();
    let target = path.with_extension(format!("v{version}-{stamp}.bak"));
    let target_text = target
        .to_str()
        .ok_or_else(|| StoreError::Io(format!("non UTF-8 path: {}", target.display())))?;
    conn.execute("VACUUM INTO ?1", [target_text]).map_err(db)?;
    tracing::info!(path = %target.display(), "backup before migrating");
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::tempdir;

    fn version(conn: &Connection) -> u32 {
        conn.pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn fresh_database_reaches_latest_without_backup() {
        let mut conn = Connection::open_in_memory().unwrap();
        let backup = migrate(&mut conn, MIGRATIONS, None).unwrap();
        assert!(backup.is_none());
        assert_eq!(version(&conn) as usize, MIGRATIONS.len());
    }

    #[test]
    fn upgrade_makes_a_backup_first() {
        let dir = tempdir();
        let path = dir.join("workspace.db");
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, &MIGRATIONS[..1], Some(&path)).unwrap();

        let next = "CREATE TABLE extra (x INTEGER);";
        let all: Vec<&str> = MIGRATIONS.iter().copied().chain([next]).collect();
        let backup = migrate(&mut conn, &all, Some(&path)).unwrap().unwrap();
        assert!(backup.exists());
        assert_eq!(version(&conn) as usize, all.len());
        assert_eq!(version(&Connection::open(&backup).unwrap()), 1);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let err = migrate(&mut conn, MIGRATIONS, None).unwrap_err();
        assert!(matches!(err, StoreError::NewerSchema { found: 99, .. }));
    }
}
