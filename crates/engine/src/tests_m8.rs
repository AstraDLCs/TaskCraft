//! M8: lenguaje de filtros en `task.list`, vistas guardadas y «Hoy».

use super::T;

#[test]
fn task_list_accepts_the_filter_language() {
    let mut t = T::with_tasks(4);
    t.run(
        "task.update",
        &[
            ("task", "API-1"),
            ("priority", "urgent"),
            ("due", "2020-01-01"),
        ],
    );
    t.run("task.label", &[("task", "API-2"), ("add", "docs")]);
    t.run("task.label", &[("task", "API-3"), ("add", "db")]);
    t.run("task.link", &[("blocker", "API-3"), ("blocked", "API-4")]);

    assert_eq!(t.keys(&[("filter", "priority>=high")]), ["API-1"]);
    assert_eq!(t.keys(&[("filter", "due<today")]), ["API-1"]);
    assert_eq!(
        t.keys(&[("filter", "label:docs OR label:db")]),
        ["API-2", "API-3"]
    );
    assert_eq!(t.keys(&[("filter", "blocked:true")]), ["API-4"]);
    assert_eq!(
        t.keys(&[("filter", "-label:docs -label:db ready:true")]),
        ["API-1"]
    );
    assert_eq!(t.keys(&[("filter", "\"tarea 2\"")]), ["API-2"]);
    let e = t.fail("task.list", &[("filter", "(status:todo")]);
    assert_eq!(e.key(), "rule-filter");
}

#[test]
fn saved_views_round_trip_and_combine_with_filters() {
    let mut t = T::with_tasks(3);
    t.run("task.update", &[("task", "API-1"), ("priority", "high")]);
    t.run("task.update", &[("task", "API-2"), ("priority", "urgent")]);
    let saved = t.run(
        "view.save",
        &[
            ("name", "Importantes"),
            ("filter", "priority>=high"),
            ("layout", "table"),
            ("columns", "key,title,priority"),
            ("project", "API"),
        ],
    );
    assert_eq!(saved["project"], "API");
    assert_eq!(saved["columns"].as_array().unwrap().len(), 3);

    let got = t.run("view.get", &[("view", "importantes")]);
    assert_eq!(got["total"], 2);
    assert_eq!(got["view"]["layout"], "table");
    assert_eq!(
        t.keys(&[("view", "Importantes"), ("filter", "priority:urgent")]),
        ["API-2"]
    );

    // Guardar con el mismo nombre la reemplaza.
    t.run(
        "view.save",
        &[("name", "IMPORTANTES"), ("filter", "priority:high")],
    );
    let list = t.run("view.list", &[]);
    assert_eq!(list["count"], 1);
    assert_eq!(list["items"][0]["filter"], "priority:high");

    assert_eq!(
        t.fail(
            "view.save",
            &[("name", "Rota"), ("filter", "status:todo OR")]
        )
        .key(),
        "rule-filter"
    );
    t.run("view.delete", &[("view", "importantes")]);
    assert_eq!(
        t.fail("view.get", &[("view", "importantes")]).key(),
        "notfound-view"
    );
}

#[test]
fn today_lists_overdue_due_and_marked_tasks() {
    let mut t = T::with_tasks(5);
    let today = jiff::Zoned::now().date();
    t.run("task.update", &[("task", "API-1"), ("due", "2020-01-01")]);
    t.run(
        "task.update",
        &[("task", "API-2"), ("due", &today.to_string())],
    );
    t.run("today.mark", &[("task", "API-3")]);
    t.run("task.update", &[("task", "API-5"), ("due", "2020-01-01")]);
    t.run("task.move", &[("task", "API-5"), ("status", "cancelled")]);

    let list = t.run("today.list", &[]);
    let rows: Vec<(String, String)> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["key"].as_str().unwrap().to_owned(),
                r["today"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("API-1".to_owned(), "overdue".to_owned()),
            ("API-2".to_owned(), "due".to_owned()),
            ("API-3".to_owned(), "marked".to_owned()),
        ]
    );
    assert_eq!(t.keys(&[("filter", "today:true")]), ["API-3"]);

    // La marca queda en el historial de la tarea.
    let history = t.run("activity.list", &[("task", "API-3")]);
    assert!(history.to_string().contains("today"));
    t.run("today.unmark", &[("task", "API-3")]);
    assert_eq!(t.run("today.list", &[])["total"], 2);
}
