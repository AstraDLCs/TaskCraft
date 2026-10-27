//! Formato: escritura y lectura, versiones y validación. La ida y vuelta
//! completa contra un workspace real está en los tests del engine.

use std::io::Cursor;

use serde_json::json;

use super::*;

fn dump() -> Dump {
    let ws = "01M5W0000000000000000000WS";
    let p = "01M5W00000000000000000000P";
    let t1 = "01M5W0000000000000000000T1";
    let t2 = "01M5W0000000000000000000T2";
    let at = "2026-10-26T10:00:00Z";
    let task = |id: &str, number: u64, parent: Option<&str>| {
        json!({
            "id": id, "project_id": p, "number": number, "parent_id": parent,
            "title": format!("Tarea {number}"), "description": "", "status": "todo",
            "priority": "medium", "start": null, "due": null, "labels": [],
            "assignee": null, "creator": "supervisor", "checklist": [], "criteria": [],
            "revision": 1, "created_at": at, "updated_at": at, "archived_at": null,
            "deleted_at": null, "claim": null, "needs_human": null, "locked": false
        })
    };
    serde_json::from_value(json!({
        "workspace": { "id": ws, "name": "w", "created_at": at, "settings": {
            "max_depth": 5, "trash_days": 30, "lease_minutes": 15,
            "agent_wip_limit": 1, "auto_archive_days": 30 } },
        "projects": [{ "id": p, "key": "API", "name": "Backend", "description": "",
            "status": "active", "close_policy": "review_required", "next_number": 3,
            "created_at": at, "updated_at": at, "locked": false }],
        "labels": [], "agents": [],
        // La hija primero: `parents_first` la reordena al recoger, no al leer.
        "tasks": [task(t1, 1, None), task(t2, 2, Some(t1))],
        "dependencies": [{ "blocker": t1, "blocked": t2 }],
        "comments": [], "activity": [], "inbox": []
    }))
    .unwrap()
}

fn manifest(version: u32) -> Manifest {
    Manifest {
        format: FORMAT.into(),
        version,
        exported_at: "2026-10-26T10:00:00Z".parse().unwrap(),
        app_version: "test".into(),
        workspace: "w".into(),
        projects: Vec::new(),
    }
}

#[test]
fn writes_and_reads_back_the_same_data() {
    let mut buf = Cursor::new(Vec::new());
    write(&mut buf, &manifest(VERSION), &dump()).unwrap();
    buf.set_position(0);
    let (m, d) = read(buf).unwrap();
    assert_eq!(m.version, VERSION);
    assert_eq!(d, dump());
    assert!(validate(&d).is_empty());
}

#[test]
fn newer_formats_and_foreign_files_are_rejected() {
    let mut buf = Cursor::new(Vec::new());
    write(&mut buf, &manifest(VERSION + 1), &dump()).unwrap();
    buf.set_position(0);
    assert!(matches!(
        read(buf),
        Err(ExchangeError::NewerFormat { found, .. }) if found == VERSION + 1
    ));
    let not_zip = Cursor::new(b"hola".to_vec());
    assert!(matches!(read(not_zip), Err(ExchangeError::NotTaskcraft(_))));
}

#[test]
fn validation_finds_dangling_references_and_duplicates() {
    let mut d = dump();
    d.tasks[1].parent_id = Some(model::TaskId::generate());
    d.dependencies[0].blocker = model::TaskId::generate();
    let dup = d.tasks[0].clone();
    d.tasks.push(dup);
    let codes: Vec<&str> = validate(&d).iter().map(|p| p.code).collect();
    assert!(codes.contains(&"dangling-parent"), "{codes:?}");
    assert!(codes.contains(&"dangling-dependency"), "{codes:?}");
    assert!(codes.contains(&"duplicate-id"), "{codes:?}");
}

#[test]
fn parents_go_before_children() {
    let mut d = dump();
    d.tasks.reverse();
    let ordered = parents_first(d.tasks);
    assert_eq!(ordered[0].number, 1);
    assert_eq!(ordered[1].number, 2);
}

#[test]
fn version_1_files_are_migrated_on_read() {
    let mut buf = Cursor::new(Vec::new());
    write(&mut buf, &manifest(1), &dump()).unwrap();
    buf.set_position(0);
    let (m, d) = read(buf).unwrap();
    assert_eq!(m.version, VERSION);
    assert!(d.epics.is_empty() && d.relations.is_empty());
    assert!(d.projects[0].workflow.is_empty());
}

