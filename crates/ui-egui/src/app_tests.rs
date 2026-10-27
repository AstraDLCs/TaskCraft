use engine::{Context, Engine};
use store_sqlite::SqliteStore;

use super::*;
use crate::state::GoTo;
use egui_kittest::kittest::Queryable;

fn app_with_tasks() -> TaskCraftApp {
    let mut engine = Engine::new(SqliteStore::open_in_memory().unwrap());
    let sup = Context::supervisor();
    engine
        .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
        .unwrap();
    for title in ["padre", "otra"] {
        engine
            .dispatch(
                &sup,
                "task.create",
                &Args::none().with("project", "API").with("title", title),
            )
            .unwrap();
    }
    engine
        .dispatch(
            &sup,
            "task.create",
            &Args::none().with("parent", "API-1").with("title", "hija"),
        )
        .unwrap();
    TaskCraftApp::new(Conn::local(engine), |k| {
        (k == prefs::LANG).then(|| "es".to_owned())
    })
}

fn delete(app: &mut TaskCraftApp, key: &str) {
    app.apply(crate::views::list::delete_intent(key));
}

#[test]
fn starts_inside_the_only_project_with_the_tree() {
    let app = app_with_tasks();
    assert!(matches!(app.st.scope, Scope::Project(_)));
    let keys: Vec<(&str, usize)> = app
        .snap
        .rows
        .iter()
        .map(|r| (r.key.as_str(), r.depth))
        .collect();
    assert_eq!(keys, [("API-1", 0), ("API-3", 1), ("API-2", 0)]);
    assert_eq!(app.snap.rows[0].children_total, 1);
}

#[test]
fn deleting_shows_an_undo_toast_that_restores() {
    let mut app = app_with_tasks();
    delete(&mut app, "API-1");
    assert_eq!(app.snap.rows.len(), 1, "se va con su subtarea");
    let toast = app.toast.as_ref().unwrap();
    assert_eq!(tr(Lang::Es, &toast.message), "API-1 eliminada");
    app.apply(Intent::Undo);
    assert!(app.toast.is_none());
    assert_eq!(app.snap.rows.len(), 3, "restaurada con su subtarea");
}

#[test]
fn the_undo_toast_expires() {
    let mut app = app_with_tasks();
    delete(&mut app, "API-2");
    if let Some(t) = app.toast.as_mut() {
        t.expires = Instant::now();
    }
    app.last_poll = Instant::now() - POLL;
    app.poll();
    assert!(app.toast.is_none());
    app.apply(Intent::Undo);
    assert_eq!(
        app.snap.rows.len(),
        2,
        "pasado el plazo sigue en la papelera"
    );
}

#[test]
fn selecting_loads_the_detail_and_edits_go_through_the_engine() {
    let mut app = app_with_tasks();
    let id = app.snap.rows[0].id();
    app.apply(Intent::Select(Some(id)));
    let d = app.detail.as_ref().unwrap();
    assert_eq!(d.subtasks.len(), 1);
    assert_eq!(app.st.edit.title, "padre");
    app.apply(Intent::run(
        "task.update",
        &[("task", "API-1"), ("title", "renombrada")],
    ));
    assert_eq!(app.detail.as_ref().unwrap().row.task.title, "renombrada");
    assert_eq!(app.st.edit.title, "renombrada");
}

#[test]
fn filters_keep_ancestors_for_context_and_collapse_hides_children() {
    let mut app = app_with_tasks();
    app.st.filter.text = "hija".into();
    let rows = crate::state::list_rows(&app.snap, &app.st);
    let shown: Vec<(&str, bool)> = rows
        .iter()
        .map(|(r, direct)| (r.key.as_str(), *direct))
        .collect();
    assert_eq!(shown, [("API-1", false), ("API-3", true)]);

    app.st.filter.text.clear();
    let parent = app.snap.rows[0].id();
    app.st.collapsed.insert(parent);
    let rows = crate::state::list_rows(&app.snap, &app.st);
    assert_eq!(rows.len(), 2, "la hija queda plegada");
}

#[test]
fn creating_on_a_calendar_day_sets_the_due_date() {
    let mut app = app_with_tasks();
    let day = jiff::civil::date(2026, 11, 20);
    app.apply(Intent::CreateOnDay {
        title: "reunión".into(),
        day,
    });
    let row = app
        .snap
        .rows
        .iter()
        .find(|r| r.task.title == "reunión")
        .unwrap();
    assert_eq!(row.task.due.as_ref().map(|d| d.date()), Some(day));
}

#[test]
fn palette_lists_the_registry_and_runs_commands() {
    let mut app = app_with_tasks();
    app.apply(Intent::OpenPalette);
    let palette = app.st.palette.as_ref().unwrap();
    assert!(palette.commands.iter().any(|c| c.id == "task.next"));
    assert!(
        !palette
            .commands
            .iter()
            .any(|c| c.id.starts_with("session."))
    );
    app.apply(Intent::RunPalette {
        id: "label.create",
        args: Args::none().with("name", "urgente"),
    });
    assert!(matches!(
        app.st.palette.as_ref().unwrap().result,
        Some(Ok(_))
    ));
    assert_eq!(app.snap.labels.len(), 1);
}

#[test]
fn language_and_view_preferences() {
    let engine = Engine::new(SqliteStore::open_in_memory().unwrap());
    let app = TaskCraftApp::new(Conn::local(engine), |k| match k {
        prefs::LANG => Some("en".into()),
        prefs::VIEW => Some("board".into()),
        prefs::LANES => Some("assignee".into()),
        _ => None,
    });
    assert_eq!(app.t("ui-projects"), "Projects");
    assert_eq!(app.st.view, ViewKind::Board);
    assert_eq!(app.st.lanes, Lanes::Assignee);
}

#[test]
fn a_task_moved_to_done_stays_visible() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.move",
        &[("task", "API-2"), ("status", "in_progress")],
    ));
    app.apply(Intent::run(
        "task.move",
        &[("task", "API-2"), ("status", "done")],
    ));
    let in_list = crate::state::list_rows(&app.snap, &app.st)
        .iter()
        .any(|(r, _)| r.key == "API-2");
    let on_board = crate::state::flat_rows(&app.snap, &app.st, false)
        .iter()
        .any(|r| r.key == "API-2" && r.task.status == model::Status::Done);
    assert!(in_list && on_board, "sigue a la vista, tachada");

    app.st.filter.hide_closed = true;
    assert!(
        !crate::state::flat_rows(&app.snap, &app.st, false)
            .iter()
            .any(|r| r.key == "API-2")
    );
}

#[test]
fn a_task_becomes_a_subtask_and_back() {
    let mut app = app_with_tasks();
    let depth =
        |app: &TaskCraftApp, key: &str| app.snap.rows.iter().find(|r| r.key == key).unwrap().depth;
    let otra = app
        .snap
        .rows
        .iter()
        .find(|r| r.key == "API-2")
        .unwrap()
        .id();

    // Tab: cuelga de la tarea anterior de su mismo nivel (API-1).
    let prev = crate::hierarchy::indent_target(&app.snap, otra)
        .unwrap()
        .key
        .clone();
    assert_eq!(prev, "API-1");
    app.apply(crate::hierarchy::set_parent("API-2", Some(&prev)));
    assert_eq!(depth(&app, "API-2"), 1);
    assert_eq!(app.snap.rows[0].children_total, 2);

    // Mayús+Tab: de nuevo tarea principal, sin perder nada.
    let up = crate::hierarchy::outdent_target(&app.snap, otra).unwrap();
    assert_eq!(up, None);
    app.apply(crate::hierarchy::set_parent("API-2", up.as_deref()));
    assert_eq!(depth(&app, "API-2"), 0);
    assert!(app.message.is_ok());

    // Una tarea no puede colgar de su propia subtarea: el engine lo rechaza.
    app.apply(crate::hierarchy::set_parent("API-1", Some("API-3")));
    assert!(app.message.is_err());
    assert_eq!(depth(&app, "API-1"), 0);
}

