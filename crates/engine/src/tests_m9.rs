//! M9: automatizaciones (RF-INT-03) y cola de webhooks (RF-INT-02).

use super::T;

#[test]
fn closing_all_children_sends_the_parent_to_review() {
    let mut t = T::with_tasks(1);
    for title in ["a", "b"] {
        t.run("task.create", &[("parent", "API-1"), ("title", title)]);
    }
    t.run("automation.create", &[("preset", "parent_review")]);
    t.run("task.move", &[("task", "API-2"), ("status", "in_progress")]);
    t.run("task.move", &[("task", "API-2"), ("status", "done")]);
    assert_eq!(t.run("task.get", &[("task", "API-1")])["status"], "todo");
    t.run("task.move", &[("task", "API-3"), ("status", "cancelled")]);
    assert_eq!(
        t.run("task.get", &[("task", "API-1")])["status"],
        "in_review"
    );
    let history = t.run("activity.list", &[("task", "API-1")]).to_string();
    assert!(history.contains("task.review_parent"), "{history}");
    assert!(history.contains("system"), "{history}");
}

#[test]
fn automations_run_when_the_condition_matches() {
    let mut t = T::with_tasks(0);
    t.run(
        "automation.create",
        &[
            ("name", "bugs urgentes"),
            ("trigger", "created"),
            ("condition", "label:bug"),
            ("command", "task.update"),
            ("args", r#"{"task": "{task}", "priority": "urgent"}"#),
        ],
    );
    t.run(
        "task.create",
        &[("project", "API"), ("title", "falla"), ("labels", "bug")],
    );
    t.run("task.create", &[("project", "API"), ("title", "idea")]);
    assert_eq!(
        t.run("task.get", &[("task", "API-1")])["priority"],
        "urgent"
    );
    assert_eq!(
        t.run("task.get", &[("task", "API-2")])["priority"],
        "medium"
    );

    t.run(
        "automation.update",
        &[("automation", "BUGS URGENTES"), ("enabled", "false")],
    );
    t.run(
        "task.create",
        &[("project", "API"), ("title", "otra"), ("labels", "bug")],
    );
    assert_eq!(
        t.run("task.get", &[("task", "API-3")])["priority"],
        "medium"
    );

    // Un comando que no existe o un filtro roto se rechazan al crearla.
    assert_eq!(
        t.fail(
            "automation.create",
            &[("name", "x"), ("trigger", "created"), ("command", "nada")]
        )
        .key(),
        "automation-command"
    );
    assert_eq!(
        t.fail(
            "automation.create",
            &[
                ("name", "x"),
                ("trigger", "created"),
                ("command", "task.get"),
                ("condition", "(")
            ]
        )
        .key(),
        "rule-filter"
    );
    // Solo el supervisor las configura: actúan como System.
    t.as_agent();
    assert_eq!(
        t.fail("automation.create", &[("preset", "parent_review")])
            .code(),
        "FORBIDDEN"
    );
}

#[test]
fn automations_do_not_loop_forever() {
    let mut t = T::with_tasks(1);
    for (name, from, to) in [
        ("ida", "in_progress", "todo"),
        ("vuelta", "todo", "in_progress"),
    ] {
        t.run(
            "automation.create",
            &[
                ("name", name),
                ("trigger", "status_changed"),
                ("condition", &format!("status:{from}")),
                ("command", "task.move"),
                (
                    "args",
                    &format!(r#"{{"task": "{{task}}", "status": "{to}"}}"#),
                ),
            ],
        );
    }
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    let moves = t.run("activity.list", &[("task", "API-1"), ("action", "updated")]);
    // El cambio original y tres encadenados como mucho.
    assert_eq!(moves["items"].as_array().unwrap().len(), 4, "{moves}");
}

#[test]
fn webhooks_queue_signed_deliveries_with_backoff() {
    let mut t = T::with_tasks(1);
    let hook = t.run(
        "webhook.create",
        &[
            ("url", "https://example.test/hook"),
            ("triggers", "completed"),
        ],
    );
    let secret = hook["secret"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("whsec_"));
    assert!(!t.run("webhook.list", &[]).to_string().contains(&secret));
    assert!(!t.run("activity.list", &[]).to_string().contains(&secret));

    t.run("task.update", &[("task", "API-1"), ("title", "cambia")]);
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    assert_eq!(
        t.run("webhook.deliveries", &[])["count"],
        0,
        "no le interesa"
    );
    t.run("task.move", &[("task", "API-1"), ("status", "done")]);
    let queued = t.run("webhook.deliveries", &[]);
    assert_eq!(queued["count"], 1);
    let id = queued["items"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(queued["items"][0]["event"], "completed");

    let base = model::Timestamp::now();
    t.clock = Some(base);
    let failed = t.run(
        "webhook.record",
        &[("delivery", &id), ("ok", "false"), ("error", "503")],
    );
    assert_eq!(failed["attempts"], 1);
    let next: model::Timestamp = failed["next_at"].as_str().unwrap().parse().unwrap();
    assert_eq!(next.duration_since(base).as_secs(), 30);
    let ok = t.run("webhook.record", &[("delivery", &id), ("ok", "true")]);
    assert_eq!(ok["status"], "delivered");
    t.clock = None;

    // La cola es del daemon: lo que toca enviar, con la carga ya formada.
    let due = t
        .engine
        .read(|r| Ok(r.deliveries_due(model::Timestamp::now(), 10)?))
        .unwrap();
    assert!(due.is_empty());
}
