//! Cada vista, diálogo y panel a los tamaños de ventana de [`SIZES`], sin
//! ventana real: un test que lo dibuja todo (tamaños negativos y caracteres
//! sin glifo hacen fallar) y una galería de capturas para revisarlo a ojo,
//! `TASKCRAFT_GALLERY=dir cargo test -p ui-egui gallery -- --ignored`.

use egui_kittest::Harness;
use engine::{Context, Engine};
use store_sqlite::SqliteStore;

use super::*;
use crate::widgets::date_picker::add_days;

/// Mínimo de la ventana, portátil pequeño, el tamaño inicial y pantalla completa.
pub(crate) const SIZES: [(f32, f32); 4] = [
    (640.0, 400.0),
    (1024.0, 680.0),
    (1440.0, 860.0),
    (1920.0, 1080.0),
];

pub(crate) const VIEWS: [ViewKind; 16] = [
    ViewKind::List,
    ViewKind::Board,
    ViewKind::Calendar,
    ViewKind::Table,
    ViewKind::Gantt,
    ViewKind::Today,
    ViewKind::Matrix,
    ViewKind::Graph,
    ViewKind::Review,
    ViewKind::Inbox,
    ViewKind::Agents,
    ViewKind::Trash,
    ViewKind::Cycles,
    ViewKind::Epics,
    ViewKind::Diagrams,
    ViewKind::Metrics,
];

fn run(app: &mut TaskCraftApp, id: &'static str, pairs: &[(&str, &str)]) {
    app.apply(Intent::run(id, pairs));
    assert!(app.message.is_ok(), "{id}: {:?}", app.message);
}

