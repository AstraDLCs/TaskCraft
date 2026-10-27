//! M15 (Extras): diagramas Mermaid citables en tareas (RF-MOD-19, D21).

use model::Actor;
use serde_json::{Value, json};

use super::T;

const FLOW: &str = "flowchart TD\n  A[Inicio] --> B[Fin]";

fn keys(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn diagrams_are_numbered_per_project_and_numbers_are_not_reused() {
    let mut t = T::with_tasks(0);
    let d1 = t.run(
        "diagram.create",
        &[("project", "API"), ("title", "Flujo"), ("source", FLOW)],
    );
    assert_eq!(d1["key"], "API-D1");
    assert!(d1["render_error"].is_null(), "{d1}");
    assert_eq!(d1["cited_by"], json!([]));
    let d2 = t.run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Roto"),
            ("source", "esto no es mermaid"),
        ],
    );
    assert_eq!(d2["key"], "API-D2");
    // Se guarda igual; el error se devuelve para que el agente lo corrija.
    assert!(d2["render_error"].as_str().is_some_and(|e| !e.is_empty()));

    t.run("diagram.delete", &[("diagram", "API-D2")]);
    let d3 = t.run(
        "diagram.create",
        &[("project", "api"), ("title", "Otro"), ("source", FLOW)],
    );
    assert_eq!(d3["key"], "API-D3", "el número borrado no se reutiliza");
    // Crear tareas no mueve el contador de diagramas, ni al revés.
    let task = t.run("task.create", &[("project", "API"), ("title", "x")]);
    assert_eq!(task["key"], "API-1");

    let list = t.run("diagram.list", &[("project", "API")]);
    assert_eq!(list["count"], 2);
    assert_eq!(list["items"][0]["key"], "API-D1");
    assert!(list["items"][0].get("source").is_none());

    // Por clave, por ID o por número dentro del proyecto.
    let id = d1["id"].as_str().unwrap().to_owned();
    assert_eq!(t.run("diagram.get", &[("diagram", &id)])["title"], "Flujo");
    assert_eq!(
        t.run("diagram.get", &[("diagram", "D1"), ("project", "API")])["title"],
        "Flujo"
    );
    assert_eq!(
        t.fail("diagram.get", &[("diagram", "API-D2")]).code(),
        "NOT_FOUND"
    );
    assert_eq!(
        t.fail("diagram.get", &[("diagram", "D1")]).code(),
        "INVALID"
    );
    assert_eq!(
        t.fail(
            "diagram.create",
            &[("project", "API"), ("title", " "), ("source", FLOW)]
        )
        .code(),
        "INVALID"
    );
}

#[test]
fn get_can_render_svg_and_update_changes_fields() {
    let mut t = T::with_tasks(0);
    t.run(
        "diagram.create",
        &[("project", "API"), ("title", "Flujo"), ("source", FLOW)],
    );
    let got = t.run("diagram.get", &[("diagram", "API-D1")]);
    assert!(got.get("svg").is_none());
    let got = t.run(
        "diagram.get",
        &[("diagram", "API-D1"), ("svg", "true"), ("theme", "dark")],
    );
    assert!(got["svg"].as_str().unwrap().starts_with("<svg"));

    let up = t.run(
        "diagram.update",
        &[
            ("diagram", "api-d1"),
            ("title", "Flujo nuevo"),
            ("description", "Ver **arriba**"),
            ("source", "sequenceDiagram\n  A->>B: hola"),
        ],
    );
    assert_eq!(up["title"], "Flujo nuevo");
    assert_eq!(up["description"], "Ver **arriba**");
    assert!(up["render_error"].is_null(), "{up}");
}