#[test]
fn activity_feed_filters_by_type_and_actor() {
    let mut app = app_with_tasks();
    app.apply(Intent::Run {
        id: "comment.add",
        args: Args::none().with("task", "API-2").with("body", "nota"),
    });
    assert!(app.snap.activity.len() >= 5);

    app.apply(Intent::FilterActivity(crate::state::ActivityFilter {
        kind: Some(crate::state::EventKind::Comments),
        ..Default::default()
    }));
    assert_eq!(app.snap.activity.len(), 1);
    assert_eq!(app.snap.activity[0].command, "comment.add");

    app.apply(Intent::FilterActivity(crate::state::ActivityFilter {
        actor: Some(model::Actor::System),
        ..Default::default()
    }));
    assert!(app.snap.activity.is_empty(), "el sistema no hizo nada");
}

#[test]
fn search_finds_text_in_comments() {
    let mut app = app_with_tasks();
    app.apply(Intent::Run {
        id: "comment.add",
        args: Args::none()
            .with("task", "API-2")
            .with("body", "Falta el benchmark de leases"),
    });
    app.st.filter.text = "benchmark".into();
    app.refresh_search();
    let rows = crate::state::list_rows(&app.snap, &app.st);
    let keys: Vec<&str> = rows.iter().map(|(r, _)| r.key.as_str()).collect();
    assert_eq!(keys, ["API-2"], "solo aparece en un comentario");
}

#[test]
fn export_and_import_from_the_app() {
    use crate::state::ExchangeForm;
    let mut app = app_with_tasks();
    let path = std::env::temp_dir()
        .join(format!("tc-app-{}.taskcraft", model::TaskId::generate()))
        .to_string_lossy()
        .into_owned();
    app.st.dialog = Some(Dialog::Export(ExchangeForm::default()));
    app.apply(Intent::Exchange {
        id: "workspace.export",
        args: Args::none().with("path", path.as_str()),
    });
    let Some(Dialog::Export(f)) = &app.st.dialog else {
        panic!("el diálogo sigue abierto");
    };
    assert_eq!(f.result.as_ref().unwrap().as_ref().unwrap()["tasks"], 3);

    let empty = Engine::new(SqliteStore::open_in_memory().unwrap());
    let mut other = TaskCraftApp::new(Conn::local(empty), |_| None);
    other.st.dialog = Some(Dialog::Import(ExchangeForm::default()));
    let args = Args::none()
        .with("path", path.as_str())
        .with("mode", "workspace");
    other.apply(Intent::Exchange {
        id: "workspace.import_preview",
        args: args.clone(),
    });
    let Some(Dialog::Import(f)) = &other.st.dialog else {
        panic!();
    };
    assert_eq!(
        f.result.as_ref().unwrap().as_ref().unwrap()["can_import"],
        true
    );
    other.apply(Intent::Exchange {
        id: "workspace.import",
        args,
    });
    let Some(Dialog::Import(f)) = &other.st.dialog else {
        panic!();
    };
    assert!(f.imported);
    assert_eq!(other.snap.rows.len(), 3, "las tareas importadas se ven");
}

#[test]
fn snapshots_load_in_the_background_without_blocking() {
    use engine::{LocalDispatch, SharedEngine};
    let dir = std::env::temp_dir().join(format!("tc-loader-{}", model::TaskId::generate()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.db");
    let engine = SharedEngine::new(Engine::new(SqliteStore::open(&path).unwrap()));
    let sup = Context::supervisor();
    engine
        .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
        .unwrap();
    let writes = Box::new(LocalDispatch::new(engine.clone(), model::Actor::Supervisor));
    let replica = SqliteStore::open_read_only(&path).unwrap();
    let reopen = path.clone();
    let conn = Conn::remote(replica, writes).with_background_loader(Box::new(move || {
        SqliteStore::open_read_only(&reopen).map(|s| Box::new(s) as Box<dyn store::Store>)
    }));
    let mut app = TaskCraftApp::new(conn, |_| None);
    app.apply(Intent::Run {
        id: "task.create",
        args: Args::none().with("project", "API").with("title", "x"),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.snap.rows.is_empty() {
        assert!(Instant::now() < deadline, "la réplica nueva debía llegar");
        std::thread::sleep(Duration::from_millis(10));
        app.poll();
    }
    assert_eq!(app.snap.rows[0].key, "API-1");
}

fn run(app: &mut TaskCraftApp, id: &'static str, pairs: &[(&str, &str)]) {
    app.apply(Intent::run(id, pairs));
    assert!(app.message.is_ok(), "{id}: {:?}", app.message);
}

#[test]
fn stages_show_their_own_names_and_columns() {
    let mut app = app_with_tasks();
    run(
        &mut app,
        "status.create",
        &[("project", "API"), ("name", "QA"), ("base", "in_review")],
    );
    run(
        &mut app,
        "task.move",
        &[("task", "API-2"), ("status", "in_progress")],
    );
    run(
        &mut app,
        "task.move",
        &[("task", "API-2"), ("status", "qa")],
    );
    let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
    assert_eq!(
        crate::flow::status_name(&app.snap, &row.task, Lang::Es),
        "QA"
    );
    let Scope::Project(id) = app.st.scope else {
        panic!("dentro del proyecto");
    };
    let cols = crate::flow::columns(&app.snap, Some(id), Lang::Es);
    let qa = cols.iter().find(|c| c.key == "qa").unwrap();
    let review = cols.iter().find(|c| c.key == "in_review").unwrap();
    assert!(qa.holds(&row.task, true));
    assert!(!review.holds(&row.task, true), "en su etapa, no en la base");
    assert!(
        review.holds(&row.task, false),
        "con todos los proyectos, en la base"
    );
}

#[test]
fn epics_appear_with_progress_and_filter_the_lists() {
    let mut app = app_with_tasks();
    run(
        &mut app,
        "epic.create",
        &[("project", "API"), ("name", "Pagos")],
    );
    run(
        &mut app,
        "task.set_epic",
        &[("task", "API-2"), ("epic", "pagos")],
    );
    assert_eq!(app.snap.epics.len(), 1);
    assert_eq!(app.snap.epics[0].total, 1);
    app.st.filter.epic = Some(app.snap.epics[0].epic.id);
    let rows = crate::state::list_rows(&app.snap, &app.st);
    let keys: Vec<&str> = rows.iter().map(|(r, _)| r.key.as_str()).collect();
    assert_eq!(keys, ["API-2"]);
}

#[test]
fn task_links_in_markdown_open_the_task() {
    let mut app = app_with_tasks();
    app.apply(Intent::OpenKey("API-3".into()));
    assert_eq!(app.detail.as_ref().unwrap().row.key, "API-3");
}

/// RF-MOD-19: `[[API-D1]]` abre la vista Diagramas con el diagrama elegido;
/// el editor crea y guarda por el engine; la tarea que lo cita aparece.
#[test]
fn diagram_links_open_the_view_and_the_editor_saves_through_the_engine() {
    let mut app = app_with_tasks();
    let project = app.snap.projects[0].id;
    let mut editor = crate::state::DiagramEditor::new(project);
    editor.title = "Flujo".into();
    let (id, args) = editor.command("API");
    app.apply(Intent::SaveDiagram { id, args });
    assert!(app.message.is_ok(), "{:?}", app.message);
    let d = app.snap.diagram_by_key("api-d1").unwrap();
    assert_eq!(d.diagram.title, "Flujo");
    assert_eq!(app.st.diagrams.selected, Some(d.diagram.id));

    app.apply(Intent::run(
        "task.update",
        &[("task", "API-2"), ("description", "![[API-D1]]")],
    ));
    app.apply(Intent::OpenKey("API-2".into()));
    assert_eq!(
        app.snap
            .diagrams_cited_by(app.detail.as_ref().unwrap().row.id())
            .count(),
        1
    );

    app.apply(Intent::OpenDiagram("API-D1".into()));
    assert_eq!(app.st.view, ViewKind::Diagrams);
    assert!(app.st.selected.is_none(), "cierra el detalle de la tarea");
    let cited: Vec<&str> = app.snap.diagrams[0]
        .cited_by
        .iter()
        .map(|t| t.key.as_str())
        .collect();
    assert_eq!(cited, ["API-2"]);

    let mut edit = crate::state::DiagramEditor::edit(&app.snap.diagrams[0].diagram);
    edit.source = "sequenceDiagram\n  A->>B: hola".into();
    let (id, args) = edit.command("API");
    app.apply(Intent::SaveDiagram { id, args });
    assert!(
        app.snap.diagrams[0]
            .diagram
            .source
            .starts_with("sequenceDiagram")
    );

    app.apply(Intent::OpenDiagram("API-D9".into()));
    assert!(app.message.is_err(), "no existe");
}

/// La vista Diagramas, el editor y un diagrama incrustado se dibujan.
#[test]
fn diagrams_render_headless() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Flujo"),
            ("source", "flowchart TD\n  A --> B"),
        ],
    ));
    app.apply(Intent::run(
        "task.update",
        &[
            ("task", "API-1"),
            ("description", "Antes\n\n![[API-D1]]\n\nDespués"),
        ],
    ));
    let ctx = egui::Context::default();
    let frame = |app: &mut TaskCraftApp| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.draw(ui));
        output.textures_delta.clear();
        output.shapes
    };
    app.apply(Intent::OpenKey("API-1".into()));
    for _ in 0..2 {
        frame(&mut app);
    }
    assert_eq!(app.md.diagrams_cached(), 1, "el incrustado se dibujó");
    app.apply(Intent::OpenDiagram("API-D1".into()));
    for _ in 0..2 {
        assert!(!frame(&mut app).is_empty());
    }
    app.st.diagrams.editor = Some(crate::state::DiagramEditor::new(app.snap.projects[0].id));
    for _ in 0..2 {
        assert!(!frame(&mut app).is_empty());
    }
}