/// Un workspace con algo de todo: dos proyectos, épicas, agentes, fechas,
/// subtareas, dependencias y títulos largos.
pub(crate) fn demo_app() -> TaskCraftApp {
    let mut engine = Engine::new(SqliteStore::open_in_memory().unwrap());
    let sup = Context::supervisor();
    for (key, name) in [("API", "Backend de pagos"), ("WEB", "Frontend web")] {
        engine
            .dispatch(
                &sup,
                "project.create",
                &Args::none().with("key", key).with("name", name),
            )
            .unwrap();
    }
    let mut app = TaskCraftApp::new(Conn::local(engine), |k| {
        (k == prefs::LANG).then(|| "es".to_owned())
    });
    let day = |n: i64| add_days(today(), n).to_string();
    run(&mut app, "label.create", &[("name", "backend")]);
    run(&mut app, "label.create", &[("name", "bug")]);
    run(
        &mut app,
        "epic.create",
        &[("project", "API"), ("name", "Pagos")],
    );
    run(
        &mut app,
        "epic.create",
        &[("project", "API"), ("name", "v1.0"), ("kind", "milestone")],
    );
    run(&mut app, "cycle.create", &[("project", "API")]);
    let tasks: [(&str, &str, &str, i64); 12] = [
        (
            "API",
            "Diseñar el esquema de la base de datos de pagos",
            "high",
            1,
        ),
        ("API", "Implementar el repositorio", "medium", 3),
        (
            "API",
            "Tests de concurrencia con veinte clientes simultáneos y reintentos",
            "urgent",
            -2,
        ),
        ("API", "Documentar la API", "low", 9),
        ("API", "Revisar permisos", "medium", 0),
        ("API", "Migración v2", "none", 14),
        ("API", "Webhook de facturas", "high", 5),
        ("WEB", "Pantalla de inicio", "medium", 2),
        (
            "WEB",
            "Formulario de pago con validación en vivo",
            "high",
            4,
        ),
        ("WEB", "Modo oscuro", "low", 20),
        ("WEB", "Accesibilidad del menú", "medium", 7),
        (
            "WEB",
            "Arreglar el scroll del carrito en móviles",
            "urgent",
            0,
        ),
    ];
    for (project, title, priority, due) in tasks {
        run(
            &mut app,
            "task.create",
            &[
                ("project", project),
                ("title", title),
                ("priority", priority),
                ("due", &day(due)),
                ("start", &day(due - 3)),
            ],
        );
    }
    for (parent, title) in [
        ("API-1", "Tabla de cobros"),
        ("API-1", "Índices"),
        ("API-1", "Claves foráneas"),
    ] {
        run(
            &mut app,
            "task.create",
            &[("parent", parent), ("title", title)],
        );
    }
    for (task, status) in [
        ("API-1", "in_progress"),
        ("API-2", "todo"),
        ("API-3", "in_progress"),
        ("API-4", "in_progress"),
        ("API-4", "done"),
        ("API-5", "in_progress"),
        ("WEB-1", "in_progress"),
        ("WEB-2", "todo"),
        ("WEB-4", "in_progress"),
        ("WEB-4", "done"),
    ] {
        run(&mut app, "task.move", &[("task", task), ("status", status)]);
    }
    run(
        &mut app,
        "task.move",
        &[("task", "API-5"), ("status", "in_review")],
    );
    for (task, epic) in [("API-1", "pagos"), ("API-2", "pagos"), ("API-7", "pagos")] {
        run(&mut app, "task.set_epic", &[("task", task), ("epic", epic)]);
    }
    // Dos sprints en API (el actual y el siguiente), uno largo en WEB, un hito
    // con tareas y una vista guardada del proyecto.
    run(&mut app, "cycle.create", &[("project", "API")]);
    run(
        &mut app,
        "cycle.create",
        &[
            ("project", "WEB"),
            (
                "name",
                "Ciclo de rediseño de la tienda en línea de primavera",
            ),
        ],
    );
    for (task, cycle) in [
        ("API-1", "current"),
        ("API-2", "current"),
        ("API-3", "current"),
        ("API-4", "current"),
        ("API-6", "next"),
        ("WEB-1", "current"),
        ("WEB-4", "current"),
    ] {
        run(
            &mut app,
            "task.set_cycle",
            &[("task", task), ("cycle", cycle)],
        );
    }
    // API-1 está en los dos sprints de API.
    run(
        &mut app,
        "task.add_cycle",
        &[("task", "API-1"), ("cycle", "next")],
    );
    for task in ["API-4", "API-6"] {
        run(
            &mut app,
            "task.set_milestone",
            &[("task", task), ("milestone", "v1.0")],
        );
    }
    run(
        &mut app,
        "view.save",
        &[
            ("name", "Urgentes de pagos"),
            ("filter", "priority:urgent"),
            ("project", "API"),
        ],
    );
    run(
        &mut app,
        "view.save",
        &[("name", "Para esta semana"), ("filter", "due:week")],
    );
    run(
        &mut app,
        "task.link",
        &[("blocked", "API-2"), ("blocker", "API-1")],
    );
    run(
        &mut app,
        "task.link",
        &[("blocked", "API-3"), ("blocker", "API-2")],
    );
    run(
        &mut app,
        "task.label",
        &[("task", "API-3"), ("add", "backend,bug")],
    );
    run(
        &mut app,
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Flujo de cobro"),
            (
                "source",
                "flowchart LR\n  C[Cliente] --> P{¿Pago válido?}\n  P -->|sí| R[Registrar cobro]\n  P -->|no| E[Avisar del error]",
            ),
        ],
    );
    run(
        &mut app,
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Borrador roto"),
            ("source", "esto no es mermaid"),
        ],
    );
    run(
        &mut app,
        "task.update",
        &[
            ("task", "API-1"),
            (
                "description",
                "Tablas de **cobros** y reembolsos.\n\n- Índices por cliente\n- Ver [[API-2]] y [[API-D1]]\n\n![[API-D1]]\n\n![[API-D2]]",
            ),
        ],
    );
    run(
        &mut app,
        "comment.add",
        &[
            ("task", "API-1"),
            ("body", "¿Usamos UUID v7 para las claves?"),
        ],
    );
    // D22: una interfaz con dos agentes y otra con uno.
    for (interface, kind) in [("Claude Code", "claude-code"), ("Cursor", "cursor")] {
        run(
            &mut app,
            "interface.register",
            &[("name", interface), ("kind", kind)],
        );
    }
    for (name, interface) in [
        ("claude-1", "Claude Code"),
        ("claude-2", "Claude Code"),
        ("revisor", "Cursor"),
    ] {
        run(
            &mut app,
            "agent.register",
            &[("name", name), ("interface", interface)],
        );
    }
    run(
        &mut app,
        "task.add_assignee",
        &[("task", "API-1"), ("agent", "claude-1")],
    );
    run(
        &mut app,
        "task.add_assignee",
        &[("task", "WEB-1"), ("agent", "claude-2")],
    );
    // Varios asignados y columnas de un agente (D20).
    for agent in ["claude-2", "revisor"] {
        run(
            &mut app,
            "task.add_assignee",
            &[("task", "API-1"), ("agent", agent)],
        );
    }
    for (name, base) in [
        ("Cola de revisor", "todo"),
        ("Mesa de revisor", "in_progress"),
    ] {
        run(
            &mut app,
            "status.create",
            &[
                ("project", "API"),
                ("name", name),
                ("base", base),
                ("owner", "revisor"),
            ],
        );
    }
    run(
        &mut app,
        "task.move",
        &[
            ("task", "API-7"),
            ("status", "cola_de_revisor"),
            ("note", "Listo para revisar"),
        ],
    );
    app.st.dialog = None;
    app.reload_now();
    app
}