#[test]
fn attachment_contents_travel_inside_the_zip() {
    let mut d = dump();
    d.attachments.push(model::Attachment {
        id: model::AttachmentId::generate(),
        task_id: d.tasks[0].id,
        name: "a.txt".into(),
        mime: "text/plain".into(),
        size: 4,
        sha256: "abc".into(),
        author: model::Actor::Supervisor,
        created_at: "2026-10-26T10:00:00Z".parse().unwrap(),
    });
    d.blobs.insert("abc".into(), b"hola".to_vec());
    let mut buf = Cursor::new(Vec::new());
    write(&mut buf, &manifest(VERSION), &d).unwrap();
    buf.set_position(0);
    let (_, back) = read(buf).unwrap();
    assert_eq!(back.blobs.get("abc").map(Vec::as_slice), Some(&b"hola"[..]));
    assert_eq!(back, d);
    let mut missing = d.clone();
    missing.blobs.clear();
    let codes: Vec<&str> = validate(&missing).iter().map(|p| p.code).collect();
    assert!(codes.contains(&"dangling-attachment"));
}

/// v2 → v5: el asignado único pasa a ser el único asignado (D15, D20).
#[test]
fn version_2_assignee_becomes_the_only_assignee() {
    let agent = model::AgentId::generate();
    let data = serde_json::json!({
        "agents": [],
        "tasks": [{ "assignee": format!("agent:{agent}") }, { "assignee": null }],
    });
    let out = crate::migrate(2, data).unwrap();
    assert_eq!(out["interfaces"], serde_json::json!([]));
    assert_eq!(
        out["tasks"][0]["assignees"],
        serde_json::json!([format!("agent:{agent}")])
    );
    assert_eq!(out["tasks"][1]["assignees"], serde_json::json!([]));
    assert!(out["tasks"][0].get("assignee").is_none());
    assert!(out["tasks"][0].get("reviews").is_none());
}

/// v4 → v5 (D20): sin roles, sin votos y sin política de cierre.
#[test]
fn version_4_roles_and_votes_are_dropped() {
    let (a, b) = (model::AgentId::generate(), model::AgentId::generate());
    let data = serde_json::json!({
        "projects": [{ "close_policy": "review_required" }],
        "tasks": [{
            "assignees": [
                { "actor": format!("agent:{a}"), "role": "executor" },
                { "actor": format!("agent:{b}"), "role": "reviewer" },
                { "actor": format!("agent:{a}"), "role": "reviewer" },
            ],
            "reviews": [{ "reviewer": format!("agent:{b}"), "verdict": "approve" }],
        }],
    });
    let out = crate::migrate(4, data).unwrap();
    assert!(out["projects"][0].get("close_policy").is_none());
    assert_eq!(
        out["tasks"][0]["assignees"],
        serde_json::json!([format!("agent:{a}"), format!("agent:{b}")])
    );
    assert!(out["tasks"][0].get("reviews").is_none());
}

/// v5 → v6 (RF-INT-05): el ciclo único pasa a ser una lista de ciclos.
#[test]
fn version_5_cycle_becomes_a_list() {
    let cycle = model::CycleId::generate();
    let data = serde_json::json!({
        "tasks": [{ "cycle_id": cycle.to_string() }, { "cycle_id": null }, {}],
    });
    let out = crate::migrate(5, data).unwrap();
    assert_eq!(
        out["tasks"][0]["cycles"],
        serde_json::json!([cycle.to_string()])
    );
    assert_eq!(out["tasks"][1]["cycles"], serde_json::json!([]));
    assert_eq!(out["tasks"][2]["cycles"], serde_json::json!([]));
    assert!(out["tasks"][0].get("cycle_id").is_none());
}

/// v6 → v7 (RF-MOD-19): sin diagramas y con el contador de cada proyecto en 1.
#[test]
fn version_6_gets_empty_diagrams() {
    let data = serde_json::json!({ "projects": [{ "key": "API" }] });
    let out = crate::migrate(6, data).unwrap();
    assert_eq!(out["diagrams"], serde_json::json!([]));
    assert_eq!(out["projects"][0]["next_diagram"], 1);
}
