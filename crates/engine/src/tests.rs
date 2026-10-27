//! Escenarios de los comandos de P1/P2 contra un workspace SQLite en memoria.

use model::{Actor, AgentId};
use serde_json::Value;
use store_sqlite::SqliteStore;

use i18n::{Arg, Lang, Localize};

use crate::testing::Boom;
use crate::{Args, Context, Engine, EngineError};

struct T {
    engine: Engine,
    actor: Actor,
    /// Hora fija para los comandos; `None` = reloj real.
    clock: Option<model::Timestamp>,
}

/// Engine de pruebas: SQLite en memoria o, con `TASKCRAFT_TEST_PG`, un
/// esquema nuevo de PostgreSQL. Toda la batería pasa en los dos (D13).
pub(crate) fn test_engine() -> Engine {
    match std::env::var("TASKCRAFT_TEST_PG") {
        Ok(url) => {
            let schema = format!(
                "t_{}",
                foundation::Ulid::generate().to_string().to_lowercase()
            );
            Engine::new(store_postgres::PostgresStore::connect_isolated(&url, &schema).unwrap())
        }
        Err(_) => Engine::new(SqliteStore::open_in_memory().unwrap()),
    }
}

impl T {
    fn new() -> Self {
        Self {
            engine: test_engine(),
            actor: Actor::Supervisor,
            clock: None,
        }
    }

    /// Interfaz con su token y, dentro, un agente con el mismo nombre (D22).
    /// Devuelve el agente con el `token` de la interfaz.
    fn register(&mut self, args: &[(&str, &str)]) -> Value {
        let prev = std::mem::replace(&mut self.actor, Actor::Supervisor);
        let name = args
            .iter()
            .find(|(k, _)| *k == "name")
            .map(|(_, v)| (*v).to_owned())
            .unwrap();
        let interface = self.run("interface.register", &[("name", &name)]);
        let mut all = args.to_vec();
        all.push(("interface", &name));
        let mut agent = self.run("agent.register", &all);
        agent["token"] = interface["token"].clone();
        self.actor = prev;
        agent
    }

    /// Registra (como supervisor) un agente nuevo y pasa a actuar como él.
    fn as_agent(&mut self) -> &mut Self {
        self.actor = Actor::Supervisor;
        let name = format!("agente-{}", AgentId::generate().to_string().to_lowercase());
        let agent = self.register(&[("name", &name), ("model", "test")]);
        self.actor = Actor::Agent(agent["id"].as_str().unwrap().parse().unwrap());
        self
    }

    fn try_run(&mut self, id: &str, args: &[(&str, &str)]) -> Result<Value, EngineError> {
        let args = args.iter().fold(Args::none(), |a, (k, v)| a.with(*k, *v));
        let ctx = Context::new(self.actor.clone());
        match self.clock {
            Some(now) => self.engine.dispatch_at(&ctx, id, &args, now),
            None => self.engine.dispatch(&ctx, id, &args),
        }
    }

    fn run(&mut self, id: &str, args: &[(&str, &str)]) -> Value {
        self.try_run(id, args)
            .unwrap_or_else(|e| panic!("{id} falló: {e}"))
    }

    fn fail(&mut self, id: &str, args: &[(&str, &str)]) -> EngineError {
        match self.try_run(id, args) {
            Ok(v) => panic!("{id} debía fallar y devolvió {v}"),
            Err(e) => e,
        }
    }

