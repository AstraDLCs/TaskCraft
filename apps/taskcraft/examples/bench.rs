//! Banco de rendimiento (RNF-06): siembra 50 000 tareas y mide `task.next`,
//! `task.claim`, la recarga de la app y un frame del kanban con 500 tarjetas.
//! Se ejecuta con `cargo xtask bench` (en release).

#![allow(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

use eframe::egui;
use engine::{Args, Context, Engine, Value};
use model::Actor;
use store_sqlite::SqliteStore;
use ui_egui::{Conn, TaskCraftApp};

const BIG_PROJECTS: usize = 10;
const TASKS_PER_BIG: usize = 4_950;
const BOARD_CARDS: usize = 500;
const RUNS: usize = 20;

fn main() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("taskcraft-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db = dir.join("workspace.db");
    let mut engine = Engine::new(SqliteStore::open(&db).map_err(|e| e.to_string())?);
    let sup = Context::supervisor();

    let seeding = Instant::now();
    let file = dir.join("seed.taskcraft");
    let dump = seed(&mut engine)?;
    let manifest = exchange::Manifest {
        format: exchange::FORMAT.into(),
        version: exchange::VERSION,
        exported_at: model::Timestamp::now(),
        app_version: "bench".into(),
        workspace: "bench".into(),
        projects: Vec::new(),
    };
    let out = std::fs::File::create(&file).map_err(|e| e.to_string())?;
    exchange::write(std::io::BufWriter::new(out), &manifest, &dump).map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().into_owned();
    let import = Instant::now();
    run(&mut engine, &sup, "workspace.import", &[("path", &path)])?;
    println!("importadas en {:.1?}", import.elapsed());
    let total = BIG_PROJECTS * TASKS_PER_BIG + BOARD_CARDS;
    println!("sembradas {total} tareas en {:.1?}", seeding.elapsed());

    run(
        &mut engine,
        &sup,
        "interface.register",
        &[("name", "bench")],
    )?;
    let agent = run(
        &mut engine,
        &sup,
        "agent.register",
        &[("name", "bench"), ("interface", "bench")],
    )?;
    let id = agent["id"]
        .as_str()
        .ok_or("sin id")?
        .parse()
        .map_err(|_| "id")?;
    let as_agent = Context::new(Actor::Agent(id));

    let next = measure(|| run(&mut engine, &as_agent, "task.next", &[]).map(drop))?;
    report("task.next", &next, Duration::from_millis(50));
    let claim = measure(|| {
        let got = run(&mut engine, &as_agent, "task.next", &[("claim", "true")])?;
        let key = got["task"]["key"]
            .as_str()
            .ok_or("nada que tomar")?
            .to_owned();
        run(
            &mut engine,
            &as_agent,
            "task.release",
            &[("task", &key), ("reason", "bench")],
        )
        .map(drop)
    })?;
    report(
        "task.next claim=true (+ release)",
        &claim,
        Duration::from_millis(100),
    );
    let list = measure(|| run(&mut engine, &sup, "task.list", &[("project", "P0")]).map(drop))?;
    report("task.list (página de 50)", &list, Duration::from_millis(50));

    // La app: recarga completa (lo que pasa con cada evento) y frames del kanban.
    let mut app = TaskCraftApp::new(Conn::local(engine), |_| None);
    app.show_board_of("BRD");
    println!("tarjetas en el tablero: {}", app.visible_tasks());
    let reload = measure(|| {
        app.force_reload();
        Ok(())
    })?;
    report(
        "recarga de la app (proyecto de 500)",
        &reload,
        Duration::from_millis(100),
    );

    let ctx = egui::Context::default();
    let frame = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        ..Default::default()
    };
    let mut frames = Vec::new();
    for _ in 0..(RUNS + 5) {
        let start = Instant::now();
        let output = ctx.run_ui(frame(), |ui| app.draw(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        frames.push(start.elapsed());
    }
    let frames = frames.split_off(5);
    report(
        "frame del kanban (500 tarjetas)",
        &frames,
        Duration::from_micros(16_667),
    );

    // Peor caso: la lista de todos los proyectos (50 000 filas).
    app.show_all_list();
    println!("filas en la lista global: {}", app.visible_tasks());
    let reload_all = measure(|| {
        app.force_reload();
        Ok(())
    })?;
    report(
        "recarga de la app (50 000 tareas)",
        &reload_all,
        Duration::from_millis(1000),
    );
    let mut frames = Vec::new();
    for _ in 0..(RUNS + 5) {
        let start = Instant::now();
        let output = ctx.run_ui(frame(), |ui| app.draw(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        frames.push(start.elapsed());
    }
    let frames = frames.split_off(5);
    report(
        "frame de la lista (50 000 filas)",
        &frames,
        Duration::from_micros(16_667),
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// 10 proyectos de 4 950 tareas (70 % cerradas, cadenas de dependencias,
/// 20 etiquetas) y uno de 500 tarjetas abiertas para el tablero.
fn seed(engine: &mut Engine) -> Result<exchange::Dump, String> {
    use model::{
        Dependency, Label, LabelId, Priority, Project, ProjectId, ProjectStatus, Status, Task,
        TaskId,
    };
    let workspace = engine
        .read(|r| Ok(r.workspace()?))
        .map_err(|e| e.to_string())?;
    let now = model::Timestamp::now();
    let labels: Vec<Label> = (0..20)
        .map(|i| Label {
            id: LabelId::generate(),
            name: format!("l{i}"),
            color: "#6e56cf".into(),
        })
        .collect();
    let mut projects = Vec::new();
    let mut tasks = Vec::new();
    let mut dependencies = Vec::new();
    let mut add_project = |key: String, count: usize, open_only: bool| {
        let pid = ProjectId::generate();
        projects.push(Project {
            id: pid,
            name: key.clone(),
            key,
            description: String::new(),
            status: ProjectStatus::Active,
            next_number: count as u64 + 1,
            next_diagram: 1,
            created_at: now,
            updated_at: now,
            locked: false,
            workflow: Vec::new(),
            estimate_unit: model::EstimateUnit::Hours,
            fields: Vec::new(),
        });
        let mut prev: Option<TaskId> = None;
        for n in 0..count {
            let status = if open_only {
                [
                    Status::Backlog,
                    Status::Todo,
                    Status::Todo,
                    Status::InReview,
                ][n % 4]
            } else {
                match n % 10 {
                    0 | 1 => Status::Todo,
                    2 => Status::Backlog,
                    9 => Status::Cancelled,
                    _ => Status::Done,
                }
            };
            let id = TaskId::generate();
            if !open_only
                && n % 10 == 1
                && let Some(p) = prev
            {
                dependencies.push(Dependency {
                    blocker: p,
                    blocked: id,
                });
            }
            prev = Some(id);
            tasks.push(Task {
                id,
                project_id: pid,
                number: n as u64 + 1,
                parent_id: None,
                title: format!("Tarea {n}"),
                description: String::new(),
                status,
                priority: [
                    Priority::Urgent,
                    Priority::High,
                    Priority::Medium,
                    Priority::Low,
                    Priority::None,
                ][n % 5],
                start: None,
                due: None,
                labels: labels.get(n % 20).map(|l| vec![l.id]).unwrap_or_default(),
                assignees: Vec::new(),
                creator: Actor::Supervisor,
                checklist: Vec::new(),
                criteria: Vec::new(),
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
            });
        }
    };
    for p in 0..BIG_PROJECTS {
        add_project(format!("P{p}"), TASKS_PER_BIG, false);
    }
    add_project("BRD".into(), BOARD_CARDS, true);
    Ok(exchange::Dump {
        workspace,
        projects,
        labels,
        interfaces: Vec::new(),
        agents: Vec::new(),
        tasks,
        dependencies,
        comments: Vec::new(),
        activity: Vec::new(),
        inbox: Vec::new(),
        epics: Vec::new(),
        templates: Vec::new(),
        attachments: Vec::new(),
        relations: Vec::new(),
        views: Vec::new(),
        costs: Vec::new(),
        git_links: Vec::new(),
        cycles: Vec::new(),
        diagrams: Vec::new(),
        blobs: Default::default(),
    })
}

fn run(
    engine: &mut Engine,
    ctx: &Context,
    id: &str,
    pairs: &[(&str, &str)],
) -> Result<Value, String> {
    let args = pairs.iter().fold(Args::none(), |a, (k, v)| a.with(*k, *v));
    engine
        .dispatch(ctx, id, &args)
        .map_err(|e| format!("{id}: {e}"))
}

fn measure(mut f: impl FnMut() -> Result<(), String>) -> Result<Vec<Duration>, String> {
    let mut times = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        f()?;
        times.push(start.elapsed());
    }
    Ok(times)
}

fn report(name: &str, times: &[Duration], target: Duration) {
    let mut sorted = times.to_vec();
    sorted.sort();
    let at = |q: f64| {
        sorted
            .get(((sorted.len() as f64 - 1.0) * q) as usize)
            .copied()
            .unwrap_or_default()
    };
    let p95 = at(0.95);
    let verdict = if p95 <= target { "✓" } else { "✗" };
    println!(
        "{verdict} {name:<38} p50 {:>8.2?}  p95 {:>8.2?}  máx {:>8.2?}  (objetivo {:?})",
        at(0.5),
        p95,
        at(1.0),
        target
    );
}
