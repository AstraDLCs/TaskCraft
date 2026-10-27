//! TC-5: métricas de flujo sobre el recorrido (D24). Un relevo guionizado
//! dev → revisor → dev → revisor → hecho, con minutos exactos.

use model::{Actor, Timestamp};
use serde_json::Value;

use super::T;

fn at(minute: i64) -> Timestamp {
    "2020-03-02T09:00:00Z"
        .parse::<Timestamp>()
        .unwrap()
        .checked_add(jiff::SignedDuration::from_mins(minute))
        .unwrap()
}

/// API con las colas de dev y rev y API-1 creada en la de dev; devuelve
/// (dev, rev).
fn setup(t: &mut T) -> (Actor, Actor) {
    t.clock = Some(at(0));
    t.run("project.create", &[("key", "api")]);
    let dev = t.register(&[("name", "dev")]);
    let rev = t.register(&[("name", "rev")]);
    for (name, owner) in [("Cola de dev", "dev"), ("Cola de rev", "rev")] {
        t.run(
            "status.create",
            &[
                ("project", "API"),
                ("name", name),
                ("base", "todo"),
                ("owner", owner),
            ],
        );
    }
    t.run(
        "task.create",
        &[
            ("project", "API"),
            ("title", "Tarea"),
            ("status", "cola_de_dev"),
        ],
    );
    let id = |v: &Value| Actor::Agent(v["id"].as_str().unwrap().parse().unwrap());
    (id(&dev), id(&rev))
}

/// El guion: toma dev (10), pasa a rev (40), rev la devuelve (55), dev la
/// vuelve a tomar (60), otra vez a rev (80) y rev la toma (85) y la cierra (90).
fn script(t: &mut T, dev: &Actor, rev: &Actor) {
    let step = |t: &mut T, who: &Actor, minute, cmd: &str, status: Option<&str>| {
        t.actor = who.clone();
        t.clock = Some(at(minute));
        let mut args = vec![("task", "API-1")];
        if let Some(s) = status {
            args.push(("status", s));
        }
        t.run(cmd, &args);
    };
    step(t, dev, 10, "task.claim", None);
    step(t, dev, 40, "task.move", Some("cola_de_rev"));
    step(t, rev, 55, "task.move", Some("cola_de_dev"));
    step(t, dev, 60, "task.claim", None);
    step(t, dev, 80, "task.move", Some("cola_de_rev"));
    step(t, rev, 85, "task.claim", None);
    step(t, rev, 90, "task.move", Some("done"));
    t.actor = Actor::Supervisor;
    t.clock = Some(at(100));
}

fn row<'a>(list: &'a Value, key: &str, value: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|r| r[key] == value)
        .unwrap_or_else(|| panic!("sin {key}={value} en {list}"))
}

#[test]
fn hand_over_scenario_counts_handoffs_returns_and_times() {
    let mut t = T::new();
    let (dev, rev) = setup(&mut t);
    script(&mut t, &dev, &rev);

    let f = t.run("metrics.flow", &[("project", "API")]);
    let s = &f["summary"];
    assert_eq!(s["done"], 1, "{f}");
    assert_eq!(s["handoffs"], 4, "{f}");
    assert_eq!(s["returns"], 1, "{f}");
    assert_eq!(s["returned_pct"], 100.0);
    assert_eq!(s["lead_p50_minutes"], 90);
    assert_eq!(s["lead_p85_minutes"], 90);
    assert_eq!(s["cycle_p50_minutes"], 80);
    let throughput: i64 = f["throughput"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["done"].as_i64().unwrap())
        .sum();
    assert_eq!(throughput, 1);
    assert_eq!(f["throughput"].as_array().unwrap().len(), 30);

    let agents = &f["agents"];
    let d = row(agents, "actor_name", "dev");
    assert_eq!(d["actor"], dev.to_string());
    assert_eq!(d["worked_minutes"], 30 + 20);
    assert_eq!(d["queued_minutes"], 10 + 5);
    assert_eq!(
        (&d["returns_received"], &d["returns_sent"]),
        (&1.into(), &0.into())
    );
    let r = row(agents, "actor_name", "rev");
    assert_eq!(r["done"], 1);
    assert_eq!(r["queued_minutes"], 15 + 5);
    assert_eq!(r["worked_minutes"], 5);
    assert_eq!(
        (&r["returns_received"], &r["returns_sent"]),
        (&0.into(), &1.into())
    );

    let columns = &f["columns"];
    let dev_q = row(columns, "key", "cola_de_dev");
    assert_eq!(dev_q["name"], "Cola de dev");
    assert_eq!(dev_q["owner"], "dev");
    assert_eq!(dev_q["base"], "todo");
    assert_eq!(dev_q["avg_minutes"], (10 + 5) / 2);
    assert_eq!(
        row(columns, "key", "cola_de_rev")["avg_minutes"],
        (15 + 5) / 2
    );
    assert_eq!(
        row(columns, "key", "in_progress")["avg_minutes"],
        (30 + 20 + 5) / 3
    );
    assert_eq!(row(columns, "key", "done")["tasks_now"], 1);
    assert_eq!(row(columns, "key", "done")["avg_minutes"], Value::Null);

    // Sin proyecto: los del alcance, aquí el único.
    assert_eq!(t.run("metrics.flow", &[])["summary"], f["summary"]);
}

