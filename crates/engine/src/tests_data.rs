//! D23: gestión de la información. Eliminar un proyecto, vaciar el workspace
//! y podar el historial, siempre con confirmación y copia previa.

use model::AgentId;

use super::T;

/// Carpeta temporal para las copias.
fn dir() -> String {
    let d = std::env::temp_dir().join(format!("taskcraft-data-{}", AgentId::generate()));
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

/// Proyectos API (2 tareas, comentario, ciclo, épica, diagrama) y WEB (1).
fn workspace() -> T {
    let mut t = T::with_tasks(2);
    t.run("comment.add", &[("task", "API-1"), ("body", "nota")]);
    t.run("cycle.create", &[("project", "API")]);
    t.run("epic.create", &[("project", "API"), ("name", "Pagos")]);
    t.run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "F"),
            ("source", "flowchart TD\n A-->B"),
        ],
    );
    t.run("project.create", &[("key", "web")]);
    t.run("task.create", &[("project", "WEB"), ("title", "Login")]);
    t
}

fn copies(dir: &str) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains("antes-de-borrar-"))
        .collect()
}

#[test]
fn deleting_a_project_needs_its_key_and_saves_a_copy_first() {
    let mut t = workspace();
    let dir = dir();
    let e = t.fail(
        "project.delete",
        &[("project", "API"), ("confirm", "WEB"), ("dir", &dir)],
    );
    assert_eq!(e.key(), "data-confirm-mismatch");
    assert!(copies(&dir).is_empty(), "sin confirmar no copia ni borra");

    let out = t.run(
        "project.delete",
        &[("project", "API"), ("confirm", "api"), ("dir", &dir)],
    );
    assert_eq!(out["deleted"], "API");
    assert_eq!(
        t.fail("project.get", &[("project", "API")]).code(),
        "NOT_FOUND"
    );
    assert_eq!(t.fail("task.get", &[("task", "API-1")]).code(), "NOT_FOUND");
    assert_eq!(t.run("diagram.list", &[])["count"], 0);
    assert_eq!(t.keys(&[("project", "WEB")]), ["WEB-1"], "lo demás sigue");

    // La copia tiene todo lo que había y se puede importar.
    let copy = copies(&dir);
    assert_eq!(copy.len(), 1);
    let path = copy[0].to_string_lossy().into_owned();
    assert_eq!(out["backup"], path.as_str());
    let mut other = T::new();
    other.run("workspace.import", &[("path", &path)]);
    assert_eq!(
        other.run("project.get", &[("project", "API")])["key"],
        "API"
    );
    assert_eq!(
        other.run("diagram.get", &[("diagram", "API-D1")])["title"],
        "F"
    );

    // Su historial se fue con él; queda la entrada del borrado.
    let history = t.run("activity.list", &[])["items"].to_string();
    assert!(history.contains("project.delete"), "{history}");
    assert!(!history.contains("comment.add"), "{history}");
}

#[test]
fn wiping_the_workspace_keeps_only_its_name_and_settings() {
    let mut t = workspace();
    t.register(&[("name", "claude-1")]);
    t.run("label.create", &[("name", "rust")]);
    t.run("workspace.configure", &[("trash_days", "7")]);
    let dir = dir();
    let name = t.run("workspace.get", &[])["name"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        t.fail("workspace.wipe", &[("confirm", "otro"), ("dir", &dir)])
            .key(),
        "data-confirm-mismatch"
    );
    t.run("workspace.wipe", &[("confirm", &name), ("dir", &dir)]);
    assert_eq!(copies(&dir).len(), 1);
    assert_eq!(t.run("project.list", &[("archived", "true")])["count"], 0);
    assert_eq!(t.run("agent.list", &[])["items"], serde_json::json!([]));
    assert_eq!(t.run("interface.list", &[])["count"], 0);
    assert_eq!(t.run("label.list", &[])["total"], 0);
    let ws = t.run("workspace.get", &[]);
    assert_eq!(ws["name"], name.as_str());
    assert_eq!(ws["settings"]["trash_days"], 7);
    let history = t.run("activity.list", &[])["items"].clone();
    assert_eq!(history.as_array().unwrap().len(), 1, "{history}");
    // Y se puede volver a empezar.
    t.run("project.create", &[("key", "api")]);
    assert_eq!(
        t.run("task.create", &[("project", "API"), ("title", "x")])["key"],
        "API-1"
    );
}

#[test]
fn pruning_history_removes_only_older_entries() {
    let mut t = workspace();
    let dir = dir();
    let e = t.fail("activity.prune", &[("before", "2999-01-01"), ("dir", &dir)]);
    assert_eq!(e.key(), "data-prune-future");
    let today = jiff::Zoned::now().date().to_string();
    let none = t.run("activity.prune", &[("before", "2000-01-01"), ("dir", &dir)]);
    assert_eq!(none["deleted"], 0);
    let out = t.run("activity.prune", &[("before", &today), ("dir", &dir)]);
    assert!(!copies(&dir).is_empty());
    // Lo de hoy se queda; el resultado dice cuánto se borró.
    assert!(out["deleted"].as_u64().is_some());
    let history = t.run("activity.list", &[])["items"].to_string();
    assert!(history.contains("activity.prune"), "{history}");
}

/// Varias eliminaciones seguidas (el mismo segundo) dejan cada una su copia.
#[test]
fn safety_copies_are_never_overwritten() {
    let mut t = workspace();
    let dir = dir();
    t.clock = Some("2026-10-27T12:00:00Z".parse().unwrap());
    t.run(
        "project.delete",
        &[("project", "API"), ("confirm", "API"), ("dir", &dir)],
    );
    t.run("activity.prune", &[("before", "2026-01-01"), ("dir", &dir)]);
    t.run("trash.purge", &[("backup", "true"), ("dir", &dir)]);
    assert_eq!(copies(&dir).len(), 3);
}

#[test]
fn only_the_supervisor_deletes_information() {
    let mut t = workspace();
    let dir = dir();
    t.as_agent();
    for (cmd, args) in [
        (
            "project.delete",
            vec![("project", "API"), ("confirm", "API")],
        ),
        ("workspace.wipe", vec![("confirm", "x")]),
        ("activity.prune", vec![("before", "2000-01-01")]),
    ] {
        let mut args = args;
        args.push(("dir", &dir));
        assert_eq!(t.fail(cmd, &args).code(), "FORBIDDEN", "{cmd}");
    }
}

#[test]
fn emptying_the_trash_from_the_app_saves_a_copy() {
    let mut t = workspace();
    let dir = dir();
    t.run("task.delete", &[("task", "API-2")]);
    let out = t.run(
        "trash.purge",
        &[("days", "0"), ("backup", "true"), ("dir", &dir)],
    );
    assert_eq!(out["purged"], 1);
    assert!(out["backup"].as_str().is_some());
    assert_eq!(copies(&dir).len(), 1);
}
