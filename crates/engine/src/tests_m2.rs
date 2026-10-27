//! M2: tokens de agente (D09), búsqueda (RF-VIS-11), paginación (RF-MCP-08)
//! y alias para agentes (RF-MCP-04).

use super::T;

#[test]
fn register_returns_a_token_once_and_only_its_hash_is_kept() {
    let mut t = T::new();
    let agent = t.register(&[("name", "claude-1")]);
    let token = agent["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("tc_"));

    let shown = t.run("agent.get", &[("agent", "claude-1")]);
    assert!(shown.get("token").is_none(), "no se vuelve a mostrar");
    assert!(shown.get("token_hash").is_none());
    let history = t.run("activity.list", &[])["items"].to_string();
    assert!(!history.contains(&token[3..]), "ni en el historial");

    // D15: el token es de su interfaz, que actúa como su único agente.
    let found = t
        .engine
        .read(|r| crate::identity::from_token(r, &token))
        .unwrap();
    assert_eq!(
        found,
        model::Actor::Agent(agent["id"].as_str().unwrap().parse().unwrap())
    );
    assert_eq!(agent["interface"], "claude-1");

    // D22: el supervisor puede volver a copiarlo; nadie más.
    let copied = t.run("interface.token", &[("interface", "claude-1")]);
    assert_eq!(copied["token"], token.as_str());
    let shown = t.run("interface.get", &[("interface", "claude-1")]);
    assert!(shown.get("token").is_none());
    t.as_agent();
    let e = t.fail("interface.token", &[("interface", "claude-1")]);
    assert_eq!(e.code(), "FORBIDDEN");
}

#[test]
fn rotating_invalidates_the_previous_token() {
    let mut t = T::new();
    let old = t.register(&[("name", "claude-1")])["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let new = t.run("interface.rotate_token", &[("interface", "claude-1")])["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(old, new);
    assert_eq!(
        t.run("interface.token", &[("interface", "claude-1")])["token"],
        new.as_str(),
        "se guarda el nuevo"
    );
    let old_err = t
        .engine
        .read(|r| crate::identity::from_token(r, &old))
        .unwrap_err();
    assert_eq!(old_err.code(), "FORBIDDEN");
    assert!(
        t.engine
            .read(|r| crate::identity::from_token(r, &new))
            .is_ok()
    );

    // Sin agentes activos, el token solo identifica a la interfaz.
    t.run("agent.disable", &[("agent", "claude-1")]);
    let alone = t
        .engine
        .read(|r| crate::identity::from_token(r, &new))
        .unwrap();
    assert!(matches!(alone, model::Actor::Interface(_)));
    t.run("interface.disable", &[("interface", "claude-1")]);
    let disabled = t
        .engine
        .read(|r| crate::identity::from_token(r, &new))
        .unwrap_err();
    assert_eq!(disabled.code(), "FORBIDDEN");
}

#[test]
fn only_the_supervisor_rotates_tokens() {
    let mut t = T::new();
    t.register(&[("name", "claude-1")]);
    t.as_agent();
    let e = t.fail("interface.rotate_token", &[("interface", "claude-1")]);
    assert_eq!(e.code(), "FORBIDDEN");
}

#[test]
fn search_finds_title_description_and_comments() {
    let mut t = T::with_tasks(0);
    t.run(
        "task.create",
        &[("project", "API"), ("title", "Migración del esquema")],
    );
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "Otra"),
            ("description", "Revisar el **índice** de tareas"),
        ],
    );
    t.run("task.create", &[("project", "API"), ("title", "Tercera")]);
    t.run(
        "comment.add",
        &[("task", "API-3"), ("body", "Falta el benchmark de leases")],
    );

    let keys = |t: &mut T, q: &str| -> Vec<String> {
        t.run("task.search", &[("query", q)])["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["key"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        keys(&mut t, "migracion"),
        ["API-1"],
        "sin acentos y por prefijo"
    );
    assert_eq!(keys(&mut t, "indice tareas"), ["API-2"]);
    assert_eq!(keys(&mut t, "benchmark"), ["API-3"], "en comentarios");
    assert!(keys(&mut t, "\"); DROP TABLE task; --").is_empty());

    t.run(
        "task.update",
        &[("task", "API-1"), ("title", "Plan de copias")],
    );
    assert!(
        keys(&mut t, "migracion").is_empty(),
        "sigue al título nuevo"
    );
    t.run("task.delete", &[("task", "API-3")]);
    assert!(keys(&mut t, "benchmark").is_empty(), "la papelera no sale");
}

#[test]
fn lists_are_paginated_with_field_selection() {
    let mut t = T::with_tasks(7);
    let first = t.run("task.list", &[("limit", "3"), ("fields", "key,title")]);
    assert_eq!(first["count"], 3);
    assert_eq!(first["total"], 7);
    assert_eq!(
        first["items"][0],
        serde_json::json!({ "key": "API-1", "title": "Tarea 1" })
    );
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = t.run("task.list", &[("limit", "3"), ("cursor", &cursor)]);
    assert_eq!(second["items"][0]["key"], "API-4");
    let cursor = second["next_cursor"].as_str().unwrap().to_owned();
    let last = t.run("task.list", &[("limit", "3"), ("cursor", &cursor)]);
    assert_eq!(last["count"], 1);
    assert!(last["next_cursor"].is_null());

    let activity = t.run("activity.list", &[("limit", "2"), ("fields", "command")]);
    assert_eq!(
        activity["items"][0],
        serde_json::json!({ "command": "task.create" })
    );
    let before = activity["next_cursor"].as_str().unwrap().to_owned();
    let older = t.run("activity.list", &[("limit", "50"), ("before", &before)]);
    assert_eq!(older["count"], 6, "8 entradas en total, 2 ya vistas");
}

#[test]
fn task_check_marks_steps_and_criteria() {
    let mut t = T::with_tasks(1);
    t.run("checklist.add", &[("task", "API-1"), ("text", "paso")]);
    t.run("criteria.add", &[("task", "API-1"), ("text", "criterio")]);
    let after = t.run(
        "task.check",
        &[("task", "API-1"), ("kind", "criteria"), ("item", "1")],
    );
    assert_eq!(after["criteria"][0]["done"], true);
    assert_eq!(after["checklist"][0]["done"], false);
    let e = t.fail(
        "task.check",
        &[("task", "API-1"), ("kind", "steps"), ("item", "1")],
    );
    assert_eq!(e.key(), "arg-one-of");
}
