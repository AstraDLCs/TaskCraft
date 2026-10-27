//! Exportar e importar `.taskcraft` (RF-EXP-01…05, 09) contra workspaces reales.

use model::{Actor, AgentId};

use super::T;

fn temp_file(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("taskcraft-exp-{}", AgentId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

/// Workspace con un poco de todo: subtareas, etiquetas, dependencias,
/// checklist, criterios, comentarios, un agente con claim, archivada,
/// papelera y bandeja.
fn rich() -> T {
    let mut t = T::with_tasks(4);
    t.run("project.create", &[("key", "web"), ("name", "Front")]);
    t.run(
        "task.create",
        &[
            ("parent", "API-1"),
            ("title", "Hija"),
            ("labels", "db,rust"),
        ],
    );
    t.run("task.create", &[("project", "WEB"), ("title", "Login")]);
    t.run("task.link", &[("blocker", "API-1"), ("blocked", "API-2")]);
    t.run("checklist.add", &[("task", "API-1"), ("text", "paso")]);
    t.run("criteria.add", &[("task", "API-1"), ("text", "criterio")]);
    t.run("comment.add", &[("task", "API-2"), ("body", "nota **md**")]);
    t.run("task.move", &[("task", "API-3"), ("status", "cancelled")]);
    t.run("task.archive", &[("task", "API-3")]);
    t.run("task.delete", &[("task", "API-4")]);
    t.run("inbox.add", &[("title", "Idea suelta")]);
    // M5: flujo, estimación, campos, épica, adjunto, relación, plantilla y mención.
    t.run(
        "status.create",
        &[("project", "API"), ("name", "QA"), ("base", "in_review")],
    );
    t.run(
        "field.create",
        &[("project", "API"), ("name", "Cliente"), ("kind", "text")],
    );
    t.run(
        "task.set_field",
        &[("task", "API-1"), ("field", "cliente"), ("value", "Acme")],
    );
    t.run("task.update", &[("task", "API-1"), ("estimate", "2.5")]);
    t.run("epic.create", &[("project", "API"), ("name", "Pagos")]);
    t.run("task.set_epic", &[("task", "API-2"), ("epic", "pagos")]);
    t.run(
        "attachment.add",
        &[("task", "API-2"), ("name", "log.txt"), ("data", "aG9sYQ==")],
    );
    t.run("task.relate", &[("task", "API-1"), ("other", "API-2")]);
    t.run("template.from_task", &[("task", "API-1"), ("name", "Rel")]);
    t.run(
        "view.save",
        &[
            ("name", "Urgentes"),
            ("filter", "priority:urgent"),
            ("project", "API"),
        ],
    );
    // M15: un diagrama citado e incrustado.
    t.run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Flujo"),
            ("source", "flowchart TD\n  A --> B"),
        ],
    );
    t.run(
        "task.update",
        &[
            ("task", "API-2"),
            ("description", "Ver [[API-1]]\n\n![[API-D1]]"),
        ],
    );
    let agent = t.register(&[("name", "claude-1")]);
    t.actor = Actor::Agent(agent["id"].as_str().unwrap().parse().unwrap());
    t.run("task.claim", &[("task", "WEB-1")]);
    t.run("comment.add", &[("task", "WEB-1"), ("body", "empiezo")]);
    t.run(
        "cost.report",
        &[("task", "WEB-1"), ("tokens_in", "1200"), ("amount", "0.03")],
    );
    t.actor = Actor::Supervisor;
    t.run(
        "cycle.create",
        &[("project", "WEB"), ("start", "2026-01-05")],
    );
    t.run(
        "task.set_cycle",
        &[("task", "WEB-1"), ("cycle", "Sprint 1")],
    );
    t.actor = Actor::Agent(agent["id"].as_str().unwrap().parse().unwrap());
    t.run(
        "git.ingest",
        &[
            ("sha", "0123456789abcdef0123456789abcdef01234567"),
            ("message", "Toca WEB-1"),
            ("branch", "feat/WEB-1-x"),
        ],
    );
    t.actor = Actor::Supervisor;
    t
}