    fn keys(&mut self, args: &[(&str, &str)]) -> Vec<String> {
        self.run("task.list", args)["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["key"].as_str().unwrap().to_owned())
            .collect()
    }

    /// Proyecto API con tareas API-1..=n.
    fn with_tasks(n: usize) -> Self {
        let mut t = Self::new();
        t.run("project.create", &[("key", "api"), ("name", "Backend")]);
        for i in 1..=n {
            t.run(
                "task.create",
                &[("project", "API"), ("title", &format!("Tarea {i}"))],
            );
        }
        t
    }
}

#[test]
fn projects() {
    let mut t = T::new();
    let p = t.run("project.create", &[("key", "api"), ("name", "Backend")]);
    assert_eq!(p["key"], "API");

    assert!(t.fail("project.create", &[("key", "API")]).key() == "project-key-taken");
    assert!(t.fail("project.create", &[("key", "1X")]).key() == "rule-project-key");

    t.run(
        "project.update",
        &[("project", "API"), ("description", "# Hola")],
    );
    assert_eq!(
        t.run("project.get", &[("project", "api")])["description"],
        "# Hola"
    );

    t.run("project.archive", &[("project", "API")]);
    assert_eq!(t.run("project.list", &[])["count"], 0);
    assert_eq!(t.run("project.list", &[("archived", "true")])["count"], 1);
    assert!(
        t.fail("task.create", &[("project", "API"), ("title", "x")])
            .key()
            == "project-archived"
    );
}

#[test]
fn task_fields_and_lookup() {
    let mut t = T::with_tasks(0);
    let task = t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "  Corregir leak  "),
            ("description", "**urgente**"),
            ("priority", "alta"),
            ("due", "2026-11-01"),
            ("labels", "rust, bug"),
        ],
    );
    assert_eq!(task["key"], "API-1");
    assert_eq!(task["title"], "Corregir leak");
    assert_eq!(task["priority"], "high");
    assert_eq!(task["status"], "todo");
    assert_eq!(task["due"], "2026-11-01");
    assert_eq!(task["labels"], serde_json::json!(["bug", "rust"]));
    assert_eq!(task["creator"], "supervisor");

    let id = task["id"].as_str().unwrap().to_owned();
    assert_eq!(t.run("task.get", &[("task", &id)])["key"], "API-1");
    assert!(t.fail("task.get", &[("task", "API-99")]).key() == "notfound-task");

    let updated = t.run("task.update", &[("task", "api-1"), ("due", "")]);
    assert_eq!(updated["due"], Value::Null);
    assert_eq!(updated["revision"], 2);

    assert!(
        t.fail(
            "task.create",
            &[("project", "API"), ("title", "x"), ("due", "mañana")]
        )
        .key()
            == "arg-bad-date"
    );
}

#[test]
fn list_filters() {
    let mut t = T::with_tasks(3);
    t.run("task.label", &[("task", "API-2"), ("add", "docs")]);
    t.run("task.move", &[("task", "API-3"), ("status", "in_progress")]);
    t.run("task.create", &[("parent", "API-1"), ("title", "Sub")]);

    assert_eq!(
        t.keys(&[("project", "API")]),
        ["API-1", "API-2", "API-3", "API-4"]
    );
    assert_eq!(t.keys(&[("label", "docs")]), ["API-2"]);
    assert_eq!(t.keys(&[("status", "in_progress,backlog")]), ["API-3"]);
    assert_eq!(t.keys(&[("parent", "root")]), ["API-1", "API-2", "API-3"]);
    assert_eq!(t.keys(&[("parent", "API-1")]), ["API-4"]);
}

#[test]
fn hierarchy_and_depth_limit() {
    let mut t = T::with_tasks(1);
    t.run("workspace.configure", &[("max_depth", "2")]);

    let child = t.run("task.create", &[("parent", "API-1"), ("title", "Hija")]);
    assert_eq!(child["parent"], "API-1");
    assert_eq!(
        t.run("task.get", &[("task", "API-1")])["subtasks"],
        serde_json::json!(["API-2"])
    );

    assert!(
        t.fail("task.create", &[("parent", "API-2"), ("title", "Nieta")])
            .key()
            == "depth-exceeded"
    );
    assert!(
        t.fail("task.set_parent", &[("task", "API-1"), ("parent", "API-2")])
            .key()
            == "parent-is-descendant"
    );

    t.run("task.create", &[("project", "API"), ("title", "Otra raíz")]);
    // API-1 (con hija) bajo API-3 daría 3 niveles.
    assert!(
        t.fail("task.set_parent", &[("task", "API-1"), ("parent", "API-3")])
            .key()
            == "depth-exceeded"
    );
    t.run("task.set_parent", &[("task", "API-2"), ("parent", "none")]);
    assert_eq!(t.keys(&[("parent", "root")]), ["API-1", "API-2", "API-3"]);
}