/// D22: la interfaz va primero (con un nombre libre) y es la única con
/// token, que se muestra al crearla y se puede volver a copiar.
#[test]
fn interfaces_come_first_and_their_token_can_be_copied() {
    let mut app = app_with_tasks();
    let form = crate::state::InterfaceForm {
        name: "Claude Code".into(),
        ..Default::default()
    };
    app.st.dialog = Some(Dialog::Interface(form.clone()));
    app.apply(form.intent());
    let Some(Dialog::Token { interface, token }) = app.st.dialog.clone() else {
        panic!("debía mostrar el token: {:?}", app.message);
    };
    assert_eq!(interface, "Claude Code");
    assert!(token.starts_with("tc_"));
    assert_eq!(app.snap.interfaces.len(), 1);
    assert!(app.snap.agents.is_empty(), "no crea agentes");

    app.apply(Intent::CopyToken("Claude Code".into()));
    assert_eq!(app.clipboard.as_deref(), Some(token.as_str()));

    // Un agente se añade dentro, sin token propio.
    let mut form = crate::state::AgentForm::new("Claude Code");
    form.name = "cc-dev".into();
    app.st.dialog = Some(Dialog::Agent(form.clone()));
    let (id, args) = form.command();
    app.apply(Intent::Submit { id, args });
    assert!(app.st.dialog.is_none(), "se cierra al guardar");
    assert_eq!(app.snap.agents.len(), 1);
    assert_eq!(
        app.snap.agents[0].agent.interface_id,
        Some(app.snap.interfaces[0].id)
    );
}

/// Si el guardado falla, el diálogo sigue abierto y muestra el error.
#[test]
fn a_failed_save_keeps_the_dialog_open_with_the_error() {
    let mut app = app_with_tasks();
    let mut form = crate::state::AgentForm::new("no-existe");
    form.name = "cc-dev".into();
    app.st.dialog = Some(Dialog::Agent(form.clone()));
    let (id, args) = form.command();
    app.apply(Intent::Submit { id, args });
    assert!(matches!(app.st.dialog, Some(Dialog::Agent(_))));
    assert!(app.st.dialog_error.is_some());
    // Un token que no se guardó (interfaz inexistente) da error, no copia nada.
    app.apply(Intent::CopyToken("no-existe".into()));
    assert!(app.clipboard.is_none() && app.message.is_err());
}

#[test]
fn new_reviews_and_questions_raise_notices() {
    let mut app = app_with_tasks();
    assert!(app.notices.is_empty());
    run(
        &mut app,
        "task.move",
        &[("task", "API-2"), ("status", "in_progress")],
    );
    run(
        &mut app,
        "task.move",
        &[("task", "API-2"), ("status", "in_review")],
    );
    assert_eq!(app.notices.len(), 1);
    assert!(app.notices[0].text.contains("API-2"));
    app.force_reload();
    assert_eq!(app.notices.len(), 1, "no se repite");
}

#[test]
fn opening_after_a_while_shows_what_happened() {
    let mut engine = Engine::new(SqliteStore::open_in_memory().unwrap());
    let sup = Context::supervisor();
    engine
        .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
        .unwrap();
    let since = "2020-01-01T00:00:00Z".to_owned();
    engine
        .dispatch(
            &sup,
            "task.create",
            &Args::none().with("project", "API").with("title", "x"),
        )
        .unwrap();
    let app = TaskCraftApp::new(Conn::local(engine), move |k| {
        (k == prefs::LAST_VISIT).then(|| since.clone())
    });
    let Some(Dialog::Summary(v)) = &app.st.dialog else {
        panic!("debía mostrar el resumen");
    };
    assert_eq!(v["created"], 1);
}

#[test]
fn metrics_and_timer_from_the_app() {
    let mut app = app_with_tasks();
    run(
        &mut app,
        "task.move",
        &[("task", "API-1"), ("status", "in_progress")],
    );
    run(
        &mut app,
        "task.move",
        &[("task", "API-1"), ("status", "done")],
    );
    app.st.view = crate::state::ViewKind::Metrics;
    app.apply(Intent::LoadMetrics);
    let data = app.st.metrics.data.clone().unwrap();
    let last = data.burndown["series"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["done"], 1);
    assert_eq!(data.flow["summary"]["done"], 1);

    app.apply(Intent::Select(Some(app.snap.rows[1].id())));
    run(&mut app, "timer.start", &[("task", "API-3")]);
    assert!(app.detail.as_ref().unwrap().time.running_here);
    run(&mut app, "timer.stop", &[]);
    run(
        &mut app,
        "time.log",
        &[("task", "API-3"), ("minutes", "30")],
    );
    assert_eq!(app.detail.as_ref().unwrap().time.manual, 30);
}

/// M8: el buscador entiende el lenguaje de filtros igual que `task.list`.
#[test]
fn the_search_box_accepts_the_filter_language() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.update",
        &[("task", "API-2"), ("priority", "urgent")],
    ));
    app.st.filter.text = "priority:urgent OR title~hij".into();
    let rows = crate::state::flat_rows(&app.snap, &app.st, false);
    let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["API-3", "API-2"]);
    assert!(app.st.filter.structured());

    // Mal escrito: se avisa y se busca como texto.
    app.st.filter.text = "(priority:urgent".into();
    assert!(app.st.filter.query().is_err());
    assert!(!app.st.filter.structured());
}

