use std::thread;

use model::{
    Actor, CheckItem, Dependency, ItemId, Label, LabelId, Priority, Project, ProjectId,
    ProjectStatus, Status, Task, TaskId, Timestamp,
};
use store::{Store, StoreError, TaskFilter};

use crate::SqliteStore;
use crate::testutil::tempdir;

fn project(key: &str) -> Project {
    let now = Timestamp::now();
    Project {
        id: ProjectId::generate(),
        key: key.into(),
        name: format!("Proyecto {key}"),
        description: String::new(),
        status: ProjectStatus::Active,
        next_number: 1,
        next_diagram: 1,
        created_at: now,
        updated_at: now,
        locked: false,
        workflow: Vec::new(),
        estimate_unit: model::EstimateUnit::Hours,
        fields: Vec::new(),
    }
}

fn task(project: &Project, number: u64) -> Task {
    let now = Timestamp::now();
    Task {
        id: TaskId::generate(),
        project_id: project.id,
        number,
        parent_id: None,
        title: format!("Tarea {number}"),
        description: "**md**".into(),
        status: Status::Todo,
        priority: Priority::High,
        start: None,
        due: Some("2026-11-01".parse().unwrap()),
        labels: vec![],
        assignees: vec![],
        creator: Actor::Supervisor,
        checklist: vec![],
        criteria: vec![],
        revision: 1,
        created_at: now,
        updated_at: now,
        archived_at: None,
        deleted_at: None,
        claim: None,
        needs_human: None,
        locked: false,
        stage: None,
        estimate: None,
        epic_id: None,
        milestone_id: None,
        fields: Default::default(),
        recurrence: None,
        cycles: Vec::new(),
    }
}

fn item(text: &str, done: bool) -> CheckItem {
    CheckItem {
        id: ItemId::generate(),
        text: text.into(),
        done,
    }
}

#[test]
fn task_aggregate_roundtrip() {
    let mut store = SqliteStore::open_in_memory().unwrap();
    let p = project("API");
    let label = Label {
        id: LabelId::generate(),
        name: "rust".into(),
        color: "#0090ff".into(),
    };
    let mut t = task(&p, 1);
    t.labels = vec![label.id];
    t.checklist = vec![item("paso 1", true), item("paso 2", false)];
    t.criteria = vec![item("compila", false)];

    let mut tx = store.begin().unwrap();
    tx.insert_project(&p).unwrap();
    tx.insert_label(&label).unwrap();
    tx.insert_task(&t).unwrap();
    tx.commit().unwrap();

    let r = store.read().unwrap();
    assert_eq!(r.task(t.id).unwrap().unwrap(), t);
    assert_eq!(r.task_by_number(p.id, 1).unwrap().unwrap(), t);
    assert_eq!(r.project_by_key("api").unwrap().unwrap(), p);
    assert_eq!(r.label_by_name("RUST").unwrap().unwrap(), label);
}

#[test]
fn dropping_a_transaction_rolls_back() {
    let mut store = SqliteStore::open_in_memory().unwrap();
    {
        let mut tx = store.begin().unwrap();
        tx.insert_project(&project("API")).unwrap();
        // Sin commit.
    }
    assert!(store.read().unwrap().projects(true).unwrap().is_empty());
}

#[test]
fn filters_and_dependencies() {
    let mut store = SqliteStore::open_in_memory().unwrap();
    let p = project("API");
    let a = task(&p, 1);
    let mut b = task(&p, 2);
    b.parent_id = Some(a.id);
    let mut c = task(&p, 3);
    c.deleted_at = Some(Timestamp::now());

    let mut tx = store.begin().unwrap();
    tx.insert_project(&p).unwrap();
    for t in [&a, &b, &c] {
        tx.insert_task(t).unwrap();
    }
    tx.add_dependency(Dependency {
        blocker: a.id,
        blocked: b.id,
    })
    .unwrap();
    tx.commit().unwrap();

    let r = store.read().unwrap();
    let all = r.tasks(&TaskFilter::project(p.id)).unwrap();
    assert_eq!(all.iter().map(|t| t.number).collect::<Vec<_>>(), [1, 2]);
    let roots = TaskFilter {
        parent: Some(None),
        ..TaskFilter::project(p.id)
    };
    assert_eq!(r.tasks(&roots).unwrap().len(), 1);
    let trash = TaskFilter {
        deleted: true,
        ..TaskFilter::default()
    };
    assert_eq!(r.tasks(&trash).unwrap()[0].id, c.id);
    assert_eq!(r.blockers_of(b.id).unwrap(), [a.id]);
    assert_eq!(r.blocks(a.id).unwrap(), [b.id]);
}