#[test]
fn checklist_and_criteria() {
    let mut t = T::with_tasks(1);
    t.run(
        "checklist.add",
        &[("task", "API-1"), ("text", "Escribir test")],
    );
    let task = t.run("checklist.add", &[("task", "API-1"), ("text", "Arreglar")]);
    let second = task["checklist"][1]["id"].as_str().unwrap().to_owned();

    let task = t.run("checklist.check", &[("task", "API-1"), ("item", "1")]);
    assert_eq!(task["checklist"][0]["done"], true);
    let task = t.run("checklist.check", &[("task", "API-1"), ("item", &second)]);
    assert_eq!(task["checklist"][1]["done"], true);
    let task = t.run(
        "checklist.check",
        &[("task", "API-1"), ("item", "1"), ("done", "false")],
    );
    assert_eq!(task["checklist"][0]["done"], false);
    assert!(
        t.fail("checklist.check", &[("task", "API-1"), ("item", "9")])
            .key()
            == "notfound-checklist-item"
    );
    let task = t.run("checklist.remove", &[("task", "API-1"), ("item", "1")]);
    assert_eq!(task["checklist"][0]["text"], "Arreglar");

    let task = t.run(
        "criteria.add",
        &[("task", "API-1"), ("text", "Sin regresiones")],
    );
    assert_eq!(task["criteria"][0]["done"], false);
    let task = t.run("criteria.check", &[("task", "API-1"), ("item", "1")]);
    assert_eq!(task["criteria"][0]["done"], true);
    assert_eq!(
        task["checklist"].as_array().unwrap().len(),
        1,
        "no se mezclan"
    );
}

#[test]
fn transitions_and_anyone_closes() {
    let mut t = T::with_tasks(1);
    assert!(
        t.fail("task.move", &[("task", "API-1"), ("status", "done")])
            .key()
            == "rule-transition"
    );
    // Sin revisión obligatoria (D20): un agente la hace y la cierra.
    t.as_agent();
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    t.run("task.move", &[("task", "API-1"), ("status", "in_review")]);
    let done = t.run("task.move", &[("task", "API-1"), ("status", "done")]);
    assert_eq!(done["status"], "done");
}

#[test]
fn dependencies_and_readiness() {
    let mut t = T::with_tasks(3);
    let link = t.run("task.link", &[("blocker", "API-1"), ("blocked", "API-2")]);
    assert_eq!(link["blocked_ready"], false);
    t.run("task.link", &[("blocker", "API-2"), ("blocked", "API-3")]);

    let err = t.fail("task.link", &[("blocker", "API-3"), ("blocked", "API-1")]);
    let EngineError::Invalid(msg) = &err else {
        panic!("{err:?}");
    };
    assert_eq!(
        msg.get("path"),
        Some(&Arg::Text("API-3 -> API-1 -> API-2 -> API-3".into()))
    );
    assert!(
        t.fail("task.link", &[("blocker", "API-1"), ("blocked", "API-1")])
            .key()
            == "dependency-cycle"
    );

    let b = t.run("task.get", &[("task", "API-2")]);
    assert_eq!(b["blocked_by"], serde_json::json!(["API-1"]));
    assert_eq!(b["blocks"], serde_json::json!(["API-3"]));

    t.run("task.move", &[("task", "API-1"), ("status", "cancelled")]);
    assert_eq!(t.run("task.get", &[("task", "API-2")])["ready"], true);

    t.run("task.unlink", &[("blocker", "API-2"), ("blocked", "API-3")]);
    assert!(
        t.fail("task.unlink", &[("blocker", "API-2"), ("blocked", "API-3")])
            .key()
            == "notfound-dependency"
    );
}

#[test]
fn archive() {
    let mut t = T::with_tasks(1);
    t.run("task.create", &[("parent", "API-1"), ("title", "Hija")]);
    t.run("task.move", &[("task", "API-1"), ("status", "cancelled")]);
    assert!(t.fail("task.archive", &[("task", "API-1")]).key() == "archive-open-tasks");

    t.run("task.move", &[("task", "API-2"), ("status", "cancelled")]);
    t.run("task.archive", &[("task", "API-1")]);
    assert!(t.keys(&[("project", "API")]).is_empty());
    assert_eq!(t.keys(&[("archived", "true")]), ["API-1", "API-2"]);
    assert!(
        t.fail("task.update", &[("task", "API-1"), ("title", "x")])
            .key()
            == "task-archived"
    );

    t.run("task.unarchive", &[("task", "API-1")]);
    assert_eq!(t.keys(&[]), ["API-1", "API-2"]);

    let auto = t.run("task.auto_archive", &[("days", "0")]);
    assert_eq!(auto["archived"], serde_json::json!(["API-1"]));
}