#[test]
fn saving_and_applying_a_view_restores_filter_layout_and_columns() {
    let mut app = app_with_tasks();
    app.st.view = ViewKind::Table;
    app.st.filter.text = "title~otra".into();
    app.st.table.columns = vec!["key".into(), "title".into()];
    app.apply(crate::dialogs::save_view_intent(
        &app.snap, &app.st, "Otras",
    ));
    assert_eq!(app.snap.views.len(), 1);

    app.st.view = ViewKind::List;
    app.st.filter.text.clear();
    app.st.table.columns.clear();
    let view = app.snap.views[0].clone();
    app.apply_view(&view);
    assert_eq!(app.st.view, ViewKind::Table);
    assert_eq!(app.st.filter.text, "title~otra");
    assert_eq!(app.st.table.columns, ["key", "title"]);
    assert_eq!(app.st.active_view.as_deref(), Some("Otras"));
}

#[test]
fn today_view_shows_marked_and_overdue_tasks() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.update",
        &[("task", "API-2"), ("due", "2020-01-01")],
    ));
    let row = app
        .snap
        .rows
        .iter()
        .find(|r| r.key == "API-1")
        .unwrap()
        .clone();
    app.apply(crate::views::today::toggle(&app.snap, &row));
    let today = rules::filter::today();
    let why: Vec<(String, crate::views::today::Why)> = app
        .snap
        .rows
        .iter()
        .filter_map(|r| crate::views::today::why(&app.snap, r, today).map(|w| (r.key.clone(), w)))
        .collect();
    assert_eq!(
        why,
        [
            ("API-1".to_owned(), crate::views::today::Why::Marked),
            ("API-2".to_owned(), crate::views::today::Why::Overdue),
        ]
    );
    // Desmarcar con el mismo atajo.
    app.st.selected = Some(row.id());
    let mut out = Vec::new();
    app.run_action(crate::keymap::Action::MarkToday, &mut out);
    for i in out {
        app.apply(i);
    }
    assert!(app.snap.today.is_empty());
}

#[test]
fn dragging_a_gantt_bar_reschedules_through_the_engine() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.update",
        &[
            ("task", "API-2"),
            ("start", "2026-03-02"),
            ("due", "2026-03-05"),
        ],
    ));
    let row = app
        .snap
        .rows
        .iter()
        .find(|r| r.key == "API-2")
        .unwrap()
        .clone();
    app.apply(crate::views::gantt::reschedule(&row, None, 3));
    let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
    assert_eq!(
        crate::views::gantt::span(&row.task),
        Some(("2026-03-05".parse().unwrap(), "2026-03-08".parse().unwrap()))
    );
}

#[test]
fn shortcuts_are_configurable_and_switch_views() {
    let mut app = app_with_tasks();
    app.st.keymap.set(
        crate::keymap::Action::ViewGantt,
        crate::keymap::Shortcut::parse("Ctrl+G"),
    );
    let mut out = Vec::new();
    app.run_action(crate::keymap::Action::ViewGantt, &mut out);
    assert_eq!(app.st.view, ViewKind::Gantt);
    // Se guarda solo lo que cambió.
    assert_eq!(app.st.keymap.to_pref(), "view_gantt=Ctrl+G");
}

/// Las vistas nuevas se dibujan sin ventana (como en `cargo xtask bench`).
#[test]
fn new_views_render_headless() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.update",
        &[
            ("task", "API-1"),
            ("start", "2026-03-02"),
            ("due", "2026-03-05"),
        ],
    ));
    app.apply(Intent::run(
        "task.link",
        &[("blocker", "API-1"), ("blocked", "API-2")],
    ));
    app.apply(Intent::run(
        "task.update",
        &[("task", "API-2"), ("due", "2026-03-09")],
    ));
    let ctx = egui::Context::default();
    for view in [ViewKind::Table, ViewKind::Gantt, ViewKind::Today] {
        app.st.view = view;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| app.draw(ui));
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty(), "{view:?}");
        }
    }
}

/// La misma consulta da lo mismo en la app que en `task.list` (CLI y MCP).
#[test]
fn the_app_and_task_list_agree_on_filters() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.label",
        &[("task", "API-3"), ("add", "db")],
    ));
    app.apply(Intent::run(
        "task.link",
        &[("blocker", "API-3"), ("blocked", "API-2")],
    ));
    for query in [
        "label:db",
        "blocked:true",
        "parent:API-1 OR -ready:true",
        "NOT (status:todo) OR title~padre",
        "created>=today-1d priority:medium",
    ] {
        app.st.filter.text = query.into();
        let mut local: Vec<String> = crate::state::flat_rows(&app.snap, &app.st, false)
            .iter()
            .map(|r| r.key.clone())
            .collect();
        let remote = app
            .conn
            .dispatch("task.list", &Args::none().with("filter", query), Lang::Es)
            .unwrap();
        let mut remote: Vec<String> = remote["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["key"].as_str().unwrap().to_owned())
            .collect();
        local.sort();
        remote.sort();
        assert_eq!(local, remote, "{query}");
    }
}

/// M11 (D15): la interfaz recibe el token, sus agentes no; los asignados con
/// rol se ven en el detalle y la vista Agentes se agrupa por interfaz.
#[test]
fn interfaces_and_role_assignments_from_the_app() {
    let mut app = app_with_tasks();
    app.apply(Intent::IssueToken {
        id: "interface.register",
        args: Args::none()
            .with("name", "claude-code")
            .with("kind", "claude-code"),
    });
    let Some(Dialog::Token { interface, token }) = &app.st.dialog else {
        panic!("el token se muestra al crearla");
    };
    assert_eq!(interface, "claude-code");
    assert!(token.starts_with("tc_"));
    app.st.dialog = None;
    for name in ["cc-dev", "cc-rev"] {
        app.apply(Intent::run(
            "agent.register",
            &[("name", name), ("interface", "claude-code")],
        ));
    }
    assert_eq!(app.snap.interfaces.len(), 1);
    assert!(
        app.snap
            .agents
            .iter()
            .all(|r| r.agent.interface_id.is_some())
    );

    for agent in ["cc-dev", "cc-rev"] {
        app.apply(Intent::run(
            "task.add_assignee",
            &[("task", "API-2"), ("agent", agent)],
        ));
    }
    let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
    assert_eq!(row.assignees.len(), 2, "varios asignados, sin papeles");

    app.st.view = ViewKind::Agents;
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
    out.textures_delta.clear();
    assert!(!out.shapes.is_empty());
}

/// Dibuja un frame de 1400×900 con estos eventos de teclado.
fn frame(app: &mut TaskCraftApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.draw(ui));
    output.textures_delta.clear();
}

fn enter() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// La caja de subtarea de la lista: se escribe, Enter la crea bajo su padre
/// y la caja se cierra; Escape la cierra sin crear nada.
#[test]
fn list_subtask_box_creates_on_enter() {
    let mut app = app_with_tasks();
    app.st.view = ViewKind::List;
    let parent = app.snap.rows[2].id();
    assert_eq!(app.snap.rows[2].key, "API-2");
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    app.st.add_child = Some((parent, String::new()));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![egui::Event::Text("nueva hija".into())]);
    frame(&mut app, &ctx, vec![enter()]);
    frame(&mut app, &ctx, vec![]);
    assert!(app.st.add_child.is_none(), "la caja se cierra");
    let child = app
        .snap
        .rows
        .iter()
        .find(|r| r.task.title == "nueva hija")
        .expect("subtarea creada");
    assert_eq!(child.task.parent_id, Some(parent));

    app.st.add_child = Some((parent, String::new()));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![egui::Event::Text("no".into())]);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    frame(&mut app, &ctx, vec![]);
    assert!(app.st.add_child.is_none(), "Escape la cierra");
    assert!(!app.snap.rows.iter().any(|r| r.task.title == "no"));
}

