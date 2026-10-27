//! M13 (Extras): planificación — tareas recurrentes (RF-MOD-18, D17).

use model::Actor;

use super::T;

/// Martes 6 de enero de 2026, a mediodía en cualquier zona razonable.
fn tuesday() -> model::Timestamp {
    "2026-01-06T15:00:00Z".parse().unwrap()
}

/// No hay tarea con esa clave.
fn missing(t: &mut T, key: &str) -> bool {
    t.fail("task.get", &[("task", key)]).code() == "NOT_FOUND"
}

fn close(t: &mut T, key: &str) {
    for s in ["in_progress", "done"] {
        t.run("task.move", &[("task", key), ("status", s)]);
    }
}

#[test]
fn finishing_a_recurring_task_creates_the_next_one() {
    let mut t = T::new();
    t.clock = Some(tuesday());
    t.run("project.create", &[("key", "ops")]);
    t.run(
        "task.create",
        &[
            ("project", "OPS"),
            ("title", "Backup semanal"),
            ("due", "2026-01-05"),
            ("start", "2026-01-04"),
            ("labels", "infra"),
        ],
    );
    t.run("checklist.add", &[("task", "OPS-1"), ("text", "copiar")]);
    t.run(
        "task.check",
        &[("task", "OPS-1"), ("kind", "checklist"), ("item", "1")],
    );
    let set = t.run(
        "task.set_recurrence",
        &[("task", "OPS-1"), ("every", "week"), ("on", "mon")],
    );
    assert_eq!(set["recurrence"]["mode"], "every");
    assert_eq!(set["recurrence"]["weekdays"], serde_json::json!([1]));

    close(&mut t, "OPS-1");
    let next = t.run("task.get", &[("task", "OPS-2")]);
    assert_eq!(next["title"], "Backup semanal");
    assert_eq!(next["status"], "todo");
    // Lunes siguiente al día en que se terminó; el inicio se desplaza igual.
    assert_eq!(next["due"], "2026-01-12");
    assert_eq!(next["start"], "2026-01-11");
    assert_eq!(next["labels"], serde_json::json!(["infra"]));
    assert_eq!(next["checklist"][0]["done"], false);
    assert_eq!(next["recurrence"]["unit"], "week");
    // La regla se mudó: reabrir y cerrar la vieja no duplica la serie.
    let old = t.run("task.get", &[("task", "OPS-1")]);
    assert!(old["recurrence"].is_null());
    t.run("task.move", &[("task", "OPS-1"), ("status", "todo")]);
    close(&mut t, "OPS-1");
    assert!(missing(&mut t, "OPS-3"));

    // Queda en el historial como creada por el mismo comando.
    let history = t.run("activity.list", &[("task", "OPS-2")]);
    assert!(history.to_string().contains("task.move"), "{history}");

    // Cancelar no crea la siguiente; quitar la regla tampoco deja repetir.
    t.run("task.move", &[("task", "OPS-2"), ("status", "cancelled")]);
    assert!(missing(&mut t, "OPS-3"));
}

#[test]
fn relative_rules_count_from_the_day_it_was_done_and_agents_also_trigger() {
    let mut t = T::with_tasks(1);
    t.clock = Some(tuesday());
    t.run("task.set_recurrence", &[("task", "API-1"), ("after", "3d")]);

    // También cuando la cierra un agente desde «en revisión» (D20).
    t.register(&[("name", "dev")]);
    let rev = t.register(&[("name", "rev")]);
    for a in ["dev", "rev"] {
        t.run("task.add_assignee", &[("task", "API-1"), ("agent", a)]);
    }
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    t.run("task.move", &[("task", "API-1"), ("status", "in_review")]);
    t.actor = Actor::Agent(rev["id"].as_str().unwrap().parse().unwrap());
    let done = t.run("task.move", &[("task", "API-1"), ("status", "done")]);
    assert_eq!(done["status"], "done", "{done}");
    t.actor = Actor::Supervisor;
    let next = t.run("task.get", &[("task", "API-2")]);
    assert_eq!(next["due"], "2026-01-09");
    assert_eq!(next["assignees"].as_array().unwrap().len(), 2);

    t.run("task.clear_recurrence", &[("task", "API-2")]);
    close(&mut t, "API-2");
    assert!(missing(&mut t, "API-3"));
}

