//! Coordinación multiagente (P4): un test por requisito RF-COO (RNF-14) más
//! escenarios con varios agentes.

use model::{Actor, Timestamp};
use serde_json::Value;

use super::T;
use crate::testing::Swarm;
use crate::{Args, Context, EngineError};

fn minutes(base: Timestamp, m: i64) -> Timestamp {
    base.checked_add(jiff::SignedDuration::from_mins(m))
        .unwrap()
}

impl T {
    /// Registra un agente con nombre y devuelve su actor (sin cambiar el actual).
    pub(super) fn agent(&mut self, name: &str) -> Actor {
        let prev = std::mem::replace(&mut self.actor, Actor::Supervisor);
        let a = self.register(&[("name", name)]);
        self.actor = prev;
        Actor::Agent(a["id"].as_str().unwrap().parse().unwrap())
    }

    pub(super) fn as_(&mut self, actor: &Actor) -> &mut Self {
        self.actor = actor.clone();
        self
    }

    fn status(&mut self, key: &str) -> Value {
        let prev = std::mem::replace(&mut self.actor, Actor::Supervisor);
        let t = self.run("task.get", &[("task", key)]);
        self.actor = prev;
        t
    }
}

fn code_key(e: &EngineError) -> (&'static str, &'static str) {
    (e.code(), e.key())
}

#[test]
fn claim_assigns_and_excludes_others() {
    let mut t = T::with_tasks(2);
    let a = t.agent("a");
    let b = t.agent("b");

    let task = t.as_(&a).run("task.claim", &[("task", "API-1")]);
    assert_eq!(task["status"], "in_progress");
    assert_eq!(task["assignee_name"], "a");
    assert_eq!(task["claim"]["agent"], "a");

    let err = t.as_(&b).fail("task.claim", &[("task", "API-1")]);
    assert_eq!(code_key(&err), ("CLAIMED_BY_OTHER", "claimed-by-other"));
    let err = t.fail("task.update", &[("task", "API-1"), ("title", "mío")]);
    assert_eq!(err.code(), "CLAIMED_BY_OTHER");

    t.as_(&a).run(
        "task.update",
        &[("task", "API-1"), ("title", "sigue siendo mío")],
    );
    let err = t
        .as_(&Actor::Supervisor)
        .fail("task.claim", &[("task", "API-2")]);
    assert_eq!(code_key(&err), ("FORBIDDEN", "agent-only"));
}

#[test]
fn wip_limit_per_agent() {
    let mut t = T::with_tasks(2);
    let a = t.agent("a");
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    let err = t.fail("task.claim", &[("task", "API-2")]);
    assert_eq!(err.key(), "agent-wip-limit");

    t.as_(&Actor::Supervisor)
        .run("workspace.configure", &[("agent_wip_limit", "2")]);
    t.as_(&a).run("task.claim", &[("task", "API-2")]);
}

#[test]
fn only_ready_tasks_can_be_claimed() {
    let mut t = T::with_tasks(3);
    t.run("task.link", &[("blocker", "API-1"), ("blocked", "API-2")]);
    t.run("task.move", &[("task", "API-3"), ("status", "in_progress")]);
    t.run("task.move", &[("task", "API-3"), ("status", "in_review")]);
    let a = t.agent("a");

    let err = t.as_(&a).fail("task.claim", &[("task", "API-2")]);
    assert_eq!(code_key(&err), ("NOT_READY", "not-ready-blocked"));
    let err = t.fail("task.claim", &[("task", "API-3")]);
    assert_eq!(code_key(&err), ("NOT_READY", "not-claimable"));
}