/// Las demás cajas en su sitio terminan igual con Enter: celda de la tabla,
/// tarea rápida del calendario y subtarea del detalle (con el foco que deja
/// el menú contextual).
#[test]
fn inline_boxes_submit_on_enter() {
    let mut app = app_with_tasks();
    let ctx = egui::Context::default();
    let api2 = app.snap.rows[2].id();

    app.st.view = ViewKind::Table;
    frame(&mut app, &ctx, vec![]);
    app.st.table.editing = Some((api2, "title".into(), "renombrada".into()));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![enter()]);
    frame(&mut app, &ctx, vec![]);
    assert!(app.st.table.editing.is_none());
    assert!(app.snap.rows.iter().any(|r| r.task.title == "renombrada"));

    app.st.view = ViewKind::Calendar;
    let day = app.st.calendar_month;
    frame(&mut app, &ctx, vec![]);
    app.st.quick_add = Some((day, String::new()));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![egui::Event::Text("del día".into())]);
    frame(&mut app, &ctx, vec![enter()]);
    frame(&mut app, &ctx, vec![]);
    assert!(app.st.quick_add.is_none());
    let row = app.snap.rows.iter().find(|r| r.task.title == "del día");
    assert_eq!(
        row.and_then(|r| r.task.due.as_ref()).map(|d| d.date()),
        Some(day)
    );

    app.st.view = ViewKind::Board;
    app.apply(Intent::Select(Some(api2)));
    app.st.focus_subtask = true;
    frame(&mut app, &ctx, vec![]);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("desde el detalle".into())],
    );
    frame(&mut app, &ctx, vec![enter()]);
    frame(&mut app, &ctx, vec![]);
    let child = app
        .snap
        .rows
        .iter()
        .find(|r| r.task.title == "desde el detalle")
        .expect("subtarea creada desde el detalle");
    assert_eq!(child.task.parent_id, Some(api2));
}

/// M12: el detalle trae los vínculos con git y se dibuja con ellos; el coste
/// ya no se muestra.
#[test]
fn detail_shows_git_links() {
    let mut app = app_with_tasks();
    let sha = "0123456789abcdef0123456789abcdef01234567";
    app.apply(Intent::run(
        "git.link",
        &[
            ("task", "API-1"),
            ("kind", "commit"),
            ("ref", sha),
            ("title", "arreglo"),
        ],
    ));
    let api1 = app.snap.rows[0].id();
    app.apply(Intent::Select(Some(api1)));
    let d = app.detail.as_ref().unwrap();
    assert_eq!(d.git.len(), 1);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
}

/// RF-MOD-18: el editor del detalle produce la regla y la deshace.
#[test]
fn the_recurrence_editor_round_trips() {
    let mut app = app_with_tasks();
    let mut edit = crate::state::RecurrenceEdit::of(None);
    assert!(edit.args().is_none(), "sin regla");
    edit.mode = 1;
    edit.unit = model::RepeatUnit::Week;
    edit.weekdays[0] = true;
    edit.weekdays[3] = true;
    let mut pairs: Vec<(&str, String)> = vec![("task", "API-2".into())];
    pairs.extend(edit.args().unwrap());
    let args: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    app.apply(Intent::run("task.set_recurrence", &args));
    let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
    let rule = row.task.recurrence.clone().unwrap();
    assert_eq!(crate::state::RecurrenceEdit::of(Some(&rule)), edit);
    assert_eq!(
        rules::recurrence::describe(&rule, Lang::Es),
        "Cada semana (lun, jue)"
    );
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
    app.apply(Intent::run("task.clear_recurrence", &[("task", "API-2")]));
    let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
    assert!(row.task.recurrence.is_none());
}

/// RF-VIS-16: aparcar una tarea crea la columna «Algún día» con su nombre
/// traducido.
#[test]
fn someday_becomes_a_named_column() {
    let mut app = app_with_tasks();
    app.apply(Intent::run("task.someday", &[("task", "API-2")]));
    let project = app.snap.rows[0].task.project_id;
    let names: Vec<String> = crate::flow::columns(&app.snap, Some(project), Lang::Es)
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&"Algún día".to_owned()), "{names:?}");
    let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
    assert_eq!(row.task.stage.as_deref(), Some("someday"));
    assert!(!row.stale);
}

/// RF-VIS-15 y RF-REL-05: la matriz y el grafo se dibujan con datos, y sus
/// atajos llevan a ellos.
#[test]
fn matrix_and_graph_views_draw() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "task.update",
        &[("task", "API-1"), ("priority", "urgent")],
    ));
    app.apply(Intent::run(
        "task.link",
        &[("blocker", "API-1"), ("blocked", "API-2")],
    ));
    let ctx = egui::Context::default();
    for view in [ViewKind::Matrix, ViewKind::Graph] {
        app.st.view = view;
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![]);
    }
    assert_eq!(
        ViewKind::from_code("matrix"),
        Some(ViewKind::Matrix),
        "las vistas guardadas la reconocen"
    );
    assert!(ViewKind::TASK_VIEWS.contains(&ViewKind::Graph));
}

/// RF-INT-05: los ciclos llegan al snapshot, el filtro `cycle:current`
/// funciona en la app y su panel se dibuja.
#[test]
fn cycles_reach_the_app() {
    let mut app = app_with_tasks();
    app.apply(Intent::run("cycle.create", &[("project", "API")]));
    app.apply(Intent::run(
        "task.set_cycle",
        &[("task", "API-2"), ("cycle", "current")],
    ));
    assert_eq!(app.snap.cycles.len(), 1);
    app.st.filter.text = "cycle:current".into();
    let keys: Vec<String> = crate::state::flat_rows(&app.snap, &app.st, false)
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert_eq!(keys, ["API-2"]);
    app.st.filter.text.clear();
    app.st.view = ViewKind::Epics;
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let api2 = app
        .snap
        .rows
        .iter()
        .find(|r| r.key == "API-2")
        .unwrap()
        .id();
    app.apply(Intent::Select(Some(api2)));
    app.st.view = ViewKind::List;
    frame(&mut app, &ctx, vec![]);
}

/// La app con un ciclo en API (API-2 dentro) y una vista guardada del proyecto.
fn app_with_cycle() -> TaskCraftApp {
    let mut app = app_with_tasks();
    run(&mut app, "cycle.create", &[("project", "API")]);
    run(
        &mut app,
        "task.set_cycle",
        &[("task", "API-2"), ("cycle", "current")],
    );
    run(
        &mut app,
        "view.save",
        &[("name", "Mías"), ("filter", "padre"), ("project", "API")],
    );
    app
}

fn sidebar_harness(app: TaskCraftApp) -> egui_kittest::Harness<'static, TaskCraftApp> {
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1440.0, 860.0))
        .with_max_steps(8)
        .with_accessibility_check(false)
        .build_ui_state(|ui, app: &mut TaskCraftApp| app.draw(ui), app)
}

#[test]
fn the_snapshot_counts_open_tasks_and_cycle_progress() {
    let mut app = app_with_cycle();
    let Scope::Project(api) = app.st.scope else {
        panic!("dentro del proyecto");
    };
    assert_eq!(app.snap.open_counts.get(&api), Some(&3));
    let cycle = app.snap.open_cycles(api).next().unwrap().clone();
    assert_eq!(app.snap.cycle_progress(cycle.id), (0, 1));
    for s in ["in_progress", "done"] {
        run(&mut app, "task.move", &[("task", "API-2"), ("status", s)]);
    }
    assert_eq!(app.snap.cycle_progress(cycle.id), (1, 1));
    assert_eq!(app.snap.open_counts.get(&api), Some(&2));
}