#[test]
fn purging_a_parent_removes_its_subtree() {
    let mut store = SqliteStore::open_in_memory().unwrap();
    let p = project("API");
    let parent = task(&p, 1);
    let mut child = task(&p, 2);
    child.parent_id = Some(parent.id);

    let mut tx = store.begin().unwrap();
    tx.insert_project(&p).unwrap();
    tx.insert_task(&parent).unwrap();
    tx.insert_task(&child).unwrap();
    tx.purge_task(parent.id).unwrap();
    tx.commit().unwrap();

    assert!(store.read().unwrap().task(child.id).unwrap().is_none());
}

#[test]
fn committed_data_survives_reopening() {
    let path = tempdir().join("workspace.db");
    let p = project("API");
    {
        let mut store = SqliteStore::open(&path).unwrap();
        let mut tx = store.begin().unwrap();
        tx.insert_project(&p).unwrap();
        tx.commit().unwrap();
        // Se descarta sin cierre ordenado.
    }
    let mut store = SqliteStore::open(&path).unwrap();
    let r = store.read().unwrap();
    assert_eq!(r.project(p.id).unwrap().unwrap(), p);
    assert_eq!(
        r.workspace().unwrap().name,
        path.parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    );
}

#[test]
fn change_token_sees_other_connections() {
    let path = tempdir().join("workspace.db");
    let a = SqliteStore::open(&path).unwrap();
    let mut b = SqliteStore::open(&path).unwrap();
    let before_a = a.change_token().unwrap();
    let before_b = b.change_token().unwrap();

    let mut tx = b.begin().unwrap();
    tx.insert_project(&project("API")).unwrap();
    tx.commit().unwrap();

    assert_ne!(a.change_token().unwrap(), before_a, "cambio ajeno");
    assert_ne!(b.change_token().unwrap(), before_b, "cambio propio");
}

/// Varias conexiones (como CLI + app + agentes) escribiendo a la vez sobre el
/// mismo archivo: ninguna escritura se pierde ni falla (RF-ALM-03).
#[test]
fn concurrent_connections_lose_nothing() {
    const WRITERS: u64 = 4;
    const EACH: u64 = 25;
    let path = tempdir().join("workspace.db");
    let p = project("API");
    {
        let mut store = SqliteStore::open(&path).unwrap();
        let mut tx = store.begin().unwrap();
        tx.insert_project(&p).unwrap();
        tx.commit().unwrap();
    }

    thread::scope(|s| {
        for w in 0..WRITERS {
            let (path, p) = (&path, &p);
            s.spawn(move || {
                let mut store = SqliteStore::open(path).unwrap();
                for i in 0..EACH {
                    let mut tx = store.begin().unwrap();
                    tx.insert_task(&task(p, w * EACH + i + 1)).unwrap();
                    tx.commit().unwrap();
                }
            });
        }
    });

    let mut store = SqliteStore::open(&path).unwrap();
    let all = store
        .read()
        .unwrap()
        .tasks(&TaskFilter::project(p.id))
        .unwrap();
    assert_eq!(all.len() as u64, WRITERS * EACH);
}

fn agent(name: &str) -> model::Agent {
    let now = Timestamp::now();
    model::Agent {
        id: model::AgentId::generate(),
        name: name.into(),
        model: "claude-opus-5-5".into(),
        description: String::new(),
        color: "#6e56cf".into(),
        status: model::AgentStatus::Active,
        created_at: now,
        updated_at: now,
        interface_id: None,
        level: model::AgentLevel::Plan,
        scope: Vec::new(),
        capabilities: Vec::new(),
        paused: false,
    }
}

fn entry(actor: Actor, task: Option<TaskId>) -> model::ActivityEntry {
    model::ActivityEntry {
        id: model::ActivityId::generate(),
        at: Timestamp::now(),
        actor,
        correlation: model::CorrelationId::generate(),
        command: "task.update".into(),
        action: model::Action::Updated,
        entity: model::EntityKind::Task,
        entity_id: task.map(|t| t.to_string()).unwrap_or_default(),
        project_id: None,
        task_id: task,
        changes: vec![model::FieldChange {
            field: "title".into(),
            before: "a".into(),
            after: "b".into(),
        }],
    }
}