#[test]
fn trash() {
    let mut t = T::with_tasks(2);
    t.run("task.create", &[("parent", "API-1"), ("title", "Hija")]);
    t.run("task.delete", &[("task", "API-3")]);
    let deleted = t.run("task.delete", &[("task", "API-1")]);
    assert_eq!(deleted["count"], 1, "API-3 ya estaba en la papelera");

    assert_eq!(t.keys(&[]), ["API-2"]);
    assert!(t.fail("task.get", &[("task", "API-1")]).key() == "task-in-trash");
    assert_eq!(t.run("trash.list", &[])["count"], 2);
    assert!(t.fail("task.restore", &[("task", "API-3")]).key() == "parent-in-trash");

    t.run("task.restore", &[("task", "API-1")]);
    assert_eq!(t.keys(&[]), ["API-1", "API-2"], "API-3 se borró aparte");

    let purged = t.run("trash.purge", &[("days", "0")]);
    assert_eq!(purged["purged"], 1);
    assert_eq!(t.run("trash.list", &[])["count"], 0);
}

#[test]
fn failed_command_leaves_nothing_behind() {
    let mut t = T::with_tasks(0);
    let too_long = "x".repeat(41);
    t.fail(
        "task.create",
        &[
            ("project", "API"),
            ("title", "x"),
            ("labels", &format!("valida,{too_long}")),
        ],
    );
    assert_eq!(
        t.run("label.list", &[])["count"],
        0,
        "la etiqueta creada se revirtió"
    );
    let next = t.run("task.create", &[("project", "API"), ("title", "x")]);
    assert_eq!(next["key"], "API-1", "el contador no avanzó");

    t.engine.register(Boom);
    assert!(matches!(
        t.try_run(Boom::ID, &[]),
        Err(EngineError::Panicked(_))
    ));
    assert!(t.fail("project.get", &[("project", Boom::KEY)]).key() == "notfound-project");
}

#[test]
fn every_command_has_a_description_in_both_languages() {
    let mut t = T::new();
    t.engine.register(Boom);
    for c in t.engine.registry().iter() {
        assert!(c.id().contains('.'), "{}", c.id());
        for lang in Lang::ALL {
            assert!(i18n::has(lang, c.label()), "{lang}: falta {}", c.label());
            for p in c.params() {
                assert!(
                    i18n::has(lang, &p.help_key()),
                    "{lang}: falta {}",
                    p.help_key()
                );
            }
        }
    }
}

#[test]
fn errors_are_localized_with_nested_reasons() {
    let mut t = T::with_tasks(0);
    let err = t.fail(
        "task.create",
        &[("project", "API"), ("title", "x"), ("priority", "ya")],
    );
    assert_eq!(err.code(), "INVALID");
    let es = err.localize(Lang::Es);
    let en = err.localize(Lang::En);
    assert!(
        es.starts_with("Argumento `priority` inválido: Prioridad inválida `ya`"),
        "{es}"
    );
    assert!(
        en.starts_with("Invalid argument `priority`: Invalid priority `ya`"),
        "{en}"
    );
    assert_eq!(err.to_string(), en, "Display = inglés, para logs");

    let err = t.fail("project.get", &[("project", "NOPE")]);
    assert_eq!(err.code(), "NOT_FOUND");
    assert!(
        err.localize(Lang::Es)
            .starts_with("No existe el proyecto `NOPE`")
    );
}

// ── P3: historial, agentes, sesiones y comentarios ───────────────────────