fn dump(t: &mut T) -> exchange::Dump {
    t.engine.read(|r| Ok(exchange::collect(r, None)?)).unwrap()
}

#[test]
fn export_then_import_into_an_empty_workspace_is_lossless() {
    let mut src = rich();
    let path = temp_file("todo.taskcraft");
    let out = src.run("workspace.export", &[("path", &path)]);
    assert_eq!(out["tasks"], 6);

    let mut dst = T::new();
    let preview = dst.run("workspace.import_preview", &[("path", &path)]);
    assert_eq!(preview["can_import"], true, "{preview}");
    assert_eq!(preview["creates"]["tasks"], 6);
    dst.run("workspace.import", &[("path", &path)]);

    let a = dump(&mut src);
    let mut b = dump(&mut dst);
    // El historial de destino tiene además la entrada "importado".
    let last = b.activity.pop().unwrap();
    assert_eq!(last.command, "workspace.import");
    // El workspace de destino conserva su propio ID y fecha de creación.
    b.workspace.id = a.workspace.id;
    b.workspace.created_at = a.workspace.created_at;
    assert_eq!(a.views.len(), 1);
    assert_eq!(a, b, "ida y vuelta sin pérdida (RF-EXP-02)");

    // Las menciones se recalculan en el destino.
    assert_eq!(
        dst.run("task.get", &[("task", "API-1")])["mentioned_by"][0],
        "API-2"
    );
    // Y sigue funcionando: búsqueda (FTS) y claves nuevas.
    let found = dst.run("task.search", &[("query", "nota")]);
    assert_eq!(found["items"][0]["key"], "API-2");
    let created = dst.run("task.create", &[("project", "API"), ("title", "nueva")]);
    assert_eq!(created["key"], "API-6");
    // Los diagramas viajan con su número y sus citas se recalculan.
    let diagram = dst.run("diagram.get", &[("diagram", "API-D1")]);
    assert_eq!(diagram["cited_by"], serde_json::json!(["API-2"]));
    let next = dst.run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Otro"),
            ("source", "graph LR\n  A --> B"),
        ],
    );
    assert_eq!(next["key"], "API-D2");
}

#[test]
fn import_as_new_projects_renames_colliding_keys_and_merges_by_name() {
    let mut src = rich();
    let path = temp_file("api.taskcraft");
    src.run("workspace.export", &[("path", &path), ("projects", "API")]);

    let mut dst = T::with_tasks(1);
    dst.run("label.create", &[("name", "db")]);
    let preview = dst.run(
        "workspace.import_preview",
        &[("path", &path), ("mode", "project")],
    );
    assert_eq!(preview["collisions"][0]["resolution"], "renamed:API2");
    dst.run("workspace.import", &[("path", &path), ("mode", "project")]);

    let api2 = dst.keys(&[("project", "API2"), ("archived", "true")]);
    assert_eq!(api2, ["API2-1", "API2-2", "API2-3", "API2-5"]);
    let detail = dst.run("task.get", &[("task", "API2-2")]);
    assert_eq!(detail["blocked_by"][0], "API2-1");
    assert_eq!(detail["comments"], 1);
    assert_eq!(detail["epic"]["name"], "Pagos");
    assert_eq!(detail["attachments"][0]["name"], "log.txt");
    assert_eq!(detail["related"][0], "API2-1");
    assert_eq!(
        dst.run("task.get", &[("task", "API2-1")])["mentioned_by"][0],
        "API2-2"
    );
    // La cita del diagrama sigue a la clave renombrada.
    assert_eq!(detail["description"], "Ver [[API2-1]]\n\n![[API2-D1]]");
    assert_eq!(detail["diagrams"][0]["key"], "API2-D1");
    assert_eq!(
        dst.run("diagram.get", &[("diagram", "API2-D1")])["cited_by"],
        serde_json::json!(["API2-2"])
    );
    let labels = dst.run("label.list", &[]);
    assert_eq!(labels["total"], 2, "db fusionada, rust nueva");
    let original = dst.keys(&[("project", "API")]);
    assert_eq!(original, ["API-1"], "lo que había sigue igual");
    // Todos los IDs son nuevos: importar dos veces no choca.
    dst.run("workspace.import", &[("path", &path), ("mode", "project")]);
    assert_eq!(dst.keys(&[("project", "API3")]).len(), 3);
}