/// Relevos entre agentes y una devolución en API, para el flujo (TC-6):
/// una cola de claude-1, y API-6 va y vuelve del revisor. Sin agente, lo
/// mueve el supervisor.
fn handoffs(app: &mut TaskCraftApp) {
    // Una vez por app: el test de tamaños abre todas las escenas seguidas.
    let done = app
        .snap
        .projects
        .iter()
        .any(|p| p.workflow.iter().any(|s| s.key == "cola_de_dev"));
    if done {
        return;
    }
    run(
        app,
        "status.create",
        &[
            ("project", "API"),
            ("name", "Cola de dev"),
            ("base", "todo"),
            ("owner", "claude-1"),
        ],
    );
    for (task, status, agent) in [
        ("API-6", "cola_de_dev", ""),
        ("API-6", "in_progress", "claude-1"),
        ("API-6", "cola_de_revisor", "claude-1"),
        ("API-6", "mesa_de_revisor", "revisor"),
        ("API-6", "cola_de_dev", "revisor"),
        ("API-6", "in_progress", "claude-1"),
        ("API-6", "cola_de_revisor", "claude-1"),
        ("API-6", "mesa_de_revisor", "revisor"),
        ("API-6", "done", "revisor"),
        ("API-7", "mesa_de_revisor", "revisor"),
        ("API-7", "done", "revisor"),
        ("API-5", "done", "claude-2"),
    ] {
        let mut args = vec![("task", task), ("status", status)];
        if !agent.is_empty() {
            args.push(("as_agent", agent));
        }
        run(app, "task.move", &args);
    }
    if let Some(p) = app.snap.projects.iter().find(|p| p.key == "API") {
        app.st.scope = crate::data::Scope::Project(p.id);
    }
}