#[test]
fn agents_sessions_and_comments_roundtrip() {
    let mut store = SqliteStore::open_in_memory().unwrap();
    let p = project("API");
    let t = task(&p, 1);
    let mut a = agent("claude-1");
    let now = Timestamp::now();
    let mut session = model::AgentSession {
        id: model::SessionId::generate(),
        agent_id: a.id,
        transport: "cli".into(),
        started_at: now,
        last_seen: now,
        ended_at: None,
    };
    let comment = model::Comment {
        id: model::CommentId::generate(),
        task_id: t.id,
        author: Actor::Agent(a.id),
        body: "Avance: **50%**".into(),
        created_at: now,
    };

    let mut tx = store.begin().unwrap();
    tx.insert_project(&p).unwrap();
    tx.insert_task(&t).unwrap();
    tx.insert_agent(&a).unwrap();
    tx.upsert_session(&session).unwrap();
    tx.insert_comment(&comment).unwrap();
    tx.commit().unwrap();

    a.status = model::AgentStatus::Disabled;
    session.ended_at = Some(Timestamp::now());
    let mut tx = store.begin().unwrap();
    tx.update_agent(&a).unwrap();
    tx.upsert_session(&session).unwrap();
    tx.commit().unwrap();

    let r = store.read().unwrap();
    assert_eq!(r.agent_by_name("CLAUDE-1").unwrap().unwrap(), a);
    assert_eq!(r.session(session.id).unwrap().unwrap(), session);
    assert!(r.open_sessions(Some(a.id)).unwrap().is_empty());
    assert_eq!(r.comments(t.id).unwrap(), [comment]);
    assert_eq!(r.comment_count(t.id).unwrap(), 1);
}