#[test]
fn bad_rules_are_rejected() {
    let mut t = T::with_tasks(1);
    for args in [
        &[("every", "fortnight")][..],
        &[("every", "month"), ("on", "mon")],
        &[("every", "week"), ("after", "3d")],
        &[],
    ] {
        let mut all = vec![("task", "API-1")];
        all.extend_from_slice(args);
        assert_eq!(
            t.fail("task.set_recurrence", &all).key(),
            "rule-recurrence",
            "{args:?}"
        );
    }
}

#[test]
fn untouched_pending_tasks_go_stale_and_can_be_parked_someday() {
    let mut t = T::new();
    t.clock = Some(tuesday());
    t.run("project.create", &[("key", "api")]);
    for title in ["vieja", "en curso", "reciente"] {
        t.run("task.create", &[("project", "API"), ("title", title)]);
    }
    t.run("task.move", &[("task", "API-2"), ("status", "in_progress")]);
    // Cinco semanas después solo se toca API-3.
    t.clock = Some("2026-02-10T15:00:00Z".parse().unwrap());
    t.run("task.update", &[("task", "API-3"), ("title", "reciente!")]);

    let stale = t.run("task.stale", &[("project", "API")]);
    assert_eq!(stale["weeks"], 4);
    let keys: Vec<&str> = stale["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["API-1"], "la en curso no cuenta");
    assert_eq!(stale["items"][0]["idle_weeks"], 5);
    assert_eq!(t.keys(&[("filter", "stale:true")]), ["API-1"]);
    // Con un umbral mayor ya no lo está.
    t.run("workspace.configure", &[("stale_weeks", "6")]);
    assert!(t.keys(&[("filter", "stale:true")]).is_empty());
    t.run("workspace.configure", &[("stale_weeks", "4")]);

    let parked = t.run("task.someday", &[("task", "API-1")]);
    assert_eq!(parked["status"], "backlog");
    assert_eq!(parked["stage"]["key"], "someday");
    let flow = t.run("status.list", &[("project", "API")]);
    assert!(flow.to_string().contains("someday"), "{flow}");
    // En «Algún día» ya no cuenta como envejecida, aunque pasen semanas.
    t.clock = Some("2026-06-01T15:00:00Z".parse().unwrap());
    assert!(
        !t.keys(&[("filter", "stale:true")])
            .contains(&"API-1".to_owned())
    );
    // Una tarea en curso no se puede aparcar.
    assert_eq!(
        t.fail("task.someday", &[("task", "API-2")]).key(),
        "rule-transition"
    );
}

#[test]
fn the_dependency_graph_has_layers_and_a_critical_path() {
    let mut t = T::with_tasks(5);
    for (key, estimate) in [
        ("API-1", "2"),
        ("API-2", "1"),
        ("API-3", "5"),
        ("API-4", "1"),
    ] {
        t.run("task.update", &[("task", key), ("estimate", estimate)]);
    }
    for (blocker, blocked) in [
        ("API-1", "API-2"),
        ("API-1", "API-3"),
        ("API-2", "API-4"),
        ("API-3", "API-4"),
    ] {
        t.run("task.link", &[("blocker", blocker), ("blocked", blocked)]);
    }
    let g = t.run("project.graph", &[("project", "API")]);
    let layer = |key: &str| {
        g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["key"] == key)
            .map(|n| {
                (
                    n["layer"].as_u64().unwrap(),
                    n["critical"].as_bool().unwrap(),
                )
            })
            .unwrap()
    };
    assert_eq!(layer("API-1"), (0, true));
    assert_eq!(layer("API-2"), (1, false));
    assert_eq!(layer("API-3"), (1, true));
    assert_eq!(layer("API-4"), (2, true));
    assert_eq!(layer("API-5"), (0, false));
    assert_eq!(g["edges"].as_array().unwrap().len(), 4);

    let cp = t.run("project.critical_path", &[("project", "API")]);
    let keys: Vec<&str> = cp["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["API-1", "API-3", "API-4"]);
    assert_eq!(cp["total"], 8.0);
    assert_eq!(cp["tasks"][0]["basis"], "estimate");

    // Terminada API-3, la ruta pasa por API-2.
    for s in ["in_progress", "done"] {
        t.run("task.move", &[("task", "API-3"), ("status", s)]);
    }
    let cp = t.run("project.critical_path", &[("project", "API")]);
    assert_eq!(cp["tasks"][1]["key"], "API-2");

    let dot = t.run(
        "project.graph",
        &[("project", "API"), ("format", "dot"), ("closed", "true")],
    );
    let text = dot["dot"].as_str().unwrap();
    assert!(text.starts_with("digraph \"API\""), "{text}");
    assert!(text.contains("\"API-1\" -> \"API-3\""));
}

#[test]
fn the_matrix_sorts_open_tasks_by_importance_and_urgency() {
    let mut t = T::with_tasks(4);
    t.clock = Some(tuesday());
    t.run(
        "task.update",
        &[
            ("task", "API-1"),
            ("priority", "urgent"),
            ("due", "2026-01-07"),
        ],
    );
    t.run(
        "task.update",
        &[
            ("task", "API-2"),
            ("priority", "high"),
            ("due", "2026-03-01"),
        ],
    );
    t.run(
        "task.update",
        &[
            ("task", "API-3"),
            ("priority", "low"),
            ("due", "2026-01-05"),
        ],
    );
    let m = t.run("task.matrix", &[("project", "API")]);
    let keys = |i: usize| -> Vec<String> {
        m["quadrants"][i]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["key"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(m["quadrants"][0]["quadrant"], "do");
    assert_eq!(keys(0), ["API-1"]);
    assert_eq!(keys(1), ["API-2"]);
    assert_eq!(keys(2), ["API-3"], "vencida y poco importante");
    assert_eq!(keys(3), ["API-4"]);
    // Con un umbral mayor, API-2 también es urgente.
    t.run("workspace.configure", &[("urgent_days", "60")]);
    let m = t.run("task.matrix", &[("project", "API")]);
    assert_eq!(m["quadrants"][0]["items"].as_array().unwrap().len(), 2);
}

#[test]
fn cycles_plan_tasks_and_carry_over_what_is_left() {
    let mut t = T::with_tasks(4);
    t.clock = Some(tuesday());
    let s1 = t.run("cycle.create", &[("project", "API")]);
    assert_eq!(s1["name"], "Sprint 1");
    assert_eq!(s1["start"], "2026-01-06");
    assert_eq!(s1["end"], "2026-01-19");
    assert_eq!(s1["state"], "active");
    let s2 = t.run("cycle.create", &[("project", "API")]);
    assert_eq!(s2["name"], "Sprint 2");
    assert_eq!(s2["start"], "2026-01-20");
    assert_eq!(s2["state"], "planned");
    assert_eq!(
        t.fail(
            "cycle.create",
            &[
                ("project", "API"),
                ("start", "2026-01-10"),
                ("name", "otro")
            ]
        )
        .key(),
        "cycle-overlap"
    );

    for key in ["API-1", "API-2", "API-3"] {
        t.run("task.set_cycle", &[("task", key), ("cycle", "current")]);
    }
    t.run("task.set_cycle", &[("task", "API-4"), ("cycle", "next")]);
    assert_eq!(
        t.run("task.get", &[("task", "API-1")])["cycles"][0]["name"],
        "Sprint 1"
    );
    assert_eq!(
        t.keys(&[("filter", "cycle:current")]),
        ["API-1", "API-2", "API-3"]
    );
    assert_eq!(t.keys(&[("filter", "cycle:\"Sprint 2\"")]), ["API-4"]);
    // La barra lateral: tareas de un ciclo y abiertas por proyecto, en una consulta.
    let sprint1: model::CycleId = s1["id"].as_str().unwrap().parse().unwrap();
    let (in_cycle, open) = t
        .engine
        .read(|r| {
            let tasks = r.tasks(&store::TaskFilter {
                cycle: Some(sprint1),
                ..store::TaskFilter::default()
            })?;
            Ok((tasks.len(), r.open_counts(&[model::Status::Done])?))
        })
        .unwrap();
    assert_eq!(in_cycle, 3);
    assert_eq!(open.values().copied().collect::<Vec<_>>(), [4]);

    // Se termina API-1 y se cierra: API-2 y API-3 pasan al Sprint 2.
    for s in ["in_progress", "done"] {
        t.run("task.move", &[("task", "API-1"), ("status", s)]);
    }
    let closed = t.run("cycle.close", &[("cycle", "Sprint 1"), ("project", "API")]);
    assert_eq!(closed["carried"], serde_json::json!(["API-2", "API-3"]));
    assert_eq!(closed["next"]["name"], "Sprint 2");
    assert_eq!(closed["closed"]["progress"]["done"], 1);
    let s2 = t.run("cycle.get", &[("cycle", "Sprint 2"), ("project", "API")]);
    assert_eq!(s2["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(
        t.fail(
            "task.set_cycle",
            &[("task", "API-1"), ("cycle", "Sprint 1")]
        )
        .key(),
        "cycle-closed"
    );

    // Pasado el Sprint 2, el daemon lo cierra y crea el Sprint 3 igual de largo.
    t.clock = Some("2026-02-03T15:00:00Z".parse().unwrap());
    t.actor = Actor::System;
    let rolled = t.run("cycle.rollover", &[]);
    t.actor = Actor::Supervisor;
    assert_eq!(rolled["closed"][0]["next"]["name"], "Sprint 3");
    let s3 = t.run("cycle.get", &[("cycle", "current"), ("project", "API")]);
    assert_eq!(s3["name"], "Sprint 3");
    assert_eq!(s3["days"], 14);
    assert_eq!(s3["progress"]["tasks"], 3);
    let list = t.run("cycle.list", &[("project", "API"), ("closed", "true")]);
    assert_eq!(list["items"].as_array().unwrap().len(), 3);
    // Borrar un ciclo lo quita de sus tareas, que conservan los demás.
    t.run("cycle.delete", &[("cycle", "Sprint 3"), ("project", "API")]);
    assert_eq!(cycle_names(&mut t, "API-4"), ["Sprint 2"]);
}

fn cycle_names(t: &mut T, key: &str) -> Vec<String> {
    t.run("task.get", &[("task", key)])["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_task_can_be_planned_in_several_cycles() {
    let mut t = T::with_tasks(3);
    t.clock = Some(tuesday());
    for _ in 0..3 {
        t.run("cycle.create", &[("project", "API")]);
    }
    t.run("task.add_cycle", &[("task", "API-1"), ("cycle", "current")]);
    t.run(
        "task.add_cycle",
        &[("task", "API-1"), ("cycle", "Sprint 3")],
    );
    t.run(
        "task.add_cycle",
        &[("task", "API-1"), ("cycle", "Sprint 3")],
    );
    assert_eq!(cycle_names(&mut t, "API-1"), ["Sprint 1", "Sprint 3"]);
    t.run("task.add_cycle", &[("task", "API-2"), ("cycle", "current")]);
    assert_eq!(t.keys(&[("filter", "cycle:\"Sprint 3\"")]), ["API-1"]);
    assert_eq!(t.keys(&[("filter", "cycle:current")]), ["API-1", "API-2"]);
    assert_eq!(t.keys(&[("filter", "cycle:none")]), ["API-3"]);

    // Al cerrar el Sprint 1, API-1 ya está en el 3: solo API-2 pasa al 2, y
    // conserva el cerrado.
    let closed = t.run("cycle.close", &[("cycle", "Sprint 1"), ("project", "API")]);
    assert_eq!(closed["carried"], serde_json::json!(["API-2"]));
    let api2 = t.run("task.get", &[("task", "API-2")]);
    assert_eq!(api2["cycles"][0]["closed"], true);
    assert_eq!(cycle_names(&mut t, "API-2"), ["Sprint 1", "Sprint 2"]);
    assert_eq!(
        t.fail(
            "task.add_cycle",
            &[("task", "API-3"), ("cycle", "Sprint 1")]
        )
        .key(),
        "cycle-closed"
    );
    // Se puede sacar de uno cerrado; set_cycle la deja en uno solo.
    t.run(
        "task.remove_cycle",
        &[("task", "API-2"), ("cycle", "Sprint 1")],
    );
    assert_eq!(cycle_names(&mut t, "API-2"), ["Sprint 2"]);
    t.run(
        "task.set_cycle",
        &[("task", "API-1"), ("cycle", "Sprint 2")],
    );
    assert_eq!(cycle_names(&mut t, "API-1"), ["Sprint 2"]);
    t.run("cycle.delete", &[("cycle", "Sprint 2"), ("project", "API")]);
    assert!(cycle_names(&mut t, "API-1").is_empty());
    assert!(cycle_names(&mut t, "API-2").is_empty());
}