#[test]
fn timeline_has_every_step_and_the_done_date() {
    let mut t = T::new();
    let (dev, rev) = setup(&mut t);
    script(&mut t, &dev, &rev);

    let tl = t.run("task.timeline", &[("task", "API-1")]);
    assert_eq!(tl["task"], "API-1");
    assert_eq!(tl["created_at"], serde_json::json!(at(0)));
    assert_eq!(tl["done_at"], serde_json::json!(at(90)));
    assert_eq!(tl["closed_at"], serde_json::json!(at(90)));
    assert_eq!(tl["total_minutes"], 90);
    let steps = tl["steps"].as_array().unwrap();
    let path: Vec<(&str, i64, &str)> = steps
        .iter()
        .map(|s| {
            (
                s["column"].as_str().unwrap(),
                s["minutes"].as_i64().unwrap(),
                s["actor_name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        path,
        vec![
            ("Cola de dev", 10, "supervisor"),
            ("in_progress", 30, "dev"),
            ("Cola de rev", 15, "dev"),
            ("Cola de dev", 5, "rev"),
            ("in_progress", 20, "dev"),
            ("Cola de rev", 5, "dev"),
            ("in_progress", 5, "rev"),
            // Cerrada: el reloj se para al cerrarla.
            ("done", 0, "rev"),
        ]
    );
    let first = &steps[0];
    assert_eq!(first["status"], "todo");
    assert_eq!(first["stage"], "cola_de_dev");
    assert_eq!(first["owner"], "dev");
    assert_eq!(first["entered_at"], serde_json::json!(at(0)));
    assert_eq!(first["left_at"], serde_json::json!(at(10)));
    assert_eq!(steps[1]["owner"], Value::Null);
    assert_eq!(steps[7]["left_at"], Value::Null);
    assert_eq!(steps[7]["actor"], rev.to_string());

    // Sin terminar: sin fecha de hecha y el total llega hasta ahora.
    t.run("task.create", &[("project", "API"), ("title", "Otra")]);
    t.clock = Some(at(130));
    let open = t.run("task.timeline", &[("task", "API-2")]);
    assert_eq!(open["done_at"], Value::Null);
    assert_eq!(open["closed_at"], Value::Null);
    assert_eq!(open["total_minutes"], 30);

    // Cancelada: cerrada sin hecha, y el total ya no crece.
    t.run("task.move", &[("task", "API-2"), ("status", "cancelled")]);
    t.clock = Some(at(200));
    let gone = t.run("task.timeline", &[("task", "API-2")]);
    assert_eq!(gone["done_at"], Value::Null);
    assert_eq!(gone["closed_at"], serde_json::json!(at(130)));
    assert_eq!(gone["total_minutes"], 30);
    let last = gone["steps"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (&last["status"], &last["minutes"]),
        (&"cancelled".into(), &0.into())
    );
}

/// Reabierta al día siguiente: el día en que se cerró la sigue contando.
#[test]
fn reopening_later_does_not_change_the_period() {
    let mut t = T::new();
    let (dev, rev) = setup(&mut t);
    script(&mut t, &dev, &rev);
    t.clock = Some(at(24 * 60));
    t.run("task.move", &[("task", "API-1"), ("status", "todo")]);
    let mut day = |d: &str| {
        let f = t.run(
            "metrics.flow",
            &[("project", "API"), ("from", d), ("to", d)],
        );
        (
            f["summary"]["done"].clone(),
            f["summary"]["returns"].clone(),
        )
    };
    assert_eq!(day("2020-03-02"), (1.into(), 1.into()));
    // Reabrir no es una devolución.
    assert_eq!(day("2020-03-03"), (0.into(), 0.into()));
}

/// El ritmo semanal no se rebaja con los días del periodo que aún no han
/// llegado.
#[test]
fn throughput_ignores_future_days() {
    let mut t = T::new();
    let (dev, rev) = setup(&mut t);
    script(&mut t, &dev, &rev);
    let f = t.run(
        "metrics.flow",
        &[
            ("project", "API"),
            ("from", "2020-03-02"),
            ("to", "2020-03-08"),
        ],
    );
    assert_eq!(f["summary"]["throughput_per_week"], 7.0, "{f}");
    assert_eq!(f["throughput"].as_array().unwrap().len(), 7);
}

/// Sin tareas: nulos donde no hay datos, ceros en lo demás.
#[test]
fn empty_project_gives_nulls_and_zeros() {
    let mut t = T::new();
    t.run("project.create", &[("key", "api")]);
    let s = t.run("metrics.flow", &[("project", "API")])["summary"].clone();
    for k in [
        "lead_p50_minutes",
        "lead_p85_minutes",
        "cycle_p50_minutes",
        "cycle_p85_minutes",
    ] {
        assert_eq!(s[k], Value::Null, "{k}");
    }
    assert_eq!(
        (&s["done"], &s["handoffs"], &s["returns"]),
        (&0.into(), &0.into(), &0.into())
    );
    assert_eq!(
        (&s["returned_pct"], &s["wip_avg"]),
        (&0.0.into(), &0.0.into())
    );
    assert_eq!(s["throughput_per_week"], 0.0);
}

/// Con `epic` o `cycle` solo cuentan sus tareas.
#[test]
fn epic_and_cycle_filters() {
    let mut t = T::new();
    let (dev, rev) = setup(&mut t);
    script(&mut t, &dev, &rev);
    let epic = t.run("epic.create", &[("project", "API"), ("name", "Épica")]);
    let cycle = t.run("cycle.create", &[("project", "API"), ("name", "Sprint")]);
    let (epic, cycle) = (
        epic["id"].as_str().unwrap().to_owned(),
        cycle["id"].as_str().unwrap().to_owned(),
    );
    // API-2 se cierra en la épica y el ciclo; API-1 queda fuera de ambos.
    t.run("task.create", &[("project", "API"), ("title", "Otra")]);
    t.run("task.set_epic", &[("task", "API-2"), ("epic", &epic)]);
    t.run("task.set_cycle", &[("task", "API-2"), ("cycle", &cycle)]);
    for status in ["in_progress", "done"] {
        t.run("task.move", &[("task", "API-2"), ("status", status)]);
    }
    let mut done = |args: &[(&str, &str)]| {
        let mut all = vec![("project", "API")];
        all.extend_from_slice(args);
        t.run("metrics.flow", &all)["summary"]["done"].clone()
    };
    assert_eq!(done(&[]), Value::from(2));
    assert_eq!(done(&[("epic", &epic)]), Value::from(1));
    assert_eq!(done(&[("cycle", &cycle), ("days", "30")]), Value::from(1));
    let burn = t.run(
        "metrics.burndown",
        &[("project", "API"), ("epic", &epic), ("days", "1")],
    );
    let last = burn["series"].as_array().unwrap().last().unwrap().clone();
    assert_eq!((&last["done"], &last["remaining"]), (&1.into(), &0.into()));
}

/// Sin proyecto, burndown y flujo acumulado suman todos los del alcance.
#[test]
fn burndown_and_cfd_without_project_add_up_all_projects() {
    let mut t = T::new();
    let (dev, rev) = setup(&mut t);
    script(&mut t, &dev, &rev);
    t.run("project.create", &[("key", "web")]);
    for title in ["Uno", "Dos"] {
        t.run("task.create", &[("project", "WEB"), ("title", title)]);
    }
    t.run("task.move", &[("task", "WEB-1"), ("status", "in_progress")]);

    let last = |v: &Value| v["series"].as_array().unwrap().last().unwrap().clone();
    for cmd in ["metrics.burndown", "metrics.cfd"] {
        let all = t.run(cmd, &[("days", "2")]);
        assert_eq!(all["project"], Value::Null);
        let api = last(&t.run(cmd, &[("project", "API"), ("days", "2")]));
        let web = last(&t.run(cmd, &[("project", "WEB"), ("days", "2")]));
        let total = last(&all);
        for (k, v) in total.as_object().unwrap() {
            if k == "date" {
                continue;
            }
            let sum = api[k].as_i64().unwrap() + web[k].as_i64().unwrap();
            assert_eq!(v.as_i64().unwrap(), sum, "{cmd} {k}");
        }
    }
    let burn = last(&t.run("metrics.burndown", &[("days", "2")]));
    assert_eq!((&burn["done"], &burn["remaining"]), (&1.into(), &2.into()));
    let cfd = last(&t.run("metrics.cfd", &[("days", "2")]));
    assert_eq!(
        (&cfd["done"], &cfd["in_progress"], &cfd["todo"]),
        (&1.into(), &1.into(), &1.into())
    );
}