#[test]
fn next_picks_the_most_urgent_available_task() {
    let mut t = T::with_tasks(0);
    for (title, priority) in [
        ("baja", "low"),
        ("urgente bloqueada", "urgent"),
        ("alta", "high"),
        ("urgente escalada", "urgent"),
    ] {
        t.run(
            "task.create",
            &[("project", "API"), ("title", title), ("priority", priority)],
        );
    }
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "bloqueador"),
            ("priority", "none"),
        ],
    );
    t.run("task.link", &[("blocker", "API-5"), ("blocked", "API-2")]);
    let a = t.agent("a");
    t.as_(&a).run("task.claim", &[("task", "API-4")]);
    t.run(
        "task.escalate",
        &[("task", "API-4"), ("question", "¿qué hago?")],
    );

    let next = t.run("task.next", &[]);
    assert_eq!(
        next["task"]["key"], "API-3",
        "alta: la urgente está bloqueada y la otra escalada"
    );
    assert_eq!(next["claimed"], false);

    let next = t.run("task.next", &[("claim", "true")]);
    assert_eq!(next["task"]["key"], "API-3");
    assert_eq!(next["task"]["claim"]["agent"], "a");

    let b = t.agent("b");
    let next = t.as_(&b).run("task.next", &[("claim", "true")]);
    assert_eq!(next["task"]["key"], "API-1", "baja");
    let c = t.agent("c");
    let next = t.as_(&c).run("task.next", &[("claim", "true")]);
    assert_eq!(
        next["task"]["key"], "API-5",
        "ninguna prioridad, pero libre"
    );
    let d = t.agent("d");
    assert_eq!(t.as_(&d).run("task.next", &[])["task"], Value::Null);
}

#[test]
fn expired_leases_are_free_and_get_swept() {
    let mut t = T::with_tasks(1);
    let (a, b) = (t.agent("a"), t.agent("b"));
    let t0 = Timestamp::now();

    t.clock = Some(t0);
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    t.clock = Some(minutes(t0, 10));
    t.run("task.heartbeat", &[("task", "API-1")]);
    t.clock = Some(minutes(t0, 20));
    assert_eq!(
        t.as_(&b).fail("task.claim", &[("task", "API-1")]).code(),
        "CLAIMED_BY_OTHER",
        "el latido de los 10 min lo extendió hasta los 25"
    );

    t.clock = Some(minutes(t0, 26));
    let swept = t.as_(&Actor::System).run("system.expire_leases", &[]);
    assert_eq!(swept["expired"], serde_json::json!(["API-1"]));
    let task = t.status("API-1");
    assert_eq!(task["status"], "todo");
    assert_eq!(task["claim"], Value::Null);
    assert_eq!(task["assignee"], Value::Null);

    let history = t
        .as_(&Actor::Supervisor)
        .run("activity.list", &[("task", "API-1"), ("limit", "1")]);
    assert_eq!(history["items"][0]["command"], "system.expire_leases");
    assert_eq!(history["items"][0]["actor_name"], "system");

    t.as_(&b).run("task.claim", &[("task", "API-1")]);
}

#[test]
fn an_expired_lease_counts_as_free_before_the_sweep() {
    let mut t = T::with_tasks(1);
    let (a, b) = (t.agent("a"), t.agent("b"));
    let t0 = Timestamp::now();
    t.clock = Some(t0);
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    t.clock = Some(minutes(t0, 16));
    let task = t.as_(&b).run("task.claim", &[("task", "API-1")]);
    assert_eq!(task["claim"]["agent"], "b");
}