impl T {
    fn activity(&mut self, args: &[(&str, &str)]) -> Vec<Value> {
        self.run("activity.list", args)["items"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn agent_id(&mut self, name: &str) -> String {
        let prev = std::mem::replace(&mut self.actor, Actor::Supervisor);
        let id = self.run("agent.get", &[("agent", name)])["id"]
            .as_str()
            .unwrap()
            .to_owned();
        self.actor = prev;
        id
    }
}

#[test]
fn history_records_who_changed_what() {
    let mut t = T::with_tasks(1);
    t.run(
        "task.update",
        &[("task", "API-1"), ("title", "Nuevo título")],
    );
    t.run("task.get", &[("task", "API-1")]);
    t.run("task.list", &[]);

    let history = t.activity(&[("task", "API-1")]);
    assert_eq!(
        history.len(),
        2,
        "crear + editar; lo que solo lee no deja rastro"
    );
    let update = &history[0];
    assert_eq!(update["action"], "updated");
    assert_eq!(update["command"], "task.update");
    assert_eq!(update["actor_name"], "supervisor");
    assert_eq!(update["target"], "API-1");
    assert_eq!(
        update["changes"],
        serde_json::json!([{ "field": "title", "before": "Tarea 1", "after": "Nuevo título" }])
    );
    assert_eq!(history[1]["action"], "created");
}

#[test]
fn failed_or_panicking_commands_leave_no_history() {
    let mut t = T::with_tasks(1);
    let before = t.activity(&[]).len();
    t.fail("task.update", &[("task", "API-1"), ("title", "")]);
    t.engine.register(Boom);
    let _ = t.try_run(Boom::ID, &[]);
    assert_eq!(t.activity(&[]).len(), before);
}

#[test]
fn history_covers_dependencies_and_deletions() {
    let mut t = T::with_tasks(2);
    t.run("task.link", &[("blocker", "API-1"), ("blocked", "API-2")]);
    t.run("task.delete", &[("task", "API-1")]);
    let history = t.activity(&[("limit", "2")]);
    assert_eq!(history[0]["action"], "deleted");
    assert_eq!(history[0]["target"], "API-1");
    assert_eq!(history[1]["action"], "linked");
    assert_eq!(history[1]["target"], "API-1 -> API-2");
}

#[test]
fn history_filters_by_action_and_entity() {
    let mut t = T::with_tasks(2);
    t.run("task.link", &[("blocker", "API-1"), ("blocked", "API-2")]);
    t.run("comment.add", &[("task", "API-1"), ("body", "nota")]);
    t.run("task.update", &[("task", "API-2"), ("title", "Otra")]);

    let links = t.activity(&[("action", "linked,unlinked")]);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["action"], "linked");

    let comments = t.activity(&[("entity", "comment")]);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["command"], "comment.add");

    let created_tasks = t.activity(&[("action", "created"), ("entity", "task")]);
    assert_eq!(created_tasks.len(), 2);

    let bad = t.fail("activity.list", &[("action", "exploded")]);
    assert_eq!(bad.code(), "INVALID");
}

#[test]
fn agents_are_attributed_by_name() {
    let mut t = T::with_tasks(0);
    t.actor = Actor::Supervisor;
    t.register(&[("name", "Claude-1"), ("model", "claude-opus-5-5")]);
    let id = t.agent_id("claude-1");
    t.actor = Actor::Agent(id.parse().unwrap());

    let task = t.run(
        "task.create",
        &[("project", "API"), ("title", "Hecha por IA")],
    );
    assert_eq!(task["creator_name"], "claude-1");
    let history = t.activity(&[("actor", "claude-1")]);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["actor_name"], "claude-1");
}

#[test]
fn agent_lifecycle_and_permissions() {
    let mut t = T::with_tasks(0);
    t.as_agent();
    assert_eq!(
        t.fail("agent.register", &[("name", "otro"), ("interface", "x")])
            .key(),
        "supervisor-only"
    );
    let agent = t.actor.clone();
    let model::Actor::Agent(id) = agent else {
        panic!("se esperaba un agente")
    };
    let id = id.to_string();

    t.actor = Actor::Supervisor;
    // D22: no hay agentes sin interfaz.
    assert_eq!(
        t.fail("agent.register", &[("name", "suelto")]).code(),
        "INVALID"
    );
    t.run("interface.register", &[("name", "Cliente")]);
    assert_eq!(
        t.fail(
            "agent.register",
            &[("name", "no válido"), ("interface", "cliente")]
        )
        .key(),
        "rule-agent-name"
    );
    t.run("agent.disable", &[("agent", &id)]);
    t.actor = agent.clone();
    let err = t.fail("task.create", &[("project", "API"), ("title", "x")]);
    assert_eq!((err.code(), err.key()), ("FORBIDDEN", "agent-disabled"));

    t.actor = Actor::Supervisor;
    t.run("agent.enable", &[("agent", &id)]);
    t.actor = agent.clone();
    t.run("task.create", &[("project", "API"), ("title", "x")]);

    t.actor = Actor::Supervisor;
    t.run("agent.revoke", &[("agent", &id)]);
    assert_eq!(
        t.fail("agent.enable", &[("agent", &id)]).key(),
        "agent-revoked-final"
    );
    t.actor = agent;
    assert_eq!(
        t.fail("task.get", &[("task", "API-1")]).key(),
        "agent-revoked"
    );

    t.actor = Actor::Supervisor;
    let creator = t.run("task.get", &[("task", "API-1")])["creator_name"].clone();
    assert!(
        creator.as_str().unwrap().starts_with("agente-"),
        "el nombre sobrevive: {creator}"
    );

    t.actor = Actor::Agent(model::AgentId::generate());
    assert_eq!(t.fail("task.list", &[]).key(), "agent-unknown");
}

