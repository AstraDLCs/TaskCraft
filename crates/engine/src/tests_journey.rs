//! D24: recorrido de cada tarea. Cada cambio de estado o de columna deja un
//! paso con quién y cuándo, a salvo de `activity.prune`.

use model::{Actor, AgentId, Status, Timestamp, Transition};
use store::TransitionFilter;

use super::T;

fn at(minute: i64) -> Timestamp {
    "2020-03-02T09:00:00Z"
        .parse::<Timestamp>()
        .unwrap()
        .checked_add(jiff::SignedDuration::from_mins(minute))
        .unwrap()
}

fn journey(t: &mut T, key: &str) -> Vec<Transition> {
    let prev = std::mem::replace(&mut t.actor, Actor::Supervisor);
    let id = t.run("task.get", &[("task", key)])["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    t.actor = prev;
    t.engine
        .read(|r| {
            Ok(r.transitions(&TransitionFilter {
                task: Some(id),
                ..TransitionFilter::default()
            })?)
        })
        .unwrap()
}

/// (desde, hasta) de cada paso: estado y columna.
fn path(steps: &[Transition]) -> Vec<(Option<String>, String)> {
    let show = |s: Status, stage: &Option<String>| match stage {
        Some(stage) => format!("{}/{stage}", s.as_str()),
        None => s.as_str().to_owned(),
    };
    steps
        .iter()
        .map(|t| {
            (
                t.from_status.map(|s| show(s, &t.from_stage)),
                show(t.to_status, &t.to_stage),
            )
        })
        .collect()
}

/// Proyecto API con una tarea y las colas de dev y rev; devuelve (dev, rev).
fn hand_over_setup(t: &mut T) -> (Actor, Actor) {
    t.clock = Some(at(0));
    t.run("project.create", &[("key", "api")]);
    t.run("task.create", &[("project", "API"), ("title", "Tarea")]);
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
    let id = |v: &serde_json::Value| Actor::Agent(v["id"].as_str().unwrap().parse().unwrap());
    (id(&dev), id(&rev))
}

#[test]
fn every_move_leaves_a_step_with_who_and_when() {
    let mut t = T::new();
    let (dev, rev) = hand_over_setup(&mut t);
    let mv = |t: &mut T, who: &Actor, minute, status| {
        t.actor = who.clone();
        t.clock = Some(at(minute));
        t.run("task.move", &[("task", "API-1"), ("status", status)]);
    };
    t.actor = dev.clone();
    t.clock = Some(at(10));
    t.run("task.claim", &[("task", "API-1")]);
    mv(&mut t, &dev, 40, "cola_de_rev");
    mv(&mut t, &rev, 55, "cola_de_dev");
    t.actor = dev.clone();
    t.clock = Some(at(60));
    t.run("task.claim", &[("task", "API-1")]);
    mv(&mut t, &dev, 90, "done");
    // Cambiar otra cosa (el título) no es un paso.
    t.actor = Actor::Supervisor;
    t.run("task.update", &[("task", "API-1"), ("title", "Otra")]);

    let steps = journey(&mut t, "API-1");
    let s = |x: &str| Some(x.to_owned());
    assert_eq!(
        path(&steps),
        vec![
            (None, "todo".to_owned()),
            (s("todo"), "in_progress".to_owned()),
            (s("in_progress"), "todo/cola_de_rev".to_owned()),
            (s("todo/cola_de_rev"), "todo/cola_de_dev".to_owned()),
            (s("todo/cola_de_dev"), "in_progress".to_owned()),
            (s("in_progress"), "done".to_owned()),
        ]
    );
    let who: Vec<&Actor> = steps.iter().map(|t| &t.actor).collect();
    assert_eq!(who, vec![&Actor::Supervisor, &dev, &dev, &rev, &dev, &dev]);
    let when: Vec<Timestamp> = steps.iter().map(|t| t.at).collect();
    assert_eq!(when, vec![at(0), at(10), at(40), at(55), at(60), at(90)]);
}

#[test]
fn pruning_history_keeps_the_journey() {
    let mut t = T::with_tasks(1);
    t.clock = Some(at(0));
    t.run("task.create", &[("project", "API"), ("title", "Vieja")]);
    t.clock = Some(at(30));
    t.run("task.move", &[("task", "API-2"), ("status", "in_progress")]);
    let before = journey(&mut t, "API-2");
    assert_eq!(before.len(), 2);

    t.clock = None;
    let dir = std::env::temp_dir().join(format!("taskcraft-journey-{}", AgentId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = t.run(
        "activity.prune",
        &[("before", "2021-01-01"), ("dir", &dir.to_string_lossy())],
    );
    assert!(out["deleted"].as_u64().unwrap() > 0, "{out}");
    assert_eq!(journey(&mut t, "API-2"), before);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Un archivo sin recorrido (anterior a D24) lo reconstruye al importarse a
/// partir de su historial: los mismos pasos, con su quién y su cuándo.
#[test]
fn tasks_without_a_journey_get_it_from_their_history() {
    let mut src = T::new();
    let (dev, _) = hand_over_setup(&mut src);
    src.actor = dev;
    src.clock = Some(at(10));
    src.run("task.claim", &[("task", "API-1")]);
    src.clock = Some(at(40));
    src.run("task.move", &[("task", "API-1"), ("status", "cola_de_rev")]);
    let original = journey(&mut src, "API-1");

    src.actor = Actor::Supervisor;
    src.clock = None;
    let dir = std::env::temp_dir().join(format!("taskcraft-journey-{}", AgentId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("antes.taskcraft");
    let path_str = file.to_string_lossy().into_owned();
    src.run("workspace.export", &[("path", &path_str)]);
    let (manifest, mut dump) = exchange::read(std::fs::File::open(&file).unwrap()).unwrap();
    assert!(!dump.transitions.is_empty(), "la v8 lo exporta");
    dump.transitions.clear();
    exchange::write(std::fs::File::create(&file).unwrap(), &manifest, &dump).unwrap();

    let mut dst = T::new();
    dst.run("workspace.import", &[("path", &path_str)]);
    let rebuilt = journey(&mut dst, "API-1");
    assert_eq!(path(&rebuilt), path(&original));
    let who_when = |s: &[Transition]| {
        s.iter()
            .map(|t| (t.actor.clone(), t.at))
            .collect::<Vec<_>>()
    };
    assert_eq!(who_when(&rebuilt), who_when(&original));
    // Ya no falta nada: una segunda pasada no escribe.
    assert_eq!(dst.engine.backfill_transitions().unwrap(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

/// D23: el recorrido se va con su proyecto.
#[test]
fn deleting_a_project_takes_its_journey_along() {
    let mut t = T::with_tasks(2);
    t.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    let project = t.run("project.get", &[("project", "API")])["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let of_project = |t: &mut T| {
        t.engine
            .read(|r| {
                Ok(r.transitions(&TransitionFilter {
                    project: Some(project),
                    ..TransitionFilter::default()
                })?)
            })
            .unwrap()
            .len()
    };
    assert_eq!(of_project(&mut t), 3);
    let dir = std::env::temp_dir().join(format!("taskcraft-journey-{}", AgentId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    t.run(
        "project.delete",
        &[
            ("project", "API"),
            ("confirm", "API"),
            ("dir", &dir.to_string_lossy()),
        ],
    );
    assert_eq!(of_project(&mut t), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Fusionar un archivo más nuevo actualiza tareas que ya estaban aquí sin
/// pasar por el engine: su recorrido se cierra hasta el estado que trae.
#[test]
fn merging_a_newer_file_closes_the_journey() {
    let mut src = T::with_tasks(1);
    let dir = std::env::temp_dir().join(format!("taskcraft-journey-{}", AgentId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("1.taskcraft").to_string_lossy().into_owned();
    let second = dir.join("2.taskcraft").to_string_lossy().into_owned();
    src.run("workspace.export", &[("path", &first)]);
    let mut dst = T::new();
    dst.run("workspace.import", &[("path", &first)]);

    src.run("task.move", &[("task", "API-1"), ("status", "in_progress")]);
    src.run("workspace.export", &[("path", &second)]);
    dst.run("workspace.import", &[("path", &second), ("mode", "merge")]);
    let steps = journey(&mut dst, "API-1");
    let last = steps.last().unwrap();
    assert_eq!(last.to_status, Status::InProgress, "{steps:?}");
    assert_eq!(last.from_status, Some(Status::Todo));
    std::fs::remove_dir_all(dir).unwrap();
}