#[test]
fn activity_is_append_only_and_paginated() {
    let mut store = SqliteStore::open_in_memory().unwrap();
    let t1 = TaskId::generate();
    let entries: Vec<_> = (0..5)
        .map(|i| entry(Actor::Supervisor, if i % 2 == 0 { Some(t1) } else { None }))
        .collect();
    let mut tx = store.begin().unwrap();
    for e in &entries {
        tx.append_activity(e).unwrap();
    }
    tx.commit().unwrap();

    let r = store.read().unwrap();
    let all = r.activity(&store::ActivityFilter::default()).unwrap();
    assert_eq!(all.len(), 5);
    let of_task = r
        .activity(&store::ActivityFilter {
            task: Some(t1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(of_task.len(), 3);
    let page = r
        .activity(&store::ActivityFilter {
            before: Some(all[1].id),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.len(), 3, "solo las anteriores al cursor");
    assert_eq!(page[0].changes, entries_changes());
    drop(r);

    // Ni siquiera con SQL directo se puede editar o borrar el historial.
    assert!(
        store
            .conn
            .execute("UPDATE activity SET command = 'x'", [])
            .is_err()
    );
    assert!(store.conn.execute("DELETE FROM activity", []).is_err());
}

fn entries_changes() -> Vec<model::FieldChange> {
    entry(Actor::Supervisor, None).changes
}

#[test]
fn opening_a_v1_workspace_migrates_with_backup() {
    let dir = tempdir();
    let path = dir.join("workspace.db");
    {
        let mut conn = rusqlite::Connection::open(&path).unwrap();
        crate::migrate::migrate(&mut conn, &crate::migrate::MIGRATIONS[..1], None).unwrap();
    }
    let mut store = SqliteStore::open(&path).unwrap();
    assert!(
        store.read().unwrap().agents().unwrap().is_empty(),
        "tabla v2 creada"
    );
    let backups: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".v1-"))
        .collect();
    assert_eq!(backups.len(), 1, "copia de la versión 1");
}

/// v9 → v10 (D15): cada agente con token pasa a ser su propia interfaz (el
/// token sigue valiendo) y el asignado único pasa a ser el ejecutor.
#[test]
fn v10_turns_agent_tokens_into_interfaces_and_assignees_into_executors() {
    let dir = tempdir();
    let path = dir.join("workspace.db");
    let agent_id = model::AgentId::generate();
    let task_id = TaskId::generate();
    let project_id = model::ProjectId::generate().to_string();
    {
        let mut conn = rusqlite::Connection::open(&path).unwrap();
        crate::migrate::migrate(&mut conn, &crate::migrate::MIGRATIONS[..9], None).unwrap();
        let now = Timestamp::now().to_string();
        conn.execute(
            "INSERT INTO workspace (id, name, created_at, max_depth, trash_days) VALUES ('W', 'w', ?1, 5, 30)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent (id, name, model, description, color, status, created_at, updated_at, token_hash)
             VALUES (?1, 'claude-1', 'claude', '', '#000000', 'active', ?2, ?2, 'hash-1')",
            rusqlite::params![agent_id.to_string(), now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO project (id, key, name, description, status, close_policy, next_number, created_at, updated_at)
             VALUES (?2, 'API', 'Api', '', 'active', 'review_required', 2, ?1, ?1)",
            [&now, &project_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO task (id, project_id, number, title, description, status, priority, assignee,
                               creator, revision, created_at, updated_at)
             VALUES (?1, ?4, 1, 't', '', 'todo', 'medium', ?2, 'supervisor', 1, ?3, ?3)",
            rusqlite::params![task_id.to_string(), format!("agent:{agent_id}"), now, project_id],
        )
        .unwrap();
    }
    let mut store = SqliteStore::open(&path).unwrap();
    let r = store.read().unwrap();
    let interface = r
        .interface_by_token("hash-1")
        .unwrap()
        .expect("interfaz con el token");
    assert_eq!(interface.name, "claude-1");
    let agent = r.agent(agent_id).unwrap().unwrap();
    assert_eq!(agent.interface_id, Some(interface.id));
    let task = r.task(task_id).unwrap().unwrap();
    assert_eq!(task.assignee(), Some(&Actor::Agent(agent_id)));
}

/// v15 → v16 (D20): cada `{actor, role}` pasa a ser el actor, sin repetir
/// y en su orden; desaparecen los votos y la política de cierre.
#[test]
fn v16_drops_roles_votes_and_close_policy() {
    let dir = tempdir();
    let path = dir.join("workspace.db");
    let (a, b) = (model::AgentId::generate(), model::AgentId::generate());
    let task_id = TaskId::generate();
    let project_id = model::ProjectId::generate().to_string();
    {
        let mut conn = rusqlite::Connection::open(&path).unwrap();
        crate::migrate::migrate(&mut conn, &crate::migrate::MIGRATIONS[..15], None).unwrap();
        let now = Timestamp::now().to_string();
        conn.execute(
            "INSERT INTO project (id, key, name, description, status, close_policy, next_number, created_at, updated_at)
             VALUES (?2, 'API', 'Api', '', 'active', 'review_required', 2, ?1, ?1)",
            [&now, &project_id],
        )
        .unwrap();
        let assignees = format!(
            r#"[{{"actor":"agent:{b}","role":"executor"}},{{"actor":"agent:{a}","role":"reviewer"}},{{"actor":"agent:{b}","role":"reviewer"}}]"#
        );
        conn.execute(
            "INSERT INTO task (id, project_id, number, title, description, status, priority,
                               creator, revision, created_at, updated_at, assignees, reviews)
             VALUES (?1, ?2, 1, 't', '', 'in_review', 'medium', 'supervisor', 1, ?3, ?3, ?4,
                     '[{\"reviewer\":\"supervisor\",\"verdict\":\"approve\"}]')",
            rusqlite::params![task_id.to_string(), project_id, now, assignees],
        )
        .unwrap();
    }
    let mut store = SqliteStore::open(&path).unwrap();
    let r = store.read().unwrap();
    let task = r.task(task_id).unwrap().unwrap();
    assert_eq!(task.assignees, vec![Actor::Agent(b), Actor::Agent(a)]);
    assert_eq!(r.projects(false).unwrap().len(), 1);
}

#[test]
fn read_only_replica_tells_older_from_newer_schema() {
    let dir = tempdir();
    let path = dir.join("workspace.db");
    drop(SqliteStore::open(&path).unwrap());
    let supported = u32::try_from(crate::migrate::MIGRATIONS.len()).unwrap();
    let set = |v: u32| {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", v).unwrap();
    };

    set(supported - 1);
    assert!(matches!(
        SqliteStore::open_read_only(&path),
        Err(StoreError::OlderSchema { found, .. }) if found == supported - 1
    ));
    set(supported + 1);
    assert!(matches!(
        SqliteStore::open_read_only(&path),
        Err(StoreError::NewerSchema { .. })
    ));
}

#[test]
fn read_only_replica_sees_commits_and_cannot_write() {
    let dir = tempdir();
    let path = dir.join("workspace.db");
    let mut writer = SqliteStore::open(&path).unwrap();
    let mut replica = SqliteStore::open_read_only(&path).unwrap();
    let before = replica.change_token().unwrap();

    let mut tx = writer.begin().unwrap();
    let mut ws = tx.workspace().unwrap();
    ws.name = "renombrado".into();
    tx.update_workspace(&ws).unwrap();
    tx.commit().unwrap();

    assert_ne!(
        replica.change_token().unwrap(),
        before,
        "ve el commit ajeno"
    );
    assert_eq!(
        replica.read().unwrap().workspace().unwrap().name,
        "renombrado"
    );
    let mut tx = replica
        .begin()
        .unwrap_or_else(|_| panic!("BEGIN IMMEDIATE debería abrirse"));
    let ws = tx.workspace().unwrap();
    assert!(tx.update_workspace(&ws).is_err(), "solo lectura");
}