#[test]
fn any_write_by_the_holder_renews_the_lease() {
    let mut t = T::with_tasks(1);
    let (a, b) = (t.agent("a"), t.agent("b"));
    let t0 = Timestamp::now();
    t.clock = Some(t0);
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    t.clock = Some(minutes(t0, 12));
    t.run("checklist.add", &[("task", "API-1"), ("text", "avance")]);
    t.clock = Some(minutes(t0, 20));
    assert_eq!(
        t.as_(&b).fail("task.claim", &[("task", "API-1")]).code(),
        "CLAIMED_BY_OTHER"
    );
    let renewals = t
        .as_(&Actor::Supervisor)
        .run("activity.list", &[("task", "API-1")])["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["command"] == "task.heartbeat")
        .count();
    assert_eq!(renewals, 0, "los latidos no ensucian el historial");
}

#[test]
fn release_requires_a_reason_from_agents() {
    let mut t = T::with_tasks(2);
    let a = t.agent("a");
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    assert!(matches!(
        t.fail("task.release", &[("task", "API-1")]),
        EngineError::MissingArg(ref n) if n == "reason"
    ));
    let task = t.run(
        "task.release",
        &[("task", "API-1"), ("reason", "faltan credenciales")],
    );
    assert_eq!(task["status"], "todo");
    assert_eq!(task["claim"], Value::Null);
    assert_eq!(task["comments"], 1);

    t.run("task.claim", &[("task", "API-2")]);
    let task = t
        .as_(&Actor::Supervisor)
        .run("task.release", &[("task", "API-2")]);
    assert_eq!(
        task["claim"],
        Value::Null,
        "el supervisor libera sin motivo"
    );
}

#[test]
fn optimistic_concurrency_with_revisions() {
    let mut t = T::with_tasks(1);
    let rev = t.run("task.get", &[("task", "API-1")])["revision"]
        .as_u64()
        .unwrap();
    t.run("task.update", &[("task", "API-1"), ("title", "primero")]);
    let err = t.fail(
        "task.update",
        &[
            ("task", "API-1"),
            ("title", "segundo"),
            ("revision", &rev.to_string()),
        ],
    );
    assert_eq!(code_key(&err), ("CONFLICT", "conflict-revision"));
    assert_eq!(
        err.current().unwrap()["title"],
        "primero",
        "el conflicto trae el estado actual"
    );
}

#[test]
fn supervisor_corrections_prevail_over_a_claim() {
    let mut t = T::with_tasks(1);
    let a = t.agent("a");
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    let current = t
        .as_(&Actor::Supervisor)
        .run("task.update", &[("task", "API-1"), ("title", "corregido")])["revision"]
        .as_u64()
        .unwrap();

    let err = t
        .as_(&a)
        .fail("task.update", &[("task", "API-1"), ("priority", "high")]);
    assert_eq!(code_key(&err), ("CONFLICT", "conflict-changed-by-other"));
    t.run(
        "task.update",
        &[
            ("task", "API-1"),
            ("priority", "high"),
            ("revision", &current.to_string()),
        ],
    );
    t.run("task.update", &[("task", "API-1"), ("priority", "urgent")]);
}

#[test]
fn submit_for_review_marks_criteria_and_anyone_may_close() {
    let mut t = T::with_tasks(1);
    t.run("criteria.add", &[("task", "API-1"), ("text", "compila")]);
    t.run("criteria.add", &[("task", "API-1"), ("text", "tests")]);
    let a = t.agent("a");
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    let task = t.run(
        "task.submit_review",
        &[
            ("task", "API-1"),
            ("summary", "Hecho, ver commit abc"),
            ("criteria", "1,2"),
        ],
    );
    assert_eq!(task["status"], "in_review");
    assert_eq!(task["claim"], Value::Null);
    assert_eq!(
        task["assignee_name"], "a",
        "sigue asignada para la revisión"
    );
    assert!(
        task["criteria"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["done"] == true)
    );
    let b = t.agent("b");
    let err = t
        .as_(&b)
        .fail("task.submit_review", &[("task", "API-1"), ("summary", "x")]);
    assert_eq!(err.key(), "not-your-claim");
    // Nadie tiene que aprobarla (D20): quien la entregó la puede cerrar.
    let done = t
        .as_(&a)
        .run("task.move", &[("task", "API-1"), ("status", "done")]);
    assert_eq!(done["status"], "done");
}

#[test]
fn escalation_blocks_agents_until_the_supervisor_answers() {
    let mut t = T::with_tasks(1);
    let (a, b) = (t.agent("a"), t.agent("b"));
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    let task = t.run(
        "task.escalate",
        &[("task", "API-1"), ("question", "¿Postgres o SQLite?")],
    );
    assert_eq!(task["needs_human"], "¿Postgres o SQLite?");
    assert_eq!(task["claim"], Value::Null);

    let err = t.as_(&b).fail("task.claim", &[("task", "API-1")]);
    assert_eq!(code_key(&err), ("LOCKED_BY_HUMAN", "needs-human-pending"));
    assert_eq!(t.run("task.next", &[])["task"], Value::Null);
    assert_eq!(
        t.fail("task.answer", &[("task", "API-1"), ("answer", "x")])
            .key(),
        "supervisor-only"
    );

    t.as_(&Actor::Supervisor)
        .run("task.answer", &[("task", "API-1"), ("answer", "SQLite")]);
    assert_eq!(t.status("API-1")["needs_human"], Value::Null);
    assert_eq!(t.status("API-1")["comments"], 2, "pregunta y respuesta");
    assert_eq!(
        t.fail("task.answer", &[("task", "API-1"), ("answer", "x")])
            .key(),
        "answer-no-question"
    );
}

#[test]
fn human_lock_on_task_subtree_and_project() {
    let mut t = T::with_tasks(2);
    t.run("task.create", &[("parent", "API-1"), ("title", "hija")]);
    let a = t.agent("a");
    t.run("lock.set", &[("task", "API-1")]);

    for key in ["API-1", "API-3"] {
        let err = t
            .as_(&a)
            .fail("task.update", &[("task", key), ("title", "x")]);
        assert_eq!(code_key(&err), ("LOCKED_BY_HUMAN", "locked-task"), "{key}");
    }
    t.as_(&a).run("task.get", &[("task", "API-1")]);
    t.as_(&Actor::Supervisor).run(
        "task.update",
        &[("task", "API-1"), ("title", "el supervisor sí")],
    );

    t.run("lock.set", &[("project", "API")]);
    let err = t
        .as_(&a)
        .fail("task.update", &[("task", "API-2"), ("title", "x")]);
    assert_eq!(err.key(), "locked-project");
    assert_eq!(t.run("task.next", &[])["task"], Value::Null);
    assert_eq!(
        t.fail("lock.clear", &[("project", "API")]).key(),
        "supervisor-only"
    );

    t.as_(&Actor::Supervisor)
        .run("lock.clear", &[("project", "API")]);
    t.run("lock.clear", &[("task", "API-1")]);
    t.as_(&a)
        .run("task.update", &[("task", "API-3"), ("title", "ya puedo")]);
}

#[test]
fn batch_planning_is_all_or_nothing() {
    let mut t = T::with_tasks(1);
    let plan = r#"[
        {"ref": "api", "title": "API REST", "priority": "high", "criteria": ["documentada"]},
        {"ref": "auth", "title": "Login", "parent": "api", "checklist": ["JWT", "tests"]},
        {"ref": "users", "title": "Usuarios", "parent": "api", "blocked_by": ["auth", "API-1"]},
        {"title": "Despliegue", "blocked_by": ["users"], "labels": ["ops"]}
    ]"#;
    let out = t.run("plan.create", &[("project", "API"), ("items", plan)]);
    assert_eq!(out["count"], 4);
    let users = t.run("task.get", &[("task", "API-4")]);
    assert_eq!(users["parent"], "API-2");
    assert_eq!(users["blocked_by"], serde_json::json!(["API-3", "API-1"]));
    assert_eq!(
        t.run("task.get", &[("task", "API-3")])["checklist"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let before = t.run("task.list", &[])["count"].clone();
    let cyclic = r#"[
        {"ref": "x", "title": "X", "blocked_by": ["y"]},
        {"ref": "y", "title": "Y", "blocked_by": ["x"]}
    ]"#;
    assert_eq!(
        t.fail("plan.create", &[("project", "API"), ("items", cyclic)])
            .key(),
        "dependency-cycle"
    );
    assert_eq!(
        t.run("task.list", &[])["count"],
        before,
        "no quedó nada a medias"
    );

    let orphan =
        r#"[{"ref": "a", "title": "A", "parent": "b"}, {"ref": "b", "title": "B", "parent": "a"}]"#;
    assert_eq!(
        t.fail("plan.create", &[("project", "API"), ("items", orphan)])
            .key(),
        "plan-unresolved"
    );
    let dup = r#"[{"ref": "a", "title": "A"}, {"ref": "a", "title": "B"}]"#;
    assert_eq!(
        t.fail("plan.create", &[("project", "API"), ("items", dup)])
            .key(),
        "plan-duplicate-ref"
    );
    assert_eq!(
        t.fail(
            "plan.create",
            &[("project", "API"), ("items", "{no es json")]
        )
        .key(),
        "plan-invalid"
    );
}

#[test]
fn inbox_findings_are_triaged_by_the_supervisor() {
    let mut t = T::with_tasks(0);
    let a = t.agent("a");
    let item = t.as_(&a).run(
        "inbox.add",
        &[
            ("title", "Fuga de memoria en el parser"),
            ("kind", "bug"),
            ("body", "Se ve con 10k tareas"),
        ],
    );
    let id = item["id"].as_str().unwrap().to_owned();
    assert_eq!(item["author_name"], "a");
    assert_eq!(
        t.fail("inbox.convert", &[("item", &id), ("project", "API")])
            .key(),
        "supervisor-only"
    );

    t.as_(&Actor::Supervisor);
    assert_eq!(t.run("inbox.list", &[])["count"], 1);
    let converted = t.run(
        "inbox.convert",
        &[("item", &id), ("project", "API"), ("priority", "high")],
    );
    assert_eq!(converted["task"], "API-1");
    let task = t.run("task.get", &[("task", "API-1")]);
    assert_eq!(task["labels"], serde_json::json!(["bug"]));
    assert_eq!(task["description"], "Se ve con 10k tareas");
    assert_eq!(t.run("inbox.list", &[])["count"], 0);
    assert_eq!(
        t.fail("inbox.discard", &[("item", &id)]).key(),
        "inbox-not-open"
    );
}

/// 20 agentes piden a la vez "la siguiente tarea" sobre 30: cada uno se lleva
/// una distinta (límite de WIP = 1) y no se reparte ninguna dos veces.
#[test]
fn twenty_agents_never_take_the_same_task() {
    let mut engine = super::test_engine();
    let sup = Context::supervisor();
    engine
        .dispatch(&sup, "project.create", &Args::none().with("key", "SWM"))
        .unwrap();
    for i in 0..30 {
        engine
            .dispatch(
                &sup,
                "task.create",
                &Args::none()
                    .with("project", "SWM")
                    .with("title", format!("t{i}")),
            )
            .unwrap();
    }
    let swarm = Swarm::new(engine, 20).unwrap();
    let results = swarm.run(|_, agent| {
        agent
            .dispatch("task.next", &Args::none().with("claim", "true"))
            .map(|v| v["task"]["key"].as_str().map(str::to_owned))
    });
    let mut keys: Vec<String> = results
        .into_iter()
        .map(|r| r.unwrap().unwrap().unwrap())
        .collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 20, "20 tareas distintas");
}