#[test]
fn a_rejected_import_leaves_nothing_behind() {
    let mut src = rich();
    let path = temp_file("x.taskcraft");
    src.run("workspace.export", &[("path", &path)]);

    let mut dst = T::with_tasks(1);
    let preview = dst.run("workspace.import_preview", &[("path", &path)]);
    assert_eq!(preview["can_import"], false);
    let e = dst.fail("workspace.import", &[("path", &path)]);
    assert_eq!(e.key(), "exchange-target-not-empty");

    // Un archivo con referencias rotas tampoco entra, ni en parte.
    let (manifest, mut broken) = {
        let file = std::fs::File::open(&path).unwrap();
        exchange::read(std::io::BufReader::new(file)).unwrap()
    };
    broken.tasks[1].parent_id = Some(model::TaskId::generate());
    let bad = temp_file("bad.taskcraft");
    exchange::write(std::fs::File::create(&bad).unwrap(), &manifest, &broken).unwrap();
    let mut empty = T::new();
    let e = empty.fail("workspace.import", &[("path", &bad)]);
    assert_eq!(e.key(), "exchange-invalid");
    let left = empty
        .engine
        .read(|r| Ok(r.projects(true)?.len() + r.labels()?.len()))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn only_the_supervisor_exports_and_imports() {
    let mut t = T::with_tasks(1);
    let path = temp_file("y.taskcraft");
    t.as_agent();
    assert_eq!(
        t.fail("workspace.export", &[("path", &path)]).code(),
        "FORBIDDEN"
    );
    assert_eq!(
        t.fail("workspace.import", &[("path", &path)]).code(),
        "FORBIDDEN"
    );
}

/// RF-EXP-03b: fusionar una copia que cambió por los dos lados.
#[test]
fn merge_updates_newer_tasks_and_reports_conflicts() {
    let mut src = rich();
    let first = temp_file("base.taskcraft");
    src.run("workspace.export", &[("path", &first)]);
    let mut dst = T::new();
    dst.run("workspace.import", &[("path", &first)]);

    // Cambia el destino primero y el origen después.
    dst.run("task.update", &[("task", "API-1"), ("title", "en destino")]);
    dst.run(
        "task.create",
        &[("project", "API"), ("title", "solo en destino")],
    );
    src.run("task.update", &[("task", "API-2"), ("title", "en origen")]);
    src.run(
        "task.create",
        &[("project", "API"), ("title", "solo en origen")],
    );
    src.run("comment.add", &[("task", "API-2"), ("body", "otra nota")]);
    let second = temp_file("cambios.taskcraft");
    src.run("workspace.export", &[("path", &second)]);

    let preview = dst.run(
        "workspace.import_preview",
        &[("path", &second), ("mode", "merge")],
    );
    assert_eq!(preview["can_import"], true, "{preview}");
    assert_eq!(preview["updates"]["tasks"], 1, "{preview}");
    assert_eq!(preview["creates"]["tasks"], 1);
    assert_eq!(preview["creates"]["comments"], 1);
    let collisions = preview["collisions"].to_string();
    assert!(collisions.contains("renumbered:API-7"), "{collisions}");
    assert!(collisions.contains("kept-local"), "{collisions}");

    dst.run("workspace.import", &[("path", &second), ("mode", "merge")]);
    let title = |t: &mut T, key: &str| t.run("task.get", &[("task", key)])["title"].clone();
    assert_eq!(
        title(&mut dst, "API-1"),
        "en destino",
        "el destino era más nuevo"
    );
    assert_eq!(title(&mut dst, "API-2"), "en origen");
    assert_eq!(title(&mut dst, "API-6"), "solo en destino");
    assert_eq!(title(&mut dst, "API-7"), "solo en origen");
    assert_eq!(dst.run("task.get", &[("task", "API-2")])["comments"], 2);

    // Volver a fusionar lo mismo no cambia nada.
    let again = dst.run(
        "workspace.import_preview",
        &[("path", &second), ("mode", "merge")],
    );
    assert_eq!(again["updates"]["tasks"], 0, "{again}");
    assert_eq!(again["creates"]["comments"], 0);
}

#[test]
fn markdown_export_writes_one_file_per_task_with_frontmatter() {
    let mut t = rich();
    let dir = temp_file("md");
    let out = t.run(
        "workspace.export_markdown",
        &[("dir", &dir), ("projects", "API")],
    );
    assert_eq!(out["files"], 4, "sin la de la papelera");
    let file = std::path::Path::new(&dir).join("API/API-1-tarea-1.md");
    let text = std::fs::read_to_string(file).unwrap();
    assert!(
        text.starts_with("---\nkey: \"API-1\"\ntitle: \"Tarea 1\""),
        "{text}"
    );
    assert!(text.contains("estimate: 2.5"));
    assert!(text.contains("fields:\n  cliente: \"Acme\""));
    assert!(text.contains("- [ ] paso"));
}

#[test]
fn csv_export_uses_the_view_columns_and_filter() {
    let mut t = rich();
    t.run(
        "view.save",
        &[
            ("name", "Etiquetadas"),
            ("filter", "label:db"),
            ("columns", "key,title,labels,field.cliente"),
        ],
    );
    let path = temp_file("hija.csv");
    let out = t.run(
        "task.export_csv",
        &[("path", &path), ("view", "etiquetadas")],
    );
    assert_eq!(out["rows"], 1);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        "key,title,labels,field.cliente\r\nAPI-5,Hija,db;rust,\r\n"
    );
}

