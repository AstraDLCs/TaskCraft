//! M5: flujo con etapas (D10), estimación, campos, épicas e hitos,
//! plantillas, adjuntos, relaciones y menciones.

use model::Actor;
use serde_json::Value;

use super::T;

impl T {
    fn get(&mut self, key: &str) -> Value {
        self.run("task.get", &[("task", key)])
    }
}

#[test]
fn stages_live_inside_base_statuses() {
    let mut t = T::with_tasks(2);
    t.run(
        "status.create",
        &[
            ("project", "API"),
            ("name", "Pruebas QA"),
            ("base", "in_review"),
        ],
    );
    let flow = t.run("status.list", &[("project", "API")]);
    let keys: Vec<&str> = flow["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "backlog",
            "todo",
            "in_progress",
            "in_review",
            "pruebas_qa",
            "done",
            "cancelled"
        ]
    );

    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    let moved = t.run("task.move", &[("task", "API-1"), ("status", "pruebas qa")]);
    assert_eq!(moved["status"], "in_review");
    assert_eq!(moved["stage"]["key"], "pruebas_qa");
    // Saltarse estados base sigue sin estar permitido.
    let e = t.fail("task.move", &[("task", "API-2"), ("status", "pruebas_qa")]);
    assert_eq!(e.code(), "INVALID");
    // Volver a un estado base limpia la etapa.
    let back = t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    assert!(back["stage"].is_null());

    t.run("task.move", &[("task", "API-1"), ("status", "pruebas_qa")]);
    t.run(
        "status.delete",
        &[("project", "API"), ("status", "pruebas_qa")],
    );
    assert!(t.get("API-1")["stage"].is_null());
    assert_eq!(t.get("API-1")["status"], "in_review");
    let e = t.fail("status.delete", &[("project", "API"), ("status", "todo")]);
    assert_eq!(e.key(), "status-base-permanent");

    // Una etapa de `todo` sirve para crear tareas, y renombrar mueve columnas.
    t.run(
        "status.create",
        &[("project", "API"), ("name", "Lista"), ("base", "todo")],
    );
    let created = t.run(
        "task.create",
        &[("project", "API"), ("title", "x"), ("status", "lista")],
    );
    assert_eq!(created["stage"]["key"], "lista");
    t.run(
        "status.update",
        &[
            ("project", "API"),
            ("status", "todo"),
            ("name", "Por empezar"),
            ("position", "1"),
        ],
    );
    let flow = t.run("status.list", &[("project", "API")]);
    assert_eq!(flow["items"][0]["key"], "todo");
    assert_eq!(flow["items"][0]["name"], "Por empezar");
}

#[test]
fn engine_moves_clear_stages_that_no_longer_fit() {
    let mut t = T::with_tasks(1);
    t.run(
        "status.create",
        &[("project", "API"), ("name", "Lista"), ("base", "todo")],
    );
    t.run("task.move", &[("task", "API-1"), ("status", "lista")]);
    let agent = t.agent("bot");
    t.as_(&agent);
    let claimed = t.run("task.claim", &[("task", "API-1")]);
    assert_eq!(claimed["status"], "in_progress");
    t.actor = Actor::Supervisor;
    assert!(t.get("API-1")["stage"].is_null());
}

#[test]
fn estimates_follow_the_project_unit() {
    let mut t = T::new();
    t.run(
        "project.create",
        &[("key", "WEB"), ("estimate_unit", "size")],
    );
    let task = t.run(
        "task.create",
        &[("project", "WEB"), ("title", "x"), ("estimate", "m")],
    );
    assert_eq!(task["estimate"]["value"], "M");
    let e = t.fail("task.update", &[("task", "WEB-1"), ("estimate", "3.5")]);
    assert_eq!(e.key(), "rule-estimate");
    t.run(
        "project.update",
        &[("project", "WEB"), ("estimate_unit", "hours")],
    );
    let task = t.run("task.update", &[("task", "WEB-1"), ("estimate", "3,5")]);
    assert_eq!(task["estimate"]["value"], "3.5");
    let task = t.run("task.update", &[("task", "WEB-1"), ("estimate", "none")]);
    assert!(task["estimate"].is_null());
}