#[test]
fn supervisor_edits_labels_items_and_rejects_submissions() {
    let mut t = T::with_tasks(1);
    t.run("task.label", &[("task", "API-1"), ("add", "bug")]);
    let l = t.run(
        "label.update",
        &[("label", "bug"), ("name", "defecto"), ("color", "#112233")],
    );
    assert_eq!(
        (l["name"].as_str(), l["color"].as_str()),
        (Some("defecto"), Some("#112233"))
    );
    assert_eq!(
        t.run("task.get", &[("task", "API-1")])["labels"],
        serde_json::json!(["defecto"])
    );
    t.run("label.create", &[("name", "otra")]);
    assert_eq!(
        t.fail("label.update", &[("label", "defecto"), ("name", "OTRA")])
            .key(),
        "label-taken"
    );

    t.run("checklist.add", &[("task", "API-1"), ("text", "borrador")]);
    let task = t.run(
        "checklist.edit",
        &[("task", "API-1"), ("item", "1"), ("text", "definitivo")],
    );
    assert_eq!(task["checklist"][0]["text"], "definitivo");
    t.run("criteria.add", &[("task", "API-1"), ("text", "a")]);
    let task = t.run(
        "criteria.edit",
        &[("task", "API-1"), ("item", "1"), ("text", "b")],
    );
    assert_eq!(task["criteria"][0]["text"], "b");

    let a = t.agent("a");
    assert_eq!(
        t.fail("task.reject", &[("task", "API-1"), ("reason", "x")])
            .key(),
        "reject-not-in-review"
    );
    t.as_(&a).run("task.claim", &[("task", "API-1")]);
    t.run(
        "task.submit_review",
        &[("task", "API-1"), ("summary", "listo")],
    );
    assert_eq!(
        t.fail("task.reject", &[("task", "API-1"), ("reason", "x")])
            .key(),
        "supervisor-only"
    );
    let task = t.as_(&Actor::Supervisor).run(
        "task.reject",
        &[
            ("task", "API-1"),
            ("reason", "Faltan tests"),
            ("keep_assignee", "true"),
        ],
    );
    assert_eq!(task["status"], "todo");
    assert_eq!(task["assignee_name"], "a");
}