/// Diálogos y paneles abiertos encima, por nombre.
type Scene = (&'static str, fn(&mut TaskCraftApp));

const SCENES: [Scene; 18] = [
    ("board-project", |app| {
        if let Some(p) = app.snap.projects.iter().find(|p| p.key == "API") {
            app.st.scope = crate::data::Scope::Project(p.id);
        }
        app.st.view = ViewKind::Board;
        app.reload_now();
        app.open_task("API-1");
    }),
    ("metrics-project", |app| {
        handoffs(app);
        app.st.view = ViewKind::Metrics;
        app.reload_now();
    }),
    ("journey", |app| {
        // El detalle de API-6 con su Recorrido; las horas de verdad serían
        // segundos, así que el recorrido se sustituye por uno con días.
        handoffs(app);
        app.st.view = ViewKind::List;
        app.reload_now();
        app.open_task("API-6");
        // Lo pendiente de recargar, antes: si no, pisaría el recorrido.
        app.poll();
        let at = |hours: i64| {
            model::Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_hours(hours))
                .map(|t| t.to_string())
                .unwrap_or_default()
        };
        let step = |status: &str, column: &str, from: i64, to: i64, who: &str| {
            serde_json::json!({
                "status": status, "column": column, "entered_at": at(from),
                "left_at": at(to), "minutes": (from - to) * 60, "actor_name": who,
            })
        };
        let timeline = serde_json::json!({
            "done_at": at(2), "closed_at": at(2), "total_minutes": 75 * 60,
            "steps": [
                step("todo", "todo", 75, 70, "supervisor"),
                step("todo", "Cola de dev", 70, 52, "supervisor"),
                step("in_progress", "in_progress", 52, 30, "claude-1"),
                step("todo", "Cola de revisor", 30, 27, "claude-1"),
                step("in_progress", "Mesa de revisor", 27, 26, "revisor"),
                step("todo", "Cola de dev", 26, 20, "revisor"),
                step("in_progress", "in_progress", 20, 8, "claude-1"),
                step("todo", "Cola de revisor", 8, 3, "claude-1"),
                step("in_progress", "Mesa de revisor", 3, 2, "revisor"),
                {"status": "done", "column": "done", "entered_at": at(2), "left_at": null,
                 "minutes": 120, "actor_name": "revisor"},
            ],
        });
        if let Some(d) = app.detail.as_mut() {
            d.journey = crate::data::Journey::from_json(&timeline);
        }
    }),
    ("shortcuts", |app| app.st.dialog = Some(Dialog::Shortcuts)),
    ("delete_data", |app| {
        let mut form =
            crate::state::DeleteForm::new(crate::state::DeleteWhat::Project, Some("API".into()));
        form.confirm = "AP".into();
        app.st.dialog = Some(Dialog::DeleteData(form));
    }),
    ("interface_edit", |app| {
        app.st.view = ViewKind::Agents;
        let form = app
            .snap
            .interfaces
            .first()
            .map(|i| crate::state::InterfaceForm::edit(i, &app.snap));
        app.st.dialog = form.map(Dialog::Interface);
    }),
    ("agent_add", |app| {
        app.st.view = ViewKind::Agents;
        app.st.dialog = Some(Dialog::Agent(crate::state::AgentForm::new("Claude Code")));
    }),
    ("new_token", |app| {
        app.st.view = ViewKind::Agents;
        app.st.dialog = Some(Dialog::ConfirmNewToken("Claude Code".into()));
    }),
    ("diagram_editor", |app| {
        app.open_diagram("API-D1");
        let editing = app
            .snap
            .diagram_by_key("API-D1")
            .map(|d| crate::state::DiagramEditor::edit(&d.diagram));
        app.st.diagrams.editor = editing;
    }),
    ("project", |app| {
        app.st.dialog = Some(Dialog::Project(ProjectForm::default()))
    }),
    ("workflow", |app| {
        let id = app.snap.projects[0].id;
        app.st.dialog = Some(Dialog::Workflow(crate::state::WorkflowForm::new(id)));
    }),
    ("palette", |app| app.apply(Intent::OpenPalette)),
    ("sidebar", |app| app.st.panels.sidebar_peek = true),
    ("sidebar-project", |app| {
        // El sprint actual de API elegido desde la barra: proyecto desplegado.
        let cycle = app
            .snap
            .cycles
            .iter()
            .find(|c| c.name == "Sprint 1")
            .cloned();
        if let Some(c) = cycle {
            app.go_to(crate::state::GoTo::cycle(&c));
        }
        app.st.panels.sidebar_peek = true;
    }),
    ("cycles", |app| app.st.view = ViewKind::Cycles),
    ("new-sprint", |app| {
        let id = app.snap.projects[0].id;
        let form = crate::state::CycleForm::after_last(&app.snap, id, today());
        app.st.dialog = Some(Dialog::Cycle(form));
    }),
    ("activity", |app| app.st.panels.activity_peek = true),
    ("huge-text", |app| {
        app.st.text_size = crate::theme::TextSize::Huge;
        app.st.view = ViewKind::Board;
    }),
];