#[test]
fn the_sidebar_opens_a_project_and_goes_to_its_cycle() {
    let mut app = app_with_cycle();
    app.st.view = ViewKind::Agents;
    let mut h = sidebar_harness(app);
    h.run();
    // El proyecto elegido está desplegado, con sus ciclos a la vista.
    h.get_by_label("Sprint 1").click();
    h.run();
    let app = h.state();
    assert!(app.st.filter.cycle.is_some());
    assert_eq!(
        app.st.view,
        ViewKind::Board,
        "fuera de las tareas, el tablero"
    );
    let rows = crate::state::list_rows(&app.snap, &app.st);
    let keys: Vec<&str> = rows.iter().map(|(r, _)| r.key.as_str()).collect();
    assert_eq!(keys, ["API-2"]);
    // «Tareas» quita el filtro de ciclo y se queda en la vista.
    h.get_by_label("Tareas").click();
    h.run();
    assert!(h.state().st.filter.cycle.is_none());
    assert_eq!(h.state().st.view, ViewKind::Board);
    // Las vistas del proyecto empiezan plegadas.
    assert!(h.query_by_label("Mías").is_none());
    h.get_by_label("Vistas").click();
    h.run();
    h.get_by_label("Mías").click();
    h.run();
    assert_eq!(h.state().st.active_view.as_deref(), Some("Mías"));
}

/// RF-INT-05: una tarea en varios sprints; el detalle los muestra y su menú
/// suma otro sin quitar el que tenía.
#[test]
fn the_detail_plans_a_task_in_several_sprints() {
    let mut app = app_with_cycle();
    run(&mut app, "cycle.create", &[("project", "API")]);
    let api2 = app
        .snap
        .rows
        .iter()
        .find(|r| r.key == "API-2")
        .unwrap()
        .id();
    app.apply(Intent::Select(Some(api2)));
    let mut h = sidebar_harness(app);
    h.run();
    h.get_by_label("Planificar en sprints…").click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Sprint 2")
        .click();
    h.run();
    let names = |app: &TaskCraftApp| -> Vec<String> {
        let row = app.snap.rows.iter().find(|r| r.key == "API-2").unwrap();
        row.task
            .cycles
            .iter()
            .filter_map(|id| app.snap.cycles.iter().find(|c| c.id == *id))
            .map(|c| c.name.clone())
            .collect()
    };
    assert_eq!(names(h.state()), ["Sprint 1", "Sprint 2"]);
    // Los dos filtros de ciclo la encuentran.
    let app = h.state_mut();
    for c in app.snap.cycles.clone() {
        app.st.filter.cycle = Some(c.id);
        let rows = crate::state::list_rows(&app.snap, &app.st);
        assert!(rows.iter().any(|(r, _)| r.key == "API-2"), "{}", c.name);
    }
}

#[test]
fn going_to_another_project_reloads_and_clears_the_cycle() {
    let mut app = app_with_cycle();
    run(&mut app, "project.create", &[("key", "WEB")]);
    let web = app
        .snap
        .projects
        .iter()
        .find(|p| p.key == "WEB")
        .unwrap()
        .id;
    let cycle = app.snap.cycles[0].clone();
    app.go_to(GoTo::cycle(&cycle));
    assert_eq!(app.st.filter.cycle, Some(cycle.id));
    app.go_to(GoTo::project(Scope::Project(web)));
    assert_eq!(app.st.scope, Scope::Project(web));
    assert!(app.st.filter.cycle.is_none());
    assert!(app.snap.rows.is_empty(), "se recargó con el otro proyecto");
}

#[test]
fn the_cycle_dialog_creates_the_next_sprint_and_edits_it() {
    let app = app_with_cycle();
    let Scope::Project(api) = app.st.scope else {
        panic!("dentro del proyecto");
    };
    let form = crate::state::CycleForm::after_last(&app.snap, api, today());
    assert_eq!(form.name, "Sprint 2");
    let first = &app.snap.cycles[0];
    assert_eq!(
        form.start,
        crate::widgets::date_picker::add_days(first.end, 1)
    );
    assert_eq!(
        (form.end - form.start).get_days(),
        (first.end - first.start).get_days()
    );
    let mut h = sidebar_harness(app);
    h.state_mut().st.dialog = Some(Dialog::Cycle(form));
    h.run();
    h.get_by_label("Guardar").click();
    h.run();
    let names: Vec<String> = h
        .state()
        .snap
        .cycles
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(names, ["Sprint 1", "Sprint 2"]);
    assert!(h.state().st.dialog.is_none());

    let mut edit = crate::state::CycleForm::edit(&h.state().snap.cycles[1]);
    edit.name = "Sprint de cierre".into();
    h.state_mut().st.dialog = Some(Dialog::Cycle(edit));
    h.run();
    h.get_by_label("Guardar").click();
    h.run();
    assert_eq!(h.state().snap.cycles[1].name, "Sprint de cierre");
}

#[test]
fn cycle_names_count_up() {
    use crate::state::next_cycle_name;
    assert_eq!(next_cycle_name("Sprint 9"), "Sprint 10");
    assert_eq!(next_cycle_name("Ciclo"), "Ciclo 2");
    assert_eq!(next_cycle_name("Q4-2026"), "Q4-2027");
}

/// RF-VIS-17: los menús por tema llevan a lo mismo que los atajos.
#[test]
fn the_menu_bar_is_organised_by_topic_and_uses_the_shortcuts() {
    let mut h = sidebar_harness(app_with_tasks());
    h.run();
    for menu in [
        "Archivo",
        "Ver",
        "Proyecto",
        "Agentes",
        "Información",
        "Workspace",
        "Preferencias",
        "Ayuda",
    ] {
        // La barra de menús va primero; algunos nombres se repiten en la lateral.
        assert!(h.get_all_by_label(menu).next().is_some(), "{menu}");
    }
    h.get_all_by_label("Ver").next().unwrap().click();
    h.run();
    // La opción del menú lleva su atajo; la pestaña de la vista, no.
    h.get_by_label_contains("Ctrl+2").click();
    h.run();
    assert_eq!(h.state().st.view, ViewKind::Board);

    h.get_all_by_label("Agentes").next().unwrap().click();
    h.run();
    h.get_by_label("Nueva interfaz").click();
    h.run();
    assert!(matches!(h.state().st.dialog, Some(Dialog::Interface(_))));
}

/// D23: «Información → Eliminar» borra un proyecto solo tras escribir su
/// clave, guarda antes una copia y vuelve a todos los proyectos.
#[test]
fn deleting_a_project_from_the_data_menu_saves_a_copy_first() {
    use crate::state::{DeleteForm, DeleteWhat};
    let home = std::env::temp_dir().join(format!("taskcraft-ui-{}", model::AgentId::generate()));
    let mut engine = Engine::new(SqliteStore::open_in_memory().unwrap());
    engine.set_home(&home);
    let sup = Context::supervisor();
    engine
        .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
        .unwrap();
    engine
        .dispatch(
            &sup,
            "task.create",
            &Args::none().with("project", "API").with("title", "x"),
        )
        .unwrap();
    let mut app = TaskCraftApp::new(Conn::local(engine), |k| {
        (k == prefs::LANG).then(|| "es".to_owned())
    });
    assert!(matches!(app.st.scope, Scope::Project(_)));

    let mut form = DeleteForm::new(DeleteWhat::Project, Some("API".into()));
    assert!(!form.ready(&app.snap), "sin escribir la clave no se puede");
    form.confirm = "api".into();
    assert!(form.ready(&app.snap));
    app.st.dialog = Some(Dialog::DeleteData(form.clone()));
    app.apply(form.intent());
    assert!(app.st.dialog.is_none(), "{:?}", app.message);
    assert!(app.snap.projects.is_empty());
    assert_eq!(app.st.scope, Scope::All);
    let message = app.message.clone().unwrap();
    assert!(message.contains("antes-de-borrar-"), "{message}");
    assert!(
        std::fs::read_dir(home.join("backups"))
            .unwrap()
            .flatten()
            .any(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with("antes-de-borrar-"))
    );
}

