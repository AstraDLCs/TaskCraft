//! Lo propio del backend; la batería de conformidad completa es la del
//! engine con `TASKCRAFT_TEST_PG` (D13).

use store::Store;

use super::*;

/// URL del Postgres de pruebas; sin ella, estos tests no hacen nada.
fn url() -> Option<String> {
    std::env::var("TASKCRAFT_TEST_PG").ok()
}

fn isolated(url: &str) -> PostgresStore {
    let schema = format!(
        "t_{}",
        foundation::Ulid::generate().to_string().to_lowercase()
    );
    PostgresStore::connect_isolated(url, &schema).unwrap()
}

#[test]
fn migrates_once_and_counts_commits() {
    let Some(url) = url() else { return };
    let mut store = isolated(&url);
    let before = store.change_token().unwrap();
    let tx = store.begin().unwrap();
    tx.commit().unwrap();
    assert_eq!(store.change_token().unwrap(), before + 1);
    let ws = store.read().unwrap().workspace().unwrap();
    assert_eq!(ws.name, "postgres");
}

#[test]
fn urls_are_recognized() {
    assert!(is_url("postgres://u@h/db"));
    assert!(is_url("postgresql://h/db"));
    assert!(!is_url("/home/u/workspace.db"));
}