/// RF-ALM-06: copias `.taskcraft` con rotación; se restauran importándolas.
#[test]
fn backups_rotate_and_restore() {
    let mut t = rich();
    let dir = temp_file("copias");
    t.run(
        "workspace.configure",
        &[("backup_keep", "2"), ("backup_hours", "24")],
    );
    let base = model::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(72))
        .unwrap();
    for h in [0, 25, 50] {
        t.clock = Some(
            base.checked_add(jiff::SignedDuration::from_hours(h))
                .unwrap(),
        );
        t.run("workspace.backup", &[("dir", &dir)]);
    }
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(files.len(), 2, "solo las dos más recientes");

    // La última es de hace 22 h: la automática todavía no toca; a las 25 h, sí.
    t.clock = Some(
        base.checked_add(jiff::SignedDuration::from_hours(72))
            .unwrap(),
    );
    let skipped = t.run("workspace.backup", &[("dir", &dir), ("auto", "true")]);
    assert_eq!(skipped["skipped"], true);
    t.clock = Some(
        base.checked_add(jiff::SignedDuration::from_hours(75))
            .unwrap(),
    );
    let made = t.run("workspace.backup", &[("dir", &dir), ("auto", "true")]);
    assert_eq!(made["removed"], 1);

    let mut restored = T::new();
    restored.run(
        "workspace.import",
        &[("path", made["path"].as_str().unwrap())],
    );
    assert_eq!(
        restored.run("task.get", &[("task", "API-1")])["title"],
        t.run("task.get", &[("task", "API-1")])["title"]
    );
    // Sin carpeta del workspace ni `dir`, falta el argumento.
    assert_eq!(t.fail("workspace.backup", &[]).code(), "INVALID");
}
