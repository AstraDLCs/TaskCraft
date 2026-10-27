//! M6: permisos por agente (D11), pausa, reversión desde el historial, WIP
//! por columna, resumen e idempotencia.

use model::Actor;

use super::T;

impl T {
    fn sup(&mut self) -> &mut Self {
        self.actor = Actor::Supervisor;
        self
    }
}

#[test]
fn levels_limit_what_each_agent_can_do() {
    let mut t = T::with_tasks(1);
    let bot = t.agent("bot");
    t.run("agent.update", &[("agent", "bot"), ("level", "read")]);
    t.as_(&bot);
    t.run("task.get", &[("task", "API-1")]);
    let e = t.fail("comment.add", &[("task", "API-1"), ("body", "x")]);
    assert_eq!(e.code(), "FORBIDDEN");
    assert_eq!(e.key(), "agent-level");

    t.sup()
        .run("agent.update", &[("agent", "bot"), ("level", "execute")]);
    t.as_(&bot);
    t.run("comment.add", &[("task", "API-1"), ("body", "x")]);
    let e = t.fail("task.create", &[("project", "API"), ("title", "y")]);
    assert_eq!(e.key(), "agent-level", "crear es planificar");

    t.sup()
        .run("agent.update", &[("agent", "bot"), ("level", "admin")]);
    t.as_(&bot);
    t.run(
        "status.create",
        &[("project", "API"), ("name", "QA"), ("base", "in_review")],
    );
    let path = std::env::temp_dir().join(format!("m6-{}.taskcraft", model::TaskId::generate()));
    t.run("workspace.export", &[("path", &path.to_string_lossy())]);
    // Lo exclusivo del humano sigue siéndolo.
    assert_eq!(t.fail("lock.set", &[("task", "API-1")]).code(), "FORBIDDEN");
}

#[test]
fn scope_limits_writes_and_offers() {
    let mut t = T::with_tasks(1);
    t.run("project.create", &[("key", "WEB")]);
    t.run("task.create", &[("project", "WEB"), ("title", "web")]);
    let bot = t.agent("bot");
    t.run("agent.update", &[("agent", "bot"), ("scope", "WEB")]);
    t.as_(&bot);
    let e = t.fail("comment.add", &[("task", "API-1"), ("body", "x")]);
    assert_eq!(e.key(), "agent-out-of-scope");
    t.run("task.get", &[("task", "API-1")]);
    let next = t.run("task.next", &[]);
    assert_eq!(next["task"]["key"], "WEB-1");
    let next = t.run("task.next", &[("project", "API")]);
    assert!(next["task"].is_null());
}

#[test]
fn capabilities_filter_the_next_task() {
    let mut t = T::with_tasks(0);
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "docs"),
            ("labels", "docs"),
            ("priority", "urgent"),
        ],
    );
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "rust"),
            ("labels", "rust"),
            ("priority", "high"),
        ],
    );
    t.run("task.create", &[("project", "API"), ("title", "libre")]);
    let bot = t.agent("bot");
    t.run(
        "agent.update",
        &[("agent", "bot"), ("capabilities", "Rust")],
    );
    t.as_(&bot);
    let next = t.run("task.next", &[]);
    assert_eq!(next["task"]["key"], "API-2", "la urgente es de docs");
    assert_eq!(next["available"], 2);
}

#[test]
fn paused_agents_and_read_only_workspaces_cannot_write() {
    let mut t = T::with_tasks(1);
    let bot = t.agent("bot");
    t.run("agent.pause", &[("agent", "bot")]);
    t.as_(&bot);
    assert_eq!(t.fail("task.claim", &[("task", "API-1")]).code(), "PAUSED");
    t.run("task.list", &[]);
    t.sup().run("agent.resume", &[("agent", "bot")]);
    t.run("workspace.configure", &[("read_only", "true")]);
    t.as_(&bot);
    assert_eq!(
        t.fail("task.claim", &[("task", "API-1")]).key(),
        "workspace-read-only"
    );
    t.sup().run(
        "task.update",
        &[("task", "API-1"), ("title", "el supervisor sí")],
    );
    t.run("workspace.configure", &[("read_only", "false")]);
    t.as_(&bot);
    t.run("task.claim", &[("task", "API-1")]);
}