#[test]
fn custom_fields_are_typed_per_project() {
    let mut t = T::with_tasks(1);
    t.run(
        "field.create",
        &[
            ("project", "API"),
            ("name", "Cliente"),
            ("kind", "select"),
            ("options", "Acme,Globex"),
        ],
    );
    let task = t.run(
        "task.set_field",
        &[("task", "API-1"), ("field", "cliente"), ("value", "acme")],
    );
    assert_eq!(task["fields"][0]["value"], "Acme");
    let e = t.fail(
        "task.set_field",
        &[
            ("task", "API-1"),
            ("field", "cliente"),
            ("value", "Initech"),
        ],
    );
    assert_eq!(e.key(), "rule-field-value");
    let created = t.run(
        "plan.create",
        &[
            ("project", "API"),
            ("items", r#"[{"title":"y","fields":{"Cliente":"Globex"}}]"#),
        ],
    );
    assert_eq!(created["count"], 1);
    assert_eq!(t.get("API-2")["fields"][0]["value"], "Globex");
    t.run("field.delete", &[("project", "API"), ("field", "cliente")]);
    assert_eq!(t.get("API-1")["fields"], serde_json::json!([]));
}

#[test]
fn epics_group_tasks_track_progress_and_can_be_locked() {
    let mut t = T::with_tasks(3);
    t.run(
        "epic.create",
        &[
            ("project", "API"),
            ("name", "Pagos"),
            ("target", "2026-12-01"),
        ],
    );
    t.run(
        "epic.create",
        &[("project", "API"), ("name", "v1"), ("kind", "milestone")],
    );
    for key in ["API-1", "API-2"] {
        t.run("task.set_epic", &[("task", key), ("epic", "pagos")]);
    }
    t.run(
        "task.set_milestone",
        &[("task", "API-3"), ("milestone", "v1")],
    );
    t.run("task.update", &[("task", "API-1"), ("estimate", "3")]);
    t.run("task.update", &[("task", "API-2"), ("estimate", "1")]);
    for s in ["in_progress", "done"] {
        t.run("task.move", &[("task", "API-1"), ("status", s)]);
    }
    let epic = t.run("epic.get", &[("epic", "Pagos")]);
    assert_eq!(epic["progress"]["tasks"], 2);
    assert_eq!(epic["progress"]["percent"], 50);
    assert_eq!(epic["progress"]["effort_percent"], 75);
    assert_eq!(epic["task_keys"], serde_json::json!(["API-1", "API-2"]));
    let e = t.fail("task.set_epic", &[("task", "API-3"), ("epic", "v1")]);
    assert_eq!(e.code(), "NOT_FOUND", "v1 es un hito, no una épica");

    t.run("lock.set", &[("epic", "pagos")]);
    let bot = t.agent("bot");
    t.as_(&bot);
    let e = t.fail("task.update", &[("task", "API-2"), ("title", "z")]);
    assert_eq!(e.code(), "LOCKED_BY_HUMAN");
    let next = t.run("task.next", &[]);
    assert_eq!(
        next["task"]["key"], "API-3",
        "la épica bloqueada no se ofrece"
    );
    t.actor = Actor::Supervisor;
    t.run("epic.delete", &[("epic", "pagos")]);
    assert!(t.get("API-2")["epic"].is_null());
}

#[test]
fn templates_capture_and_recreate_task_trees_and_projects() {
    let mut t = T::with_tasks(1);
    t.run("task.create", &[("parent", "API-1"), ("title", "Hija")]);
    t.run(
        "task.create",
        &[("parent", "API-1"), ("title", "Otra hija")],
    );
    t.run("task.link", &[("blocker", "API-2"), ("blocked", "API-3")]);
    t.run("checklist.add", &[("task", "API-2"), ("text", "paso")]);
    t.run(
        "template.from_task",
        &[("task", "API-1"), ("name", "Release")],
    );

    t.run("project.create", &[("key", "WEB")]);
    let out = t.run(
        "template.instantiate",
        &[("template", "release"), ("project", "WEB")],
    );
    assert_eq!(out["count"], 3);
    let child = t.get("WEB-2");
    assert_eq!(child["parent"], "WEB-1");
    assert_eq!(child["checklist"][0]["text"], "paso");
    assert_eq!(t.get("WEB-3")["blocked_by"][0], "WEB-2");

    // Bajo una tarea existente: la raíz de la plantilla cuelga de ella.
    t.run(
        "template.instantiate",
        &[("template", "release"), ("parent", "WEB-1")],
    );
    assert_eq!(t.get("WEB-4")["parent"], "WEB-1");

    let definition = r#"{"project":{"description":"Servicio","estimate_unit":"points",
        "workflow":[{"key":"qa","name":"QA","base":"in_review"}]},
        "items":[{"title":"Montar CI","status":"backlog"}]}"#;
    t.run(
        "template.create",
        &[
            ("name", "Servicio"),
            ("kind", "project"),
            ("definition", definition),
        ],
    );
    t.run(
        "template.instantiate",
        &[("template", "servicio"), ("key", "SRV")],
    );
    let project = t.run("project.get", &[("project", "SRV")]);
    assert_eq!(project["estimate_unit"], "points");
    assert!(project["workflow"].to_string().contains("\"qa\""));
    assert_eq!(t.get("SRV-1")["status"], "backlog");
    let bot = t.agent("bot");
    t.as_(&bot);
    let e = t.fail(
        "template.create",
        &[
            ("name", "x"),
            ("kind", "task"),
            ("definition", r#"{"items":[]}"#),
        ],
    );
    assert_eq!(e.code(), "FORBIDDEN");
}