#[test]
fn citations_in_descriptions_and_comments_are_tracked() {
    let mut t = T::with_tasks(2);
    t.run(
        "diagram.create",
        &[("project", "API"), ("title", "Flujo"), ("source", FLOW)],
    );
    t.run(
        "diagram.create",
        &[("project", "API"), ("title", "Datos"), ("source", FLOW)],
    );
    let task = t.run(
        "task.update",
        &[
            ("task", "API-1"),
            (
                "description",
                "Ver [[api-d1]] y [[API-9]].\n\n![[API-D1]]\n\n[[API-D9]]",
            ),
        ],
    );
    assert_eq!(task["diagrams"][0]["key"], "API-D1");
    assert_eq!(
        task["diagrams"].as_array().unwrap().len(),
        1,
        "D9 no existe"
    );
    t.run(
        "comment.add",
        &[("task", "API-2"), ("body", "y ![[API-D2]]")],
    );
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "Nueva"),
            ("description", "[[API-D1]]"),
        ],
    );

    let d1 = t.run("diagram.get", &[("diagram", "API-D1")]);
    assert_eq!(keys(&d1["cited_by"]), ["API-1", "API-3"]);
    let d2 = t.run("diagram.get", &[("diagram", "API-D2")]);
    assert_eq!(keys(&d2["cited_by"]), ["API-2"]);
    assert_eq!(
        t.run("task.get", &[("task", "API-2")])["diagrams"][0]["title"],
        "Datos"
    );

    // Quitar la cita del texto la quita de la lista.
    t.run("task.update", &[("task", "API-1"), ("description", "nada")]);
    let d1 = t.run("diagram.get", &[("diagram", "API-D1")]);
    assert_eq!(keys(&d1["cited_by"]), ["API-3"]);
    assert_eq!(
        t.run("task.get", &[("task", "API-1")])["diagrams"],
        json!([])
    );

    // Borrar el diagrama borra sus citas; el texto se queda como estaba.
    t.run("diagram.delete", &[("diagram", "API-D2")]);
    let task = t.run("task.get", &[("task", "API-2")]);
    assert_eq!(task["diagrams"], json!([]));
}

#[test]
fn diagram_changes_are_in_history_and_can_be_reverted() {
    let mut t = T::with_tasks(0);
    t.run(
        "diagram.create",
        &[("project", "API"), ("title", "Flujo"), ("source", FLOW)],
    );
    t.run(
        "diagram.update",
        &[("diagram", "API-D1"), ("title", "Malo")],
    );
    let history = t.run("activity.list", &[("entity", "diagram")]);
    let items = history["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{history}");
    assert_eq!(items[0]["target"], "API-D1", "{history}");
    // La creación no deja un cambio de proyecto por el contador.
    let projects = t.run("activity.list", &[("entity", "project")]);
    assert_eq!(projects["items"].as_array().unwrap().len(), 1, "{projects}");

    let rename = items[0]["id"].as_str().unwrap().to_owned();
    t.run("activity.revert", &[("entry", &rename)]);
    assert_eq!(
        t.run("diagram.get", &[("diagram", "API-D1")])["title"],
        "Flujo"
    );

    let created = items[1]["id"].as_str().unwrap().to_owned();
    t.run("activity.revert", &[("entry", &created)]);
    assert_eq!(
        t.fail("diagram.get", &[("diagram", "API-D1")]).code(),
        "NOT_FOUND"
    );
}

#[test]
fn executing_agents_write_diagrams_but_only_planners_delete_them() {
    let mut t = T::with_tasks(0);
    let bot = t.agent("bot");
    t.run("agent.update", &[("agent", "bot"), ("level", "execute")]);
    t.as_(&bot);
    t.run(
        "diagram.create",
        &[("project", "API"), ("title", "Flujo"), ("source", FLOW)],
    );
    t.run("diagram.update", &[("diagram", "API-D1"), ("source", FLOW)]);
    let e = t.fail("diagram.delete", &[("diagram", "API-D1")]);
    assert_eq!(e.key(), "agent-level");
    t.actor = Actor::Supervisor;
    t.run("diagram.delete", &[("diagram", "API-D1")]);
}
