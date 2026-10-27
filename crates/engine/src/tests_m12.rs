//! M12 (Extras): coste reportado por los agentes (RF-HIS-06).

use model::Actor;

use super::T;

#[test]
fn agents_report_cost_and_it_adds_up_by_task_epic_and_project() {
    let mut t = T::with_tasks(3);
    t.run("task.create", &[("parent", "API-1"), ("title", "subtarea")]);
    t.run("epic.create", &[("project", "API"), ("name", "Pagos")]);
    t.run("task.set_epic", &[("task", "API-1"), ("epic", "pagos")]);

    t.as_agent();
    let first = t.run(
        "cost.report",
        &[
            ("task", "API-1"),
            ("tokens_in", "10000"),
            ("tokens_out", "2000"),
            ("amount", "0.1"),
            ("model", "claude-opus-5-5"),
        ],
    );
    assert_eq!(first["amount"], "0.10");
    assert_eq!(first["currency"], "USD");
    t.run(
        "cost.report",
        &[("task", "API-4"), ("tokens_in", "500"), ("amount", "0.2")],
    );
    t.run(
        "cost.report",
        &[("task", "API-2"), ("amount", "1,5"), ("currency", "eur")],
    );
    let agent = t.actor.clone();
    t.as_agent();
    t.run(
        "cost.report",
        &[("task", "API-3"), ("tokens_out", "42"), ("model", "otro")],
    );

    // Una tarea incluye sus subtareas: 0.1 + 0.2 sin error de coma flotante.
    let task = t.run("cost.summary", &[("task", "API-1")]);
    assert_eq!(task["total"]["entries"], 2);
    assert_eq!(task["total"]["tokens_in"], 10500);
    assert_eq!(task["total"]["amounts"][0]["amount"], "0.30");

    // La épica tiene API-1 (no su subtarea, que no está en la épica).
    let epic = t.run("cost.summary", &[("epic", "Pagos")]);
    assert_eq!(epic["total"]["entries"], 1);

    // El proyecto, todo, con monedas separadas y agrupado por agente.
    let project = t.run("cost.summary", &[("project", "API")]);
    assert_eq!(project["total"]["entries"], 4);
    let amounts = project["total"]["amounts"].as_array().unwrap();
    assert_eq!(amounts.len(), 2, "EUR y USD por separado: {amounts:?}");
    assert_eq!(project["groups"].as_array().unwrap().len(), 2);
    let by_model = t.run("cost.summary", &[("project", "API"), ("group", "model")]);
    let keys: Vec<&str> = by_model["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["", "claude-opus-5-5", "otro"]);

    // Filtro por actor y lista.
    let mine = t.run("cost.list", &[("actor", &agent.to_string())]);
    assert_eq!(mine["items"].as_array().unwrap().len(), 3);
    assert_eq!(mine["items"][0]["model"], "claude-opus-5-5");

    // Y aparece en el historial de la tarea.
    t.actor = Actor::Supervisor;
    let history = t.run("activity.list", &[("task", "API-3")]);
    assert!(
        history["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["entity"] == "cost"),
        "{history}"
    );
}

#[test]
fn a_cost_report_needs_something_valid_and_respects_scope() {
    let mut t = T::with_tasks(1);
    t.run("project.create", &[("key", "web")]);
    t.run("task.create", &[("project", "WEB"), ("title", "fuera")]);
    t.as_agent();
    assert_eq!(
        t.fail("cost.report", &[("task", "API-1")]).key(),
        "cost-empty"
    );
    assert_eq!(
        t.fail("cost.report", &[("task", "API-1"), ("amount", "-1")])
            .key(),
        "rule-amount"
    );
    assert_eq!(
        t.fail(
            "cost.report",
            &[("task", "API-1"), ("amount", "1"), ("currency", "dolares")]
        )
        .key(),
        "rule-currency"
    );

    // Con alcance limitado a API, no reporta en WEB.
    let me = t.actor.clone();
    t.actor = Actor::Supervisor;
    let name = t.engine.read(|r| crate::views::actor_name(r, &me)).unwrap();
    t.run("agent.update", &[("agent", &name), ("scope", "API")]);
    t.actor = me;
    t.run("cost.report", &[("task", "API-1"), ("tokens_in", "1")]);
    assert_eq!(
        t.fail("cost.report", &[("task", "WEB-1"), ("tokens_in", "1")])
            .code(),
        "FORBIDDEN"
    );
}

const SHA1: &str = "1111111111111111111111111111111111111111";
const SHA2: &str = "2222222222222222222222222222222222222222";

fn status(t: &mut T, key: &str) -> String {
    t.run("task.get", &[("task", key)])["status"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_commit_links_the_tasks_it_mentions_and_moves_the_ones_it_closes() {
    let mut t = T::with_tasks(4);
    t.run("task.move", &[("task", "API-2"), ("status", "in_progress")]);
    t.run("task.move", &[("task", "API-4"), ("status", "in_progress")]);
    t.run("lock.set", &[("task", "API-4")]);

    t.as_agent();
    let out = t.run(
        "git.ingest",
        &[
            ("sha", SHA1),
            (
                "message",
                "Fixes API-1 and API-2, closes API-4\n\nToca también API-3 y NOPE-1",
            ),
            ("branch", "feat/API-3-login"),
            ("remote", "git@github.com:org/repo.git"),
        ],
    );
    assert_eq!(
        out["linked"],
        serde_json::json!(["API-1", "API-2", "API-4", "API-3"])
    );
    assert_eq!(out["moved"], serde_json::json!(["API-1", "API-2"]));
    assert_eq!(
        out["not_moved"][0]["task"], "API-4",
        "bloqueada por el humano"
    );
    assert_eq!(out["unknown"], serde_json::json!(["NOPE-1"]));
    assert_eq!(out["branch_linked"], serde_json::json!(["API-3"]));

    // Desde por hacer pasa por en curso; la tarea mencionada sin cierre no se mueve.
    assert_eq!(status(&mut t, "API-1"), "in_review");
    assert_eq!(status(&mut t, "API-2"), "in_review");
    assert_eq!(status(&mut t, "API-3"), "todo");
    assert_eq!(status(&mut t, "API-4"), "in_progress");

    let detail = t.run("task.get", &[("task", "API-1")]);
    let link = &detail["git"][0];
    assert_eq!(link["kind"], "commit");
    assert_eq!(link["short"], "1111111");
    assert_eq!(link["title"], "Fixes API-1 and API-2, closes API-4");
    assert_eq!(
        link["url"],
        format!("https://github.com/org/repo/commit/{SHA1}")
    );
    let api3 = t.run("git.links", &[("task", "API-3")]);
    let kinds: Vec<&str> = api3["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["commit", "branch"]);

    // El mismo commit otra vez no duplica ni mueve: API-1 volvió a por hacer.
    t.actor = Actor::Supervisor;
    t.run("task.move", &[("task", "API-1"), ("status", "todo")]);
    let again = t.run("git.ingest", &[("sha", SHA1), ("message", "Fixes API-1")]);
    assert_eq!(again["moved"], serde_json::json!([]));
    assert_eq!(status(&mut t, "API-1"), "todo");
    assert_eq!(
        t.run("git.links", &[("task", "API-1")])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Uno nuevo sí.
    t.run(
        "git.ingest",
        &[("sha", SHA2), ("message", "resuelve API-1")],
    );
    assert_eq!(status(&mut t, "API-1"), "in_review");
}

#[test]
fn links_can_be_added_and_removed_by_hand() {
    let mut t = T::with_tasks(1);
    let pr = t.run(
        "git.link",
        &[
            ("task", "API-1"),
            ("kind", "pr"),
            ("ref", "https://github.com/org/repo/pull/7"),
            ("title", "Login"),
        ],
    );
    assert_eq!(pr["created"], true);
    assert_eq!(pr["url"], "https://github.com/org/repo/pull/7");
    let dup = t.run(
        "git.link",
        &[
            ("task", "API-1"),
            ("kind", "pr"),
            ("ref", "https://github.com/org/repo/pull/7"),
        ],
    );
    assert_eq!(dup["created"], false);
    t.run(
        "git.link",
        &[("task", "API-1"), ("kind", "commit"), ("ref", SHA1)],
    );
    assert_eq!(
        t.fail(
            "git.link",
            &[("task", "API-1"), ("kind", "commit"), ("ref", "abc")]
        )
        .key(),
        "git-bad-sha"
    );
    // Un commit se quita con su SHA corto.
    let removed = t.run(
        "git.unlink",
        &[("task", "API-1"), ("kind", "commit"), ("ref", "1111111")],
    );
    assert_eq!(removed["removed"], true);
    let left = t.run("git.links", &[("project", "API")]);
    assert_eq!(left["items"].as_array().unwrap().len(), 1);
    let history = t.run("activity.list", &[("task", "API-1")]);
    assert!(
        history["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["entity"] == "git" && e["action"] == "deleted"),
        "{history}"
    );
}