fn builder(size: (f32, f32)) -> egui_kittest::HarnessBuilder<TaskCraftApp> {
    Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .with_theme(egui::Theme::Light)
        .with_max_steps(8)
        .with_accessibility_check(false)
}

/// Con render por GPU, para las capturas.
fn harness(app: TaskCraftApp, size: (f32, f32)) -> Harness<'static, TaskCraftApp> {
    builder(size)
        .wgpu()
        .build_ui_state(|ui, app: &mut TaskCraftApp| app.draw(ui), app)
}

#[test]
fn every_view_and_dialog_fits_every_window_size() {
    for size in SIZES {
        // Sin GPU: unos frames bastan para que egui distribuya y dibuje todo.
        let mut h =
            builder(size).build_ui_state(|ui, app: &mut TaskCraftApp| app.draw(ui), demo_app());
        for view in VIEWS {
            h.state_mut().st.view = view;
            h.state_mut().apply(Intent::Select(None));
            h.run_steps(2);
            h.state_mut().open_task("API-1");
            h.run_steps(2);
        }
        for (_, open) in SCENES {
            let app = h.state_mut();
            app.st.dialog = None;
            app.st.panels = Default::default();
            open(app);
            h.run_steps(2);
        }
    }
}

fn shoot(h: &mut Harness<'_, TaskCraftApp>, dir: &std::path::Path, name: &str) {
    h.run_steps(4);
    let image = h.render().unwrap();
    image.save(dir.join(format!("{name}.png"))).unwrap();
}

#[test]
#[ignore = "genera capturas; TASKCRAFT_GALLERY=dir"]
fn gallery() {
    let Ok(dir) = std::env::var("TASKCRAFT_GALLERY") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let only = std::env::var("TASKCRAFT_GALLERY_ONLY").unwrap_or_default();
    for size in SIZES {
        let tag = format!("{}x{}", size.0, size.1);
        for view in VIEWS {
            if !only.is_empty() && !only.split(',').any(|v| v == view.code()) {
                continue;
            }
            let mut app = demo_app();
            app.st.view = view;
            app.reload_now();
            let mut h = harness(app, size);
            shoot(&mut h, &dir, &format!("{tag}-{}", view.code()));
            if view == ViewKind::Gantt {
                // Desplazada a la derecha: la columna de nombres debe seguir ahí.
                h.hover_at(egui::pos2(size.0 * 0.6, size.1 * 0.5));
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-400.0, 0.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
                shoot(&mut h, &dir, &format!("{tag}-gantt-scrolled"));
            }
            if matches!(view, ViewKind::List | ViewKind::Board) {
                h.state_mut().open_task("API-1");
                shoot(&mut h, &dir, &format!("{tag}-{}-detail", view.code()));
            }
        }
        // Las escenas salen con «dialogs» o por su nombre o prefijo
        // («metrics» incluye «metrics-project»).
        let wanted = |name: &str| {
            only.is_empty()
                || only.split(',').any(|v| {
                    v == "dialogs"
                        || name == v
                        || name.strip_prefix(v).is_some_and(|r| r.starts_with('-'))
                })
        };
        for (name, open) in SCENES {
            if !wanted(name) {
                continue;
            }
            let mut app = demo_app();
            open(&mut app);
            let mut h = harness(app, size);
            if name == "journey" {
                // El Recorrido está al final del detalle: se baja hasta él.
                h.run_steps(2);
                h.hover_at(egui::pos2(size.0 * 0.75, size.1 * 0.5));
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -3000.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            shoot(&mut h, &dir, &format!("{tag}-dialog-{name}"));
        }
    }
}