/// TC-1: la vista Diagramas tiene un visor con zoom; «+» amplía y «Ajustar»
/// vuelve a encajarlo.
#[test]
fn the_diagram_viewer_zooms_with_its_buttons() {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "Flujo"),
            (
                "source",
                "flowchart LR\n  A[Inicio] --> B[Medio] --> C[Fin]",
            ),
        ],
    ));
    app.apply(Intent::OpenDiagram("API-D1".into()));
    let mut h = sidebar_harness(app);
    h.run();
    // El porcentaje del visor: por etiqueta o por valor según el widget.
    let labels = |h: &egui_kittest::Harness<'_, TaskCraftApp>| -> Vec<String> {
        let by_label = h
            .query_all(egui_kittest::kittest::By::new().label_contains(" %"))
            .filter_map(|n| egui_kittest::kittest::NodeT::accesskit_node(&n).label());
        let by_value = h
            .query_all(egui_kittest::kittest::By::new())
            .filter_map(|n| n.value())
            .filter(|v| v.ends_with(" %"));
        by_label.chain(by_value).collect()
    };
    // Un diagrama pequeño encaja a su tamaño natural: 100 %.
    assert!(!labels(&h).iter().any(|l| l == "125 %"), "{:?}", labels(&h));
    h.get_by_label("+").click();
    h.run();
    assert!(labels(&h).iter().any(|l| l == "125 %"), "{:?}", labels(&h));
    h.get_by_label("Ajustar").click();
    h.run();
    assert!(!labels(&h).iter().any(|l| l == "125 %"), "{:?}", labels(&h));
}

/// TC-2: sobre el visor, la rueda (o dos dedos en vertical) amplía y
/// reduce, y el pellizco y Ctrl+rueda también amplían.
#[test]
fn the_wheel_and_pinch_zoom_the_diagram_viewer() {
    let mut h = sidebar_harness(app_with_diagram());
    h.run();
    let start = viewer_zoom(&h);
    let up = viewer_gesture(&mut h, wheel(120.0, egui::Modifiers::NONE));
    assert!(up > start, "la rueda amplía: {start} → {up}");
    let down = viewer_gesture(&mut h, wheel(-240.0, egui::Modifiers::NONE));
    assert!(down < up, "y reduce: {up} → {down}");
    let pinch = viewer_gesture(&mut h, egui::Event::Zoom(1.5));
    assert!(pinch > down, "el pellizco amplía: {down} → {pinch}");
    let ctrl = viewer_gesture(&mut h, wheel(120.0, egui::Modifiers::CTRL));
    assert!(ctrl > pinch, "Ctrl+rueda amplía: {pinch} → {ctrl}");
    let shift = viewer_gesture(&mut h, wheel(120.0, egui::Modifiers::SHIFT));
    assert_eq!(shift, ctrl, "Shift+rueda desplaza sin ampliar");
}

/// TC-2: en el editor la vista previa va dentro de una página con scroll; la
/// rueda sigue siendo de la página y solo Ctrl+rueda (o el pellizco) amplía.
#[test]
fn the_wheel_scrolls_the_editor_instead_of_zooming_its_preview() {
    let mut app = app_with_diagram();
    app.st.diagrams.editor = app
        .snap
        .diagram_by_key("API-D1")
        .map(|d| crate::state::DiagramEditor::edit(&d.diagram));
    let mut h = sidebar_harness(app);
    h.run();
    let start = viewer_zoom(&h);
    let wheel_only = viewer_gesture(&mut h, wheel(120.0, egui::Modifiers::NONE));
    assert_eq!(wheel_only, start, "la rueda no amplía la vista previa");
    let ctrl = viewer_gesture(&mut h, wheel(120.0, egui::Modifiers::CTRL));
    assert!(ctrl > start, "Ctrl+rueda sí: {start} → {ctrl}");
}

fn app_with_diagram() -> TaskCraftApp {
    let mut app = app_with_tasks();
    app.apply(Intent::run(
        "diagram.create",
        &[
            ("project", "API"),
            ("title", "F"),
            (
                "source",
                "flowchart LR\n  A[Inicio] --> B[Medio] --> C[Fin]",
            ),
        ],
    ));
    app.apply(Intent::OpenDiagram("API-D1".into()));
    app
}

/// El zoom que muestra la etiqueta «N %» de la barra del visor.
fn zoom_label(n: &egui_kittest::Node<'_>) -> Option<f32> {
    n.value()?.strip_suffix(" %")?.trim().parse().ok()
}

fn viewer_zoom_node<'a>(h: &'a egui_kittest::Harness<'_, TaskCraftApp>) -> egui_kittest::Node<'a> {
    h.query_all(egui_kittest::kittest::By::new())
        .find(|n| zoom_label(n).is_some())
        .expect("etiqueta de zoom")
}

fn viewer_zoom(h: &egui_kittest::Harness<'_, TaskCraftApp>) -> f32 {
    zoom_label(&viewer_zoom_node(h)).unwrap_or(0.0)
}

fn wheel(dy: f32, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers,
    }
}

/// Envía `event` con el cursor sobre el lienzo, justo bajo la barra del
/// visor, y devuelve el zoom resultante.
fn viewer_gesture(h: &mut egui_kittest::Harness<'_, TaskCraftApp>, event: egui::Event) -> f32 {
    let bar = viewer_zoom_node(h).rect();
    h.hover_at(egui::pos2(bar.center().x + 40.0, bar.max.y + 60.0));
    h.run();
    h.event(event);
    h.run();
    viewer_zoom(h)
}

/// TC-6: guion con traspasos a la columna de un agente y una devolución.
fn app_with_flow() -> TaskCraftApp {
    let mut app = app_with_tasks();
    run(
        &mut app,
        "interface.register",
        &[("name", "Claude Code"), ("kind", "claude-code")],
    );
    run(
        &mut app,
        "agent.register",
        &[("name", "dev"), ("interface", "Claude Code")],
    );
    run(
        &mut app,
        "status.create",
        &[
            ("project", "API"),
            ("name", "Cola de dev"),
            ("base", "todo"),
            ("owner", "dev"),
        ],
    );
    for (task, status) in [
        ("API-1", "cola_de_dev"),
        ("API-1", "in_progress"),
        ("API-1", "cola_de_dev"),
        ("API-1", "in_progress"),
        ("API-1", "done"),
        ("API-2", "in_progress"),
        ("API-2", "done"),
    ] {
        run(&mut app, "task.move", &[("task", task), ("status", status)]);
    }
    app.st.view = ViewKind::Metrics;
    app
}

fn metrics_harness(app: TaskCraftApp) -> egui_kittest::Harness<'static, TaskCraftApp> {
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1440.0, 1400.0))
        .with_max_steps(8)
        .with_accessibility_check(false)
        .build_ui_state(|ui, app: &mut TaskCraftApp| app.draw(ui), app)
}

