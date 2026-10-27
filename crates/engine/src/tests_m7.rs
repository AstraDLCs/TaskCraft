//! M7: tiempo automático del historial, cronómetro, registro manual, hojas
//! de horas y métricas, con un reloj controlado.

use model::{Actor, Timestamp};

use super::T;

fn at(base: Timestamp, min: i64) -> Timestamp {
    base.checked_add(jiff::SignedDuration::from_mins(min))
        .unwrap()
}

#[test]
fn agent_time_comes_from_history_and_logs_add_up() {
    let mut t = T::with_tasks(2);
    let base = Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(3))
        .unwrap();
    let bot = t.agent("bot");
    t.as_(&bot);
    t.clock = Some(base);
    t.run("task.claim", &[("task", "API-1")]);
    t.clock = Some(at(base, 45));
    t.run(
        "task.submit_review",
        &[("task", "API-1"), ("summary", "listo")],
    );

    t.actor = Actor::Supervisor;
    t.clock = Some(at(base, 50));
    t.run("timer.start", &[("task", "API-2")]);
    t.clock = Some(at(base, 70));
    let stopped = t.run("timer.stop", &[]);
    assert_eq!(stopped["minutes"], 20);
    t.run(
        "time.log",
        &[("task", "API-2"), ("minutes", "15"), ("note", "reunión")],
    );
    t.clock = None;

    let report = t.run("time.report", &[]);
    let bot_row = report["by_actor"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actor"] == "bot")
        .unwrap()
        .clone();
    assert_eq!(bot_row["auto"], 45);
    let sup = report["by_actor"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actor"] == "supervisor")
        .unwrap()
        .clone();
    assert_eq!(sup["manual"], 35);
    assert_eq!(report["total_minutes"], 80);

    let sheet = t.run("time.sheet", &[("period", "month")]);
    let total: i64 = sheet["totals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["minutes"].as_i64().unwrap())
        .sum();
    assert_eq!(total, 80);
    assert!(sheet["rows"].to_string().contains("API-1"));
    assert_eq!(t.fail("timer.stop", &[]).key(), "timer-not-running");
}

#[test]
fn burndown_cfd_and_agent_metrics() {
    let mut t = T::with_tasks(3);
    let bot = t.agent("bot");
    t.as_(&bot);
    t.run("task.claim", &[("task", "API-1")]);
    t.run(
        "task.submit_review",
        &[("task", "API-1"), ("summary", "uno")],
    );
    t.actor = Actor::Supervisor;
    t.run("task.move", &[("task", "API-1"), ("status", "done")]);
    t.as_(&bot);
    t.run("task.claim", &[("task", "API-2")]);
    t.run(
        "task.submit_review",
        &[("task", "API-2"), ("summary", "dos")],
    );
    t.actor = Actor::Supervisor;
    t.run(
        "task.reject",
        &[("task", "API-2"), ("reason", "falta algo")],
    );

    let burn = t.run("metrics.burndown", &[("project", "API"), ("days", "3")]);
    let today = burn["series"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(today["done"], 1);
    assert_eq!(today["remaining"], 2);

    let cfd = t.run("metrics.cfd", &[("project", "API"), ("days", "2")]);
    let today = cfd["series"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(today["done"], 1);
    assert_eq!(today["todo"], 2);

    let agents = t.run("metrics.agents", &[]);
    let row = agents["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["agent"] == "bot")
        .unwrap()
        .clone();
    assert_eq!(row["submitted"], 2);
    assert_eq!(row["approved"], 1);
    assert_eq!(row["rejected"], 1);
    assert_eq!(row["rejection_rate"], 0.5);
}