#[test]
fn sessions_track_connected_agents() {
    let mut t = T::with_tasks(0);
    assert_eq!(t.fail("session.open", &[]).key(), "session-agent-only");

    t.as_agent();
    let first = t.actor.clone();
    let session = t.run("session.open", &[("transport", "stdio")])["session"]
        .as_str()
        .unwrap()
        .to_owned();
    t.run("session.heartbeat", &[("session", &session)]);

    t.actor = Actor::Supervisor;
    let agents = t.run("agent.list", &[]);
    assert_eq!(agents["items"][0]["connected"], true);

    t.as_agent();
    assert_eq!(
        t.fail("session.heartbeat", &[("session", &session)]).key(),
        "session-other-agent"
    );

    t.actor = first;
    t.run("session.close", &[("session", &session)]);
    assert_eq!(
        t.fail("session.heartbeat", &[("session", &session)]).key(),
        "notfound-session"
    );
    t.actor = Actor::Supervisor;
    let agents = t.run("agent.list", &[]);
    assert!(
        agents["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["connected"] == false)
    );
}

#[test]
fn disabling_an_agent_closes_its_sessions() {
    let mut t = T::with_tasks(0);
    t.as_agent();
    let agent = t.actor.clone();
    t.run("session.open", &[]);
    let model::Actor::Agent(id) = agent else {
        panic!("se esperaba un agente")
    };
    t.actor = Actor::Supervisor;
    t.run("agent.disable", &[("agent", &id.to_string())]);
    assert_eq!(
        t.run("agent.get", &[("agent", &id.to_string())])["connected"],
        false
    );
}

#[test]
fn comments() {
    let mut t = T::with_tasks(1);
    t.as_agent();
    let c = t.run(
        "comment.add",
        &[("task", "API-1"), ("body", "Avance: **50%**")],
    );
    assert!(c["author_name"].as_str().unwrap().starts_with("agente-"));
    assert_eq!(
        t.fail("comment.add", &[("task", "API-1"), ("body", "  ")])
            .key(),
        "rule-comment-body"
    );

    t.actor = Actor::Supervisor;
    t.run("comment.add", &[("task", "API-1"), ("body", "Bien, sigue")]);
    let list = t.run("comment.list", &[("task", "API-1")]);
    assert_eq!(list["count"], 2);
    assert_eq!(list["items"][1]["author_name"], "supervisor");
    assert_eq!(t.run("task.get", &[("task", "API-1")])["comments"], 2);
    assert_eq!(t.activity(&[("task", "API-1")])[0]["entity"], "comment");
}

#[test]
fn unknown_and_missing_arguments_are_rejected_before_running() {
    let mut t = T::with_tasks(1);
    let err = t.fail("task.get", &[("task", "API-1"), ("tarea", "API-1")]);
    assert_eq!(err.key(), "arg-unknown");
    assert!(err.localize(Lang::En).contains("valid ones: task"), "{err}");
    let err = t.fail("task.move", &[("task", "API-1")]);
    assert!(matches!(err, EngineError::MissingArg(ref n) if n == "status"));
}

#[path = "tests_coord.rs"]
mod coord;

#[path = "tests_m2.rs"]
mod m2;

#[path = "tests_exchange.rs"]
mod exchange_tests;

#[path = "tests_m5.rs"]
mod m5;

#[path = "tests_m6.rs"]
mod m6;

#[path = "tests_m7.rs"]
mod m7;

#[path = "tests_m8.rs"]
mod m8;

#[path = "tests_m9.rs"]
mod m9;

#[path = "tests_m11.rs"]
mod m11;

#[path = "tests_m12.rs"]
mod m12;

#[path = "tests_m13.rs"]
mod m13;

#[path = "tests_m15.rs"]
mod m15;

#[path = "tests_data.rs"]
mod data;