#[test]
fn reverting_an_entry_keeps_later_changes_of_others() {
    let mut t = T::with_tasks(2);
    t.run("task.update", &[("task", "API-1"), ("title", "Malo")]);
    let bot = t.agent("bot");
    t.as_(&bot);
    t.run("task.update", &[("task", "API-1"), ("priority", "urgent")]);
    t.sup();
    let history = t.run("activity.list", &[("task", "API-1")]);
    let rename = history["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["changes"][0]["field"] == "title")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    t.run("activity.revert", &[("entry", &rename)]);
    let task = t.run("task.get", &[("task", "API-1")]);
    assert_eq!(task["title"], "Tarea 1");
    assert_eq!(task["priority"], "urgent", "lo del agente sigue");

    // Si el campo cambió después, es un conflicto.
    t.run("task.update", &[("task", "API-2"), ("title", "Uno")]);
    let first = t.run("activity.list", &[("task", "API-2"), ("limit", "1")])["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    t.run("task.update", &[("task", "API-2"), ("title", "Dos")]);
    assert_eq!(
        t.fail("activity.revert", &[("entry", &first)]).code(),
        "CONFLICT"
    );
}

#[test]
fn reverting_a_whole_command_restores_a_deleted_tree() {
    let mut t = T::with_tasks(1);
    t.run("task.create", &[("parent", "API-1"), ("title", "hija")]);
    t.run("task.delete", &[("task", "API-1")]);
    let last = t.run("activity.list", &[("limit", "1")])["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let out = t.run(
        "activity.revert",
        &[("entry", &last), ("whole_command", "true")],
    );
    assert_eq!(out["reverted"], 2);
    assert_eq!(t.keys(&[]), ["API-1", "API-2"]);
    t.as_agent();
    let e = t.fail("activity.revert", &[("entry", &last)]);
    assert_eq!(e.code(), "FORBIDDEN");
}

#[test]
fn wip_limits_per_column() {
    let mut t = T::with_tasks(3);
    t.run(
        "status.update",
        &[
            ("project", "API"),
            ("status", "in_progress"),
            ("wip", "1"),
            ("wip_reject", "true"),
        ],
    );
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    let e = t.fail("task.move", &[("task", "API-2"), ("status", "in_progress")]);
    assert_eq!(e.key(), "wip-full");
    t.run(
        "status.update",
        &[
            ("project", "API"),
            ("status", "in_progress"),
            ("wip_reject", "false"),
        ],
    );
    t.run("task.move", &[("task", "API-2"), ("status", "in_progress")]);
    let flow = t.run("status.list", &[("project", "API")]);
    let col = flow["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "in_progress")
        .unwrap()
        .clone();
    assert_eq!(col["over_limit"], true);
}

#[test]
fn summary_since_last_visit() {
    let mut t = T::with_tasks(3);
    let since = model::Timestamp::now().to_string();
    std::thread::sleep(std::time::Duration::from_millis(5));
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    t.run("task.move", &[("task", "API-1"), ("status", "in_review")]);
    let bot = t.agent("bot");
    t.as_(&bot);
    t.run("task.claim", &[("task", "API-2")]);
    t.run("task.escalate", &[("task", "API-2"), ("question", "¿?")]);
    t.run("inbox.add", &[("title", "idea")]);
    t.sup();
    let s = t.run("activity.summary", &[("since", &since)]);
    assert_eq!(s["in_review"], serde_json::json!(["API-1"]));
    assert_eq!(s["escalated"], serde_json::json!(["API-2"]));
    assert_eq!(s["findings"], 1);
    assert_eq!(s["agents"], serde_json::json!(["bot"]));
    assert_eq!(s["created"], 0);
}

#[test]
fn creations_with_the_same_key_happen_once() {
    let mut t = T::with_tasks(0);
    let a = t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "x"),
            ("idempotency_key", "k1"),
        ],
    );
    let b = t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "x"),
            ("idempotency_key", "k1"),
        ],
    );
    assert_eq!(a["key"], b["key"]);
    assert_eq!(t.keys(&[]), ["API-1"]);
    let e = t.fail(
        "comment.add",
        &[("task", "API-1"), ("body", "x"), ("idempotency_key", "k1")],
    );
    assert_eq!(e.key(), "idempotency-key-reused");
    // Cada actor tiene sus propias claves.
    t.as_agent();
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "x"),
            ("idempotency_key", "k1"),
        ],
    );
    t.sup();
    assert_eq!(t.keys(&[]).len(), 2);
}