/// TC-6: las tarjetas muestran las cifras del flujo, también con «Todos»,
/// y la vista ya no tiene coste.
#[test]
fn metrics_cards_show_the_flow_figures() {
    let mut app = app_with_flow();
    app.st.scope = Scope::All;
    app.apply(Intent::LoadMetrics);
    let data = app.st.metrics.data.clone().unwrap();
    assert!(data.errors.is_empty(), "{:?}", data.errors);
    let summary = &data.flow["summary"];
    assert_eq!(summary["done"], 2);
    assert_eq!(summary["returns"], 1, "API-1 volvió a la cola de dev");
    assert_eq!(summary["returned_pct"].as_f64(), Some(50.0));
    assert!(
        !data.burndown["series"].as_array().unwrap().is_empty(),
        "con Todos, burndown agregado"
    );
    assert!(!data.cfd["series"].as_array().unwrap().is_empty());
    let dev = data.flow["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["actor_name"] == "dev")
        .cloned();
    assert_eq!(dev.map(|a| a["returns_received"].clone()), Some(1.into()));

    let mut h = metrics_harness(app);
    h.run();
    // Cada tarjeta se anuncia como «título: valor».
    for label in [
        "Hechas: 2",
        "Con devolución: 50 %",
        "1 devolución",
        "Cola de dev",
    ] {
        assert!(h.query_all_by_label(label).count() > 0, "falta «{label}»");
    }
    assert!(
        h.query_by_label_contains("devoluciones").is_none(),
        "singular"
    );
    assert!(h.query_by_label_contains("Coste").is_none());
    assert!(h.query_by_label_contains("Elige un proyecto").is_none());
}

/// Sin tareas hechas: las tarjetas que no se pueden calcular dicen «—».
#[test]
fn metrics_without_data_show_dashes() {
    let mut app = app_with_tasks();
    app.st.view = ViewKind::Metrics;
    app.apply(Intent::LoadMetrics);
    let mut h = metrics_harness(app);
    h.run();
    for label in ["Hechas: 0", "Lead time (p50): —", "Cycle time (p50): —"] {
        assert!(h.query_by_label(label).is_some(), "falta «{label}»");
    }
    assert!(h.query_by_label_contains("Coste").is_none());
}

/// Al pasar de un proyecto a «Todos» se recalcula con el agregado, y al
/// volver a la vista desde otra, también.
#[test]
fn metrics_reload_on_scope_change_and_on_entering() {
    let app = app_with_flow();
    let Scope::Project(api) = app.st.scope else {
        panic!("dentro del proyecto");
    };
    let mut h = metrics_harness(app);
    h.run();
    let data =
        |h: &egui_kittest::Harness<'_, TaskCraftApp>| h.state().st.metrics.data.clone().unwrap();
    assert_eq!(data(&h).burndown["project"], "API");
    assert_eq!(h.state().st.metrics.scope, Some(Scope::Project(api)));

    h.state_mut().go_to(GoTo::project(Scope::All));
    h.state_mut().st.view = ViewKind::Metrics;
    h.run();
    assert_eq!(h.state().st.metrics.scope, Some(Scope::All));
    assert!(data(&h).burndown["project"].is_null(), "agregado");

    // Fuera de la vista se hace otra tarea; al volver, la cuenta está al día.
    h.state_mut().st.view = ViewKind::List;
    h.run();
    for s in ["in_progress", "done"] {
        h.state_mut().apply(Intent::run(
            "task.move",
            &[("task", "API-3"), ("status", s)],
        ));
    }
    h.state_mut().st.view = ViewKind::Metrics;
    h.run();
    assert_eq!(data(&h).flow["summary"]["done"], 3);
}

/// `task.timeline` con campos ausentes o `null`: lo que falta queda vacío.
#[test]
fn the_journey_tolerates_nulls() {
    let j = crate::data::Journey::from_json(&serde_json::json!({
        "done_at": null,
        "total_minutes": null,
        "steps": [
            {"status": "todo", "column": null, "stage": "cola_de_dev", "entered_at": null,
             "left_at": null, "minutes": null, "actor": "agent:x", "actor_name": null},
            {"status": "nada", "entered_at": "no es una fecha"},
            {}
        ]
    }));
    assert_eq!(j.steps.len(), 3);
    let first = &j.steps[0];
    assert_eq!(first.column, "cola_de_dev");
    assert_eq!(first.status, Some(model::Status::Todo));
    assert_eq!(first.actor, "agent:x");
    assert!(first.entered_at.is_none() && first.minutes.is_none());
    assert!(j.steps[1].status.is_none() && j.steps[1].entered_at.is_none());
    assert!(j.done_at.is_none() && j.closed_at.is_none() && j.total_minutes.is_none());
    assert!(
        crate::data::Journey::from_json(&serde_json::Value::Null)
            .steps
            .is_empty()
    );
}

/// TC-6: el recorrido del detalle lista los tramos y cuándo se completó.
#[test]
fn the_detail_shows_the_journey() {
    let mut app = app_with_flow();
    app.st.view = ViewKind::List;
    app.open_task("API-1");
    assert_eq!(
        app.token,
        app.conn.change_token(),
        "pedir el recorrido no deja una recarga pendiente"
    );
    let journey = &app.detail.as_ref().unwrap().journey;
    let columns: Vec<&str> = journey.steps.iter().map(|s| s.column.as_str()).collect();
    assert!(columns.contains(&"Cola de dev"), "{columns:?}");
    assert_eq!(
        columns.iter().filter(|c| **c == "Cola de dev").count(),
        2,
        "dos estancias en la cola"
    );
    assert!(journey.done_at.is_some());
    assert!(journey.steps.last().is_some_and(|s| s.left_at.is_none()));

    let mut h = metrics_harness(app);
    h.run();
    assert_eq!(h.query_all_by_label("Cola de dev").count(), 2);
    assert!(h.query_by_label_contains("Completada el").is_some());
    assert!(
        h.query_by_label_contains("– ahora").is_none(),
        "hecha: el último tramo no sigue «ahora»"
    );
    // Sin hacer: el tramo actual llega hasta «ahora» y no hay fecha de cierre.
    h.state_mut().open_task("API-3");
    h.run();
    assert!(h.query_by_label_contains("– ahora").is_some());
    assert!(h.query_by_label_contains("Completada el").is_none());
}

/// Una tarea cancelada no sigue «en curso»: su cierre es la cancelación.
#[test]
fn a_cancelled_task_ends_its_journey() {
    let mut app = app_with_flow();
    run(
        &mut app,
        "task.move",
        &[("task", "API-3"), ("status", "cancelled")],
    );
    app.st.view = ViewKind::List;
    app.open_task("API-3");
    let mut h = metrics_harness(app);
    h.run();
    assert!(h.query_by_label_contains("Cancelada el").is_some());
    assert!(h.query_by_label_contains("Completada el").is_none());
    assert!(h.query_by_label_contains("– ahora").is_none());
}

/// Con el detalle abierto y sin daemon, pedir el recorrido (una lectura) no
/// deja una recarga pendiente; una escritura de otro agente, en cambio, se
/// detecta y se recarga.
#[test]
fn reading_does_not_reload_but_others_writes_do() {
    let mut app = app_with_flow();
    app.open_task("API-2");
    assert_eq!(app.token, app.conn.change_token(), "sin recarga pendiente");
    let force_poll = |app: &mut TaskCraftApp| {
        app.last_poll = Instant::now()
            .checked_sub(POLL * 2)
            .unwrap_or_else(Instant::now);
        app.poll();
    };
    force_poll(&mut app);
    assert_eq!(app.token, app.conn.change_token());

    // Otro agente escribe sin pasar por la app.
    app.conn
        .dispatch(
            "task.update",
            &Args::none()
                .with("task", "API-2")
                .with("title", "cambiada fuera")
                .with("as_agent", "dev"),
            app.st.lang,
        )
        .unwrap();
    assert_ne!(app.token, app.conn.change_token(), "queda pendiente");
    force_poll(&mut app);
    let d = app.detail.as_ref().unwrap();
    assert_eq!(d.row.task.title, "cambiada fuera");
    assert_eq!(app.token, app.conn.change_token(), "y no vuelve a recargar");
}