#[test]
fn attachments_are_stored_once_and_limited_in_size() {
    use base64::Engine as _;
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut t = T::with_tasks(2);
    let a = t.run(
        "attachment.add",
        &[
            ("task", "API-1"),
            ("name", "log.txt"),
            ("data", &b64(b"hola")),
        ],
    );
    assert_eq!(a["mime"], "text/plain");
    assert_eq!(a["size"], 4);
    t.run(
        "attachment.add",
        &[
            ("task", "API-2"),
            ("name", "copia.txt"),
            ("data", &b64(b"hola")),
        ],
    );
    let got = t.run(
        "attachment.get",
        &[("attachment", a["id"].as_str().unwrap())],
    );
    assert_eq!(got["data"], b64(b"hola"));
    assert_eq!(t.get("API-1")["attachments"][0]["name"], "log.txt");

    t.run("workspace.configure", &[("max_attachment_mb", "1")]);
    let big = vec![0u8; 1024 * 1024 + 1];
    let e = t.fail(
        "attachment.add",
        &[("task", "API-1"), ("name", "big.bin"), ("data", &b64(&big))],
    );
    assert_eq!(e.key(), "attachment-too-big");

    t.run(
        "attachment.remove",
        &[("attachment", a["id"].as_str().unwrap())],
    );
    assert_eq!(t.get("API-1")["attachments"], serde_json::json!([]));
    // El contenido sigue para la otra tarea que lo adjunta.
    let other = t.run("attachment.list", &[("task", "API-2")]);
    let id = other["items"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        t.run("attachment.get", &[("attachment", &id)])["data"],
        b64(b"hola")
    );
}

#[test]
fn relations_and_duplicates() {
    let mut t = T::with_tasks(3);
    t.run("task.relate", &[("task", "API-2"), ("other", "API-1")]);
    assert_eq!(t.get("API-1")["related"][0], "API-2");
    assert_eq!(t.get("API-2")["related"][0], "API-1");
    let dup = t.run(
        "task.relate",
        &[
            ("task", "API-3"),
            ("other", "API-1"),
            ("kind", "duplicates"),
        ],
    );
    assert_eq!(dup["status"], "cancelled");
    assert_eq!(dup["duplicate_of"], "API-1");
    assert_eq!(t.get("API-1")["duplicates"][0], "API-3");
    t.run("task.unrelate", &[("task", "API-1"), ("other", "API-2")]);
    assert_eq!(t.get("API-2")["related"], serde_json::json!([]));
    assert_eq!(
        t.fail("task.relate", &[("task", "API-1"), ("other", "API-1")])
            .key(),
        "relation-self"
    );
}

#[test]
fn mentions_create_incoming_references() {
    let mut t = T::with_tasks(3);
    t.run(
        "task.update",
        &[
            ("task", "API-1"),
            ("description", "Depende de [[API-2]] y [[NOPE-1]]"),
        ],
    );
    t.run(
        "comment.add",
        &[("task", "API-3"), ("body", "Ver también [[api-2]]")],
    );
    assert_eq!(
        t.get("API-2")["mentioned_by"],
        serde_json::json!(["API-1", "API-3"])
    );
    t.run(
        "task.update",
        &[("task", "API-1"), ("description", "Ya no")],
    );
    assert_eq!(t.get("API-2")["mentioned_by"], serde_json::json!(["API-3"]));
}
