use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, ThemePreference};
use engine::{Args, Value};
use i18n::{Lang, Localize, Msg, t, tr};
use model::{Action, EntityKind, ProjectStatus, Status};

use crate::conn::{self, Conn};
use crate::data::{self, ActivityLine, Detail, Scope, Snapshot};
use crate::keymap::{self, Keymap, Shortcut};
use crate::state::{
    AssigneeFilter, Dialog, Intent, LabelsForm, Lanes, Palette, PaletteCommand, ProjectForm,
    SettingsForm, UiState, ViewKind,
};
use crate::theme::TextSize;
use crate::widgets::date_picker::today;
use crate::widgets::icons::{self, Icon};
use crate::widgets::markdown::Markdown;
use crate::{detail, dialogs, hierarchy, layout, palette, theme, views};

/// Cada cuánto se comprueba si otro proceso (CLI, agentes) cambió los datos.
const POLL: Duration = Duration::from_millis(500);
/// Cuánto dura el aviso "Deshacer" tras eliminar algo (D07).
const UNDO_WINDOW: Duration = Duration::from_secs(10);
/// Variable de entorno con la vista inicial (`list`, `board`, `calendar`…).
pub const VIEW_ENV: &str = "TASKCRAFT_VIEW";

/// Preferencias de la interfaz que se recuerdan entre sesiones.
pub mod prefs {
    pub const LANG: &str = "taskcraft.lang";
    pub const THEME: &str = "taskcraft.theme";
    pub const VIEW: &str = "taskcraft.view";
    pub const LANES: &str = "taskcraft.lanes";
    pub const ROOTS_ONLY: &str = "taskcraft.roots_only";
    pub const ACTIVITY: &str = "taskcraft.activity";
    pub const WEEK: &str = "taskcraft.calendar_week";
    pub const TEXT_SIZE: &str = "taskcraft.text_size";
    /// Cuándo cerró el supervisor la app por última vez (RF-SUP-09).
    pub const LAST_VISIT: &str = "taskcraft.last_visit";
    /// Avisos también como notificaciones del sistema (RF-SUP-10).
    pub const OS_NOTIFY: &str = "taskcraft.os_notify";
    /// Columnas de la tabla (RF-VIS-05).
    pub const TABLE_COLUMNS: &str = "taskcraft.table_columns";
    /// Atajos cambiados respecto a los predeterminados (RF-VIS-14).
    pub const SHORTCUTS: &str = "taskcraft.shortcuts";
    /// Barra lateral plegada a iconos.
    pub const SIDEBAR_COLLAPSED: &str = "taskcraft.sidebar_collapsed";
}

/// Cuánto se ve un aviso de revisión o escalado (RF-SUP-10).
const NOTICE_WINDOW: Duration = Duration::from_secs(8);

/// Aviso de algo que pide al supervisor: una entrega o una pregunta.
struct Notice {
    text: String,
    task: model::TaskId,
    expires: Instant,
}

/// Aviso temporal con un botón para revertir la última eliminación (D07).
struct UndoToast {
    message: Msg,
    inverse: (&'static str, Args),
    expires: Instant,
}

/// Abre otro workspace en una ventana nueva; `Err` con el motivo.
pub type OpenWorkspace = Box<dyn Fn(&str) -> Result<(), String>>;

/// Cambio de workspace (RF-ALM-08): lo aporta `main`, que sabe dónde viven
/// y cómo abrir otro.
pub struct Workspaces {
    pub current: String,
    pub list: Box<dyn Fn() -> Vec<String>>,
    pub open: OpenWorkspace,
}

pub struct TaskCraftApp {
    conn: Conn,
    workspaces: Option<Workspaces>,
    /// Cerrar la ventana en el próximo frame (tras abrir otro workspace).
    close: bool,
    st: UiState,
    snap: Snapshot,
    detail: Option<Detail>,
    token: Option<u64>,
    last_poll: Instant,
    message: Result<String, String>,
    toast: Option<UndoToast>,
    md: Markdown,
    /// Texto para el portapapeles en el próximo frame (Copiar token, D22).
    clipboard: Option<String>,
    applied_theme: Option<ThemePreference>,
    applied_text: Option<TextSize>,
    /// Tareas en revisión o escaladas ya avisadas.
    seen_review: std::collections::HashSet<model::TaskId>,
    notices: Vec<Notice>,
    /// Avisar también con notificaciones del sistema (`notify-send`).
    os_notify: bool,
    /// Cargar en este hilo aunque haya hilo de carga.
    sync_reload: bool,
}

fn theme_code(t: ThemePreference) -> &'static str {
    match t {
        ThemePreference::Dark => "dark",
        ThemePreference::Light => "light",
        ThemePreference::System => "system",
    }
}

impl TaskCraftApp {
    /// `saved`: preferencia guardada por clave (ver [`prefs`]).
    pub fn new(conn: Conn, saved: impl Fn(&str) -> Option<String>) -> Self {
        let forced_lang = std::env::var(i18n::LANG_ENV)
            .ok()
            .and_then(|v| Lang::from_tag(&v));
        let lang = forced_lang
            .or_else(|| saved(prefs::LANG).as_deref().and_then(Lang::from_tag))
            .unwrap_or_else(Lang::detect);
        let theme = match saved(prefs::THEME).as_deref() {
            Some("dark") => ThemePreference::Dark,
            Some("light") => ThemePreference::Light,
            _ => ThemePreference::System,
        };
        let view = std::env::var(VIEW_ENV)
            .ok()
            .as_deref()
            .and_then(ViewKind::from_code)
            .or_else(|| saved(prefs::VIEW).as_deref().and_then(ViewKind::from_code))
            .unwrap_or(ViewKind::List);
        let st = UiState {
            lang,
            theme,
            text_size: saved(prefs::TEXT_SIZE)
                .as_deref()
                .and_then(TextSize::from_code)
                .unwrap_or_default(),
            scope: Scope::All,
            view,
            lanes: saved(prefs::LANES)
                .as_deref()
                .and_then(Lanes::from_code)
                .unwrap_or(Lanes::None),
            roots_only: saved(prefs::ROOTS_ONLY).as_deref() == Some("true"),
            show_archived: false,
            show_activity: saved(prefs::ACTIVITY).as_deref() != Some("false"),
            activity_filter: Default::default(),
            filter: Default::default(),
            collapsed: Default::default(),
            selected: None,
            calendar_month: today(),
            calendar_week: saved(prefs::WEEK).as_deref() == Some("true"),
            new_task: String::new(),
            focus_new_task: false,
            add_child: None,
            graph_closed: false,
            focus_subtask: false,
            parent_query: String::new(),
            quick_add: None,
            edit: Default::default(),
            dialog: None,
            dialog_error: None,
            palette: None,
            rejecting: None,
            answers: Default::default(),
            inbox_target: Default::default(),
            epic_form: Default::default(),
            diagrams: Default::default(),
            metrics: Default::default(),
            active_view: None,
            table: match saved(prefs::TABLE_COLUMNS) {
                Some(cols) if !cols.trim().is_empty() => crate::state::TableState {
                    columns: cols.split(',').map(str::to_owned).collect(),
                    editing: None,
                },
                _ => Default::default(),
            },
            gantt: Default::default(),
            keymap: saved(prefs::SHORTCUTS)
                .map(|s| Keymap::from_pref(&s))
                .unwrap_or_default(),
            capturing: None,
            focus_search: false,
            panels: crate::state::PanelsState {
                sidebar_collapsed: saved(prefs::SIDEBAR_COLLAPSED).as_deref() == Some("true"),
                ..Default::default()
            },
        };
        let mut app = Self {
            conn,
            workspaces: None,
            close: false,
            st,
            snap: Snapshot::default(),
            detail: None,
            token: None,
            last_poll: Instant::now(),
            message: Ok(t(lang, "ui-ready")),
            toast: None,
            md: Markdown::default(),
            clipboard: None,
            applied_theme: None,
            applied_text: None,
            seen_review: Default::default(),
            notices: Vec::new(),
            os_notify: false,
            sync_reload: false,
        };
        // La primera carga es síncrona: sin datos no se puede elegir dónde empezar.
        app.reload_now();
        // Con un solo proyecto activo, empezar dentro de él.
        let only = match app
            .snap
            .projects
            .iter()
            .filter(|p| p.status == ProjectStatus::Active)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [only] => Some(only.id),
            _ => None,
        };
        if let Some(id) = only {
            app.st.scope = Scope::Project(id);
            app.reload_now();
        }
        // Qué hicieron los agentes desde la última vez que se cerró la app.
        if let Some(since) = saved(prefs::LAST_VISIT)
            && let Ok(v) = app.conn.dispatch(
                "activity.summary",
                &Args::none().with("since", since),
                app.st.lang,
            )
            && crate::dialogs_project::summary_has_news(&v)
        {
            app.st.dialog = Some(Dialog::Summary(v));
        }
        app.os_notify = saved(prefs::OS_NOTIFY).as_deref() == Some("true");
        app.seen_review = app.review_ids();
        app
    }

    /// Bandera que el hilo de eventos del daemon pone a `true` (RNF-08).
    pub fn notifier(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.conn.notifier()
    }

    /// Qué hacer para despertar la ventana cuando llega una réplica nueva.
    pub fn set_waker(&self, wake: Box<dyn Fn() + Send>) {
        self.conn.set_waker(wake);
    }

    /// Dibuja los diagramas en un hilo aparte para que el zoom no congele la
    /// ventana (TC-3). Sin llamarlo (tests), se dibujan en el fotograma.
    pub fn draw_diagrams_in_background(&mut self) {
        self.md.diagrams.in_background();
    }

    /// Permite cambiar de workspace desde el menú.
    pub fn set_workspaces(&mut self, workspaces: Workspaces) {
        self.workspaces = Some(workspaces);
    }

    /// Mensaje de la barra de estado (avisos al arrancar).
    pub fn set_status(&mut self, message: Result<String, String>) {
        self.message = message;
    }

    /// Abre la app con una tarea seleccionada (`taskcraft API-3`).
    pub fn open_task(&mut self, reference: &str) {
        let found = self
            .conn
            .read(|r| engine::lookup::task(r, reference))
            .map(|t| (t.id, t.project_id));
        match found {
            Ok((id, project)) => {
                self.st.scope = Scope::Project(project);
                self.st.selected = Some(id);
                self.reload();
            }
            Err(e) => self.message = Err(e.localize(self.st.lang)),
        }
    }

    /// Abre un diagrama por su clave (`API-D3`) en la vista Diagramas.
    pub fn open_diagram(&mut self, key: &str) {
        let found = self
            .conn
            .read(|r| engine::lookup::diagram(r, key, None))
            .map(|d| (d.id, d.project_id));
        match found {
            Ok((id, project)) => self.show_diagrams(project, Some(id)),
            Err(e) => self.message = Err(e.localize(self.st.lang)),
        }
    }

    /// Vista Diagramas de un proyecto, con uno elegido o el que ya lo estaba.
    fn show_diagrams(&mut self, project: model::ProjectId, diagram: Option<model::DiagramId>) {
        self.st.view = ViewKind::Diagrams;
        self.st.selected = None;
        self.st.panels.sidebar_peek = false;
        if diagram.is_some() {
            self.st.diagrams.selected = diagram;
            self.st.diagrams.editor = None;
        }
        if self.st.scope != Scope::Project(project) {
            self.st.scope = Scope::Project(project);
        }
        self.reload();
    }

    fn t(&self, key: &'static str) -> String {
        t(self.st.lang, key)
    }

    /// Recarga sin el hilo de carga (arranque y benchmarks).
    fn reload_now(&mut self) {
        self.sync_reload = true;
        self.reload();
        self.sync_reload = false;
    }

    fn reload(&mut self) {
        self.token = self.conn.change_token();
        let opts = data::Load {
            scope: self.st.scope,
            include_archived: self.st.show_archived,
            activity_limit: 40,
            activity_filter: self.st.activity_filter.clone(),
        };
        // Con hilo de carga, la ventana sigue con la réplica anterior hasta
        // que llega la nueva (`poll`); sin él, se carga aquí mismo.
        if self.sync_reload || !self.conn.request_snapshot(&opts) {
            match data::load(&mut self.conn, &opts) {
                Ok(snap) => {
                    self.snap = snap;
                    self.refresh_search();
                    self.notify_new_reviews();
                }
                Err(e) => self.message = Err(e.localize(self.st.lang)),
            }
        }
        self.detail = match self.st.selected {
            Some(id) => data::load_detail(&mut self.conn, id, self.st.lang)
                .ok()
                .flatten(),
            None => None,
        };
        match &self.detail {
            Some(d) if d.row.task.deleted_at.is_none() => detail::sync(&mut self.st, d),
            _ => {
                self.detail = None;
                self.st.selected = None;
            }
        }
    }

    /// Pide las métricas del alcance actual (RF-INT-06, RF-HIS-05). Sin
    /// proyecto, el motor agrega todos los del alcance.
    fn load_metrics(&mut self) {
        let lang = self.st.lang;
        let days = self.st.metrics.days.to_string();
        let project = match self.st.scope {
            Scope::Project(id) => self.snap.project(id).map(|p| p.key.clone()),
            Scope::All => None,
        };
        let args = |extra: Args| {
            let extra = extra.with("days", days.as_str());
            match &project {
                Some(key) => extra.with("project", key.as_str()),
                None => extra,
            }
        };
        let mut errors = Vec::new();
        let mut get = |id: &str, args: Args| match self.conn.dispatch(id, &args, lang) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("{id}: {e}"));
                serde_json::Value::Null
            }
        };
        let burndown = get("metrics.burndown", args(Args::none()));
        let cfd = get("metrics.cfd", args(Args::none()));
        let flow = get("metrics.flow", args(Args::none()));
        let sheet = get(
            "time.sheet",
            args(Args::none().with("period", self.st.metrics.period)),
        );
        self.st.metrics.data = Some(crate::state::MetricsData {
            burndown,
            cfd,
            flow,
            sheet,
            errors,
        });
        self.st.metrics.scope = Some(self.st.scope);
    }

    fn review_ids(&self) -> std::collections::HashSet<model::TaskId> {
        self.snap.review.iter().map(|r| r.id()).collect()
    }

    /// Avisa de lo que acaba de entrar en revisión o de escalarse (RF-SUP-10).
    fn notify_new_reviews(&mut self) {
        let lang = self.st.lang;
        let now = self.review_ids();
        for row in self
            .snap
            .review
            .iter()
            .filter(|r| !self.seen_review.contains(&r.id()))
        {
            let who = row
                .assignee
                .as_ref()
                .map(|w| w.name.clone())
                .unwrap_or_default();
            // Sin agente asignado, el aviso no lo nombra.
            let (question_key, review_key) = if who.is_empty() {
                ("ui-notice-question-plain", "ui-notice-review-plain")
            } else {
                ("ui-notice-question", "ui-notice-review")
            };
            let text = match &row.task.needs_human {
                Some(question) => tr(
                    lang,
                    &Msg::new(question_key)
                        .arg("task", row.key.as_str())
                        .arg("agent", who.as_str())
                        .arg("question", question.as_str()),
                ),
                None => tr(
                    lang,
                    &Msg::new(review_key)
                        .arg("task", row.key.as_str())
                        .arg("agent", who.as_str()),
                ),
            };
            if self.os_notify {
                // Opcional y sin dependencias: si no hay `notify-send`, no pasa nada.
                let _ = std::process::Command::new("notify-send")
                    .args(["TaskCraft", &text])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            self.notices.push(Notice {
                text,
                task: row.id(),
                expires: Instant::now() + NOTICE_WINDOW,
            });
        }
        self.seen_review = now;
    }

    fn show_notices(&mut self, ctx: &egui::Context, out: &mut Vec<Intent>) {
        self.notices.retain(|n| Instant::now() < n.expires);
        if self.notices.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("taskcraft_notices"))
            // Abajo a la derecha: no tapan las cabeceras de los paneles.
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -40.0])
            .show(ctx, |ui| {
                for n in &self.notices {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(320.0);
                        if ui
                            .add(egui::Label::new(&n.text).sense(egui::Sense::click()))
                            .clicked()
                        {
                            out.push(Intent::Select(Some(n.task)));
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    /// Vuelve a buscar si cambió el texto del filtro (o tras recargar).
    fn refresh_search(&mut self) {
        let text = self.st.filter.text.trim().to_owned();
        if self.snap.search.0 != text || !text.is_empty() {
            let hits = if self.st.filter.structured() {
                std::collections::HashSet::new()
            } else {
                data::search(&mut self.conn, &text)
            };
            self.snap.search = (text, hits);
        }
    }

    fn poll(&mut self) {
        match self.conn.take_snapshot() {
            Some(Ok(snap)) => {
                self.snap = snap;
                self.refresh_search();
                self.notify_new_reviews();
            }
            Some(Err(e)) => self.message = Err(e),
            None => {}
        }
        // Un evento del daemon llega al instante; el token es el respaldo.
        let notified = self.conn.take_notification();
        let due = self.last_poll.elapsed() >= POLL;
        if due {
            self.last_poll = Instant::now();
        }
        if notified || (due && self.conn.change_token() != self.token) {
            self.reload();
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|t| Instant::now() >= t.expires)
        {
            self.toast = None;
        }
    }

    fn run(&mut self, id: &str, args: &Args) -> Option<Value> {
        match self.conn.dispatch(id, args, self.st.lang) {
            Ok(v) => {
                self.message = Ok(tr(self.st.lang, &Msg::new("ui-command-ok").arg("id", id)));
                Some(v)
            }
            Err(e) => {
                self.message = Err(e.message);
                None
            }
        }
    }

    /// Guardado desde un diálogo: el error queda dentro del diálogo abierto.
    fn submit(&mut self, id: &str, args: &Args) -> Option<Value> {
        let result = self.run(id, args);
        self.st.dialog_error = match &result {
            Some(_) => None,
            None => self.message.clone().err(),
        };
        if let Some(v) = &result {
            // Al eliminar información (D23) se dice dónde quedó la copia.
            if let Some(path) = v.get("backup").and_then(Value::as_str) {
                self.message = Ok(tr(
                    self.st.lang,
                    &Msg::new("ui-delete-done").arg("path", path),
                ));
            }
            // Si se borró lo que se estaba viendo, vuelta a todos los proyectos.
            if matches!(id, "project.delete" | "workspace.wipe") {
                self.st.scope = Scope::All;
                self.st.selected = None;
                self.reload();
            }
        }
        result
    }

    fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::FilterActivity(filter) => self.st.activity_filter = filter,
            Intent::OpenKey(key) => self.open_task(&key),
            Intent::OpenDiagram(key) => self.open_diagram(&key),
            Intent::Diagrams(project) => self.show_diagrams(project, None),
            Intent::SaveDiagram { id, args } => {
                if let Some(v) = self.run(id, &args) {
                    self.st.diagrams.editor = None;
                    self.st.diagrams.selected = v
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|id| id.parse().ok());
                }
            }
            Intent::OpenWorkspace(name) => {
                if let Some(w) = &self.workspaces {
                    match (w.open)(&name) {
                        Ok(()) => self.close = true,
                        Err(e) => self.message = Err(e),
                    }
                }
            }
            Intent::LoadMetrics => self.load_metrics(),
            Intent::IssueToken { id, args } => {
                if let Some(v) = self.submit(id, &args)
                    && let (Some(interface), Some(token)) = (
                        v.get("name").and_then(Value::as_str),
                        v.get("token").and_then(Value::as_str),
                    )
                {
                    self.st.dialog = Some(Dialog::Token {
                        interface: interface.to_owned(),
                        token: token.to_owned(),
                    });
                }
            }
            Intent::Submit { id, args } => {
                if self.submit(id, &args).is_some() {
                    self.st.dialog = None;
                }
            }
            Intent::CopyToken(interface) => {
                let args = Args::none().with("interface", interface.as_str());
                match self.conn.dispatch("interface.token", &args, self.st.lang) {
                    Ok(v) => {
                        self.clipboard = v.get("token").and_then(Value::as_str).map(str::to_owned);
                        self.message = Ok(tr(
                            self.st.lang,
                            &Msg::new("ui-token-copied").arg("interface", interface.as_str()),
                        ));
                    }
                    Err(e) => self.message = Err(e.message),
                }
            }
            Intent::Exchange { id, args } => {
                let result = self.conn.dispatch(id, &args, self.st.lang);
                if let Some(Dialog::Export(f) | Dialog::Import(f)) = self.st.dialog.as_mut() {
                    f.imported = id == "workspace.import" && result.is_ok();
                    f.result = Some(result.map_err(|e| e.message));
                }
            }
            Intent::Select(id) => {
                self.st.selected = id;
                self.st.edit = Default::default();
            }
            Intent::GoTo(to) => self.go_to(to),
            Intent::Run { id, args } => {
                self.run(id, &args);
            }
            Intent::RunPalette { id, args } => {
                let result = self
                    .conn
                    .dispatch(id, &args, self.st.lang)
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                    .map_err(|e| e.message);
                if let Some(p) = self.st.palette.as_mut() {
                    p.result = Some(result);
                }
            }
            Intent::Delete {
                id,
                args,
                undo,
                message,
            } => {
                if self.run(id, &args).is_some() {
                    self.toast = Some(UndoToast {
                        message,
                        inverse: undo,
                        expires: Instant::now() + UNDO_WINDOW,
                    });
                }
            }
            Intent::Undo => {
                if let Some(toast) = self.toast.take() {
                    let (id, args) = toast.inverse;
                    self.run(id, &args);
                }
            }
            Intent::OpenPalette => self.open_palette(),
            Intent::CreateOnDay { title, day } => match self.target_project() {
                Some(key) => {
                    let args = Args::none()
                        .with("project", key)
                        .with("title", title)
                        .with("due", day.to_string());
                    self.run("task.create", &args);
                }
                None => self.message = Err(self.t("ui-pick-project")),
            },
        }
        self.reload();
    }

    /// Proyecto donde crear tareas: el seleccionado, o el único activo.
    fn target_project(&self) -> Option<String> {
        match self.st.scope {
            Scope::Project(id) => self.snap.project(id).map(|p| p.key.clone()),
            Scope::All => match self
                .snap
                .projects
                .iter()
                .filter(|p| p.status == ProjectStatus::Active)
                .collect::<Vec<_>>()
                .as_slice()
            {
                [only] => Some(only.key.clone()),
                _ => None,
            },
        }
    }

    fn open_palette(&mut self) {
        let mut palette = Palette {
            commands: conn::registry()
                .iter()
                .filter(|c| !c.id().starts_with("test.") && !c.id().starts_with("session."))
                .map(|c| PaletteCommand {
                    id: c.id(),
                    label: c.label(),
                    params: c.params(),
                })
                .collect(),
            ..Palette::default()
        };
        // Rellenar con la selección actual.
        if let Some(d) = &self.detail {
            palette.values.insert("task", d.row.key.clone());
        }
        if let Some(key) = self.target_project() {
            palette.values.insert("project", key);
        }
        self.st.palette = Some(palette);
    }

    fn shortcuts(&mut self, ctx: &egui::Context, out: &mut Vec<Intent>) {
        let typing = ctx.memory(|m| m.focused().is_some());
        if let Some(action) = self.st.capturing {
            self.capture_shortcut(ctx, action);
            return;
        }
        let keys = self.st.keymap.ordered();
        let mut fired = Vec::new();
        ctx.input_mut(|i| {
            // Los configurables (RF-VIS-14), los más específicos primero.
            for (action, shortcut) in keys {
                if i.consume_shortcut(&shortcut.egui()) {
                    fired.push(action);
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                if self.st.dialog.is_some() {
                    self.st.dialog = None;
                } else if self.st.selected.is_some() {
                    out.push(Intent::Select(None));
                }
            }
            if !typing
                && i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                && let Some(d) = &self.detail
            {
                out.push(views::list::delete_intent(&d.row.key));
            }
            // Tab / Mayús+Tab: hacer subtarea de la anterior / subir un nivel.
            if !typing
                && self.st.view == ViewKind::List
                && let Some(row) = self
                    .st
                    .selected
                    .and_then(|id| hierarchy::row(&self.snap, id))
            {
                if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                    if let Some(up) = hierarchy::outdent_target(&self.snap, row.id()) {
                        out.push(hierarchy::set_parent(&row.key, up.as_deref()));
                    }
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                    && let Some(prev) = hierarchy::indent_target(&self.snap, row.id())
                {
                    out.push(hierarchy::set_parent(&row.key, Some(&prev.key)));
                }
            }
        });
        for action in fired {
            self.run_action(action, out);
        }
    }

    /// En el editor de atajos: la siguiente combinación con tecla se asigna
    /// a `action`; Esc cancela.
    fn capture_shortcut(&mut self, ctx: &egui::Context, action: keymap::Action) {
        let pressed = ctx.input_mut(|i| {
            let found = i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            });
            if found.is_some() {
                i.events.clear();
            }
            found
        });
        match pressed {
            Some((egui::Key::Escape, m)) if m.is_none() => self.st.capturing = None,
            Some((key, m)) => {
                self.st.keymap.set(action, Some(Shortcut::pressed(m, key)));
                self.st.capturing = None;
            }
            None => {}
        }
    }

    pub(crate) fn run_action(&mut self, action: keymap::Action, out: &mut Vec<Intent>) {
        use keymap::Action as A;
        let task_view = |st: &mut UiState| {
            if !ViewKind::TASK_VIEWS.contains(&st.view) {
                st.view = ViewKind::List;
            }
        };
        match action {
            A::Palette => out.push(Intent::OpenPalette),
            A::NewTask => {
                self.st.focus_new_task = true;
                task_view(&mut self.st);
            }
            A::Search => {
                self.st.focus_search = true;
                task_view(&mut self.st);
            }
            A::SaveView => {
                self.st.dialog = Some(Dialog::SaveView(
                    self.st.active_view.clone().unwrap_or_default(),
                ));
            }
            A::MarkToday => {
                if let Some(row) = self
                    .st
                    .selected
                    .and_then(|id| hierarchy::row(&self.snap, id))
                {
                    out.push(views::today::toggle(&self.snap, row));
                }
            }
            A::Activity => self.st.show_activity = !self.st.show_activity,
            A::ViewList => self.st.view = ViewKind::List,
            A::ViewBoard => self.st.view = ViewKind::Board,
            A::ViewCalendar => self.st.view = ViewKind::Calendar,
            A::ViewTable => self.st.view = ViewKind::Table,
            A::ViewGantt => self.st.view = ViewKind::Gantt,
            A::ViewToday => self.st.view = ViewKind::Today,
            A::ViewMatrix => self.st.view = ViewKind::Matrix,
            A::ViewGraph => self.st.view = ViewKind::Graph,
        }
    }

    /// Opción de menú con su atajo configurado: hace lo mismo que el atajo.
    fn action_item(
        &mut self,
        ui: &mut egui::Ui,
        action: keymap::Action,
        label: String,
        selected: bool,
        out: &mut Vec<Intent>,
    ) {
        let mut button = egui::Button::selectable(selected, label);
        if let Some(s) = self.st.keymap.get(action) {
            button = button.shortcut_text(s.text());
        }
        if ui.add(button).clicked() {
            self.run_action(action, out);
            ui.close();
        }
    }

    /// Barra de menús por tema (RF-VIS-17): Archivo · Ver · Proyecto ·
    /// Agentes · Información · Workspace · Preferencias · Ayuda.
    fn menu(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        egui::MenuBar::new().ui(ui, |ui| {
            self.menu_file(ui, out);
            self.menu_view(ui, out);
            self.menu_project(ui, out);
            self.menu_agents(ui, out);
            self.menu_data(ui, out);
            self.menu_workspace(ui);
            self.menu_preferences(ui);
            self.menu_help(ui);
        });
    }

    /// Gestión de la información (D23): exportar, importar, copias y eliminar.
    fn menu_data(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        use crate::state::{DeleteForm, DeleteWhat};
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-data"), |ui| {
            if ui.button(t(lang, "ui-export")).clicked() {
                self.st.dialog = Some(Dialog::Export(crate::state::ExchangeForm::suggested(
                    today(),
                )));
                ui.close();
            }
            if ui.button(t(lang, "ui-import")).clicked() {
                self.st.dialog = Some(Dialog::Import(crate::state::ExchangeForm::suggested(
                    today(),
                )));
                ui.close();
            }
            if ui.button(t(lang, "ui-backup-now")).clicked() {
                out.push(Intent::run("workspace.backup", &[]));
                ui.close();
            }
            ui.separator();
            ui.menu_button(t(lang, "ui-delete-data"), |ui| {
                let current = match self.st.scope {
                    Scope::Project(id) => self.snap.project(id).map(|p| p.key.clone()),
                    Scope::All => None,
                };
                for what in DeleteWhat::ALL {
                    if ui.button(t(lang, what.label_key())).clicked() {
                        let project = (what == DeleteWhat::Project)
                            .then(|| current.clone())
                            .flatten();
                        self.st.dialog = Some(Dialog::DeleteData(DeleteForm::new(what, project)));
                        ui.close();
                    }
                }
                ui.separator();
                ui.weak(t(lang, "ui-delete-backup-first"));
            });
        });
    }

    fn menu_file(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-file"), |ui| {
            if ui.button(t(lang, "ui-new-project")).clicked() {
                self.st.dialog = Some(Dialog::Project(ProjectForm::default()));
                ui.close();
            }
            self.action_item(
                ui,
                keymap::Action::NewTask,
                t(lang, "ui-new-task"),
                false,
                out,
            );
            ui.separator();
            if let Some(w) = &self.workspaces
                && ui.button(t(lang, "ui-workspaces")).clicked()
            {
                self.st.dialog = Some(Dialog::Workspaces(crate::state::WorkspaceForm {
                    current: w.current.clone(),
                    names: (w.list)(),
                    new_name: String::new(),
                }));
                ui.close();
            }
            ui.separator();
            if ui.button(t(lang, "ui-quit")).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn menu_view(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        use keymap::Action as A;
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-view"), |ui| {
            for (view, action) in [
                (ViewKind::List, A::ViewList),
                (ViewKind::Board, A::ViewBoard),
                (ViewKind::Calendar, A::ViewCalendar),
                (ViewKind::Table, A::ViewTable),
                (ViewKind::Gantt, A::ViewGantt),
                (ViewKind::Today, A::ViewToday),
                (ViewKind::Matrix, A::ViewMatrix),
                (ViewKind::Graph, A::ViewGraph),
            ] {
                let label = t(lang, view.label_key());
                self.action_item(ui, action, label, self.st.view == view, out);
            }
            ui.separator();
            for view in [
                ViewKind::Review,
                ViewKind::Inbox,
                ViewKind::Cycles,
                ViewKind::Epics,
                ViewKind::Diagrams,
                ViewKind::Metrics,
                ViewKind::Trash,
            ] {
                if ui
                    .selectable_label(self.st.view == view, t(lang, view.label_key()))
                    .clicked()
                {
                    self.st.view = view;
                    self.st.selected = None;
                    ui.close();
                }
            }
            ui.separator();
            let activity = self.st.show_activity;
            self.action_item(ui, A::Activity, t(lang, "ui-activity"), activity, out);
            ui.checkbox(&mut self.st.filter.hide_closed, t(lang, "ui-hide-closed"));
            if ui
                .checkbox(&mut self.st.show_archived, t(lang, "ui-show-archived"))
                .changed()
            {
                self.reload();
            }
            ui.separator();
            self.action_item(ui, A::Search, t(lang, "ui-menu-search"), false, out);
            self.action_item(ui, A::Palette, t(lang, "ui-palette"), false, out);
        });
    }

    fn menu_project(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        let project = match self.st.scope {
            Scope::Project(id) => self.snap.project(id).cloned(),
            Scope::All => None,
        };
        ui.menu_button(t(lang, "ui-menu-project"), |ui| {
            let Some(p) = &project else {
                // Sin proyecto elegido: lo que haría, desactivado y con el porqué.
                ui.weak(t(lang, "ui-menu-pick-project"));
                ui.add_enabled_ui(false, |ui| {
                    for key in ["ui-edit-project", "ui-workflow-fields", "ui-templates"] {
                        let _ = ui.button(t(lang, key));
                    }
                });
                return;
            };
            if ui.button(t(lang, "ui-edit-project")).clicked() {
                self.st.dialog = Some(Dialog::Project(ProjectForm::edit(p)));
                ui.close();
            }
            if ui.button(t(lang, "ui-workflow-fields")).clicked() {
                self.st.dialog = Some(Dialog::Workflow(crate::state::WorkflowForm::new(p.id)));
                ui.close();
            }
            if ui.button(t(lang, "ui-templates")).clicked() {
                self.st.dialog = Some(Dialog::Templates(crate::state::TemplateForm {
                    project_key: p.key.clone(),
                    ..Default::default()
                }));
                ui.close();
            }
            ui.separator();
            if ui.button(t(lang, "ui-cycle-new")).clicked() {
                let form = crate::state::CycleForm::after_last(&self.snap, p.id, today());
                self.st.dialog = Some(Dialog::Cycle(form));
                ui.close();
            }
            if ui.button(t(lang, "ui-epic-new")).clicked() {
                // La vista de épicas tiene el alta, con el proyecto ya elegido.
                self.st.epic_form.project = Some(p.id);
                self.st.view = ViewKind::Epics;
                self.st.selected = None;
                ui.close();
            }
            if ui.button(t(lang, "ui-diagram-new")).clicked() {
                self.st.diagrams.editor = Some(crate::state::DiagramEditor::new(p.id));
                self.st.view = ViewKind::Diagrams;
                self.st.selected = None;
                ui.close();
            }
            ui.separator();
            let (lock_key, lock_cmd) = if p.locked {
                ("ui-unlock", "lock.clear")
            } else {
                ("ui-lock", "lock.set")
            };
            if ui.button(t(lang, lock_key)).clicked() {
                out.push(Intent::run(lock_cmd, &[("project", &p.key)]));
                ui.close();
            }
            let (arch_key, arch_cmd) = if p.status == ProjectStatus::Archived {
                ("ui-unarchive", "project.unarchive")
            } else {
                ("ui-archive", "project.archive")
            };
            if ui.button(t(lang, arch_key)).clicked() {
                out.push(Intent::run(arch_cmd, &[("project", &p.key)]));
                ui.close();
            }
        });
    }

    /// Interfaces y agentes (D15, D22).
    fn menu_agents(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-agents"), |ui| {
            if ui.button(t(lang, "ui-interface-register")).clicked() {
                self.st.dialog = Some(Dialog::Interface(Default::default()));
                ui.close();
            }
            if ui
                .selectable_label(
                    self.st.view == ViewKind::Agents,
                    t(lang, "ui-menu-interfaces-agents"),
                )
                .clicked()
            {
                self.st.view = ViewKind::Agents;
                self.st.selected = None;
                ui.close();
            }
            ui.separator();
            // Solo lectura para todos los agentes (RF-SUP-08).
            let read_only = self
                .snap
                .workspace
                .as_ref()
                .is_some_and(|w| w.settings.read_only);
            let mut on = read_only;
            if ui.checkbox(&mut on, t(lang, "ui-read-only")).changed() {
                out.push(Intent::run(
                    "workspace.configure",
                    &[("read_only", if on { "true" } else { "false" })],
                ));
                ui.close();
            }
        });
    }

    fn menu_workspace(&mut self, ui: &mut egui::Ui) {
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-workspace"), |ui| {
            if ui.button(t(lang, "ui-settings")).clicked() {
                if let Some(ws) = &self.snap.workspace {
                    let s = ws.settings;
                    self.st.dialog = Some(Dialog::Settings(SettingsForm {
                        max_depth: s.max_depth,
                        trash_days: s.trash_days,
                        lease_minutes: s.lease_minutes,
                        agent_wip_limit: s.agent_wip_limit,
                        auto_archive_days: s.auto_archive_days,
                        backup_hours: s.backup_hours,
                        backup_keep: s.backup_keep,
                    }));
                }
                ui.close();
            }
            if ui.button(t(lang, "ui-labels")).clicked() {
                self.st.dialog = Some(Dialog::Labels(LabelsForm {
                    rows: self
                        .snap
                        .labels
                        .iter()
                        .map(|l| {
                            let c = theme::parse_hex(&l.color);
                            (l.name.clone(), l.name.clone(), [c.r(), c.g(), c.b()])
                        })
                        .collect(),
                    new_color: [0x00, 0x90, 0xff],
                    ..LabelsForm::default()
                }));
                ui.close();
            }
        });
    }

    /// Lo que es de esta app y no del workspace: idioma, aspecto, avisos y atajos.
    fn menu_preferences(&mut self, ui: &mut egui::Ui) {
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-preferences"), |ui| {
            ui.menu_button(t(lang, "ui-language"), |ui| {
                for l in Lang::ALL {
                    if ui
                        .selectable_label(self.st.lang == l, l.native_name())
                        .clicked()
                    {
                        self.st.lang = l;
                        ui.close();
                    }
                }
            });
            ui.menu_button(t(lang, "ui-theme"), |ui| {
                for (pref, key) in [
                    (ThemePreference::System, "ui-theme-system"),
                    (ThemePreference::Light, "ui-theme-light"),
                    (ThemePreference::Dark, "ui-theme-dark"),
                ] {
                    if ui
                        .selectable_label(self.st.theme == pref, t(lang, key))
                        .clicked()
                    {
                        self.st.theme = pref;
                        ui.close();
                    }
                }
            });
            ui.menu_button(t(lang, "ui-text-size"), |ui| {
                for size in TextSize::ALL {
                    if ui
                        .selectable_label(self.st.text_size == size, t(lang, size.label_key()))
                        .clicked()
                    {
                        self.st.text_size = size;
                        ui.close();
                    }
                }
            });
            ui.checkbox(&mut self.os_notify, t(lang, "ui-os-notify"));
            ui.separator();
            if ui.button(t(lang, "ui-shortcuts")).clicked() {
                self.st.dialog = Some(Dialog::Shortcuts);
                ui.close();
            }
        });
    }

    fn menu_help(&mut self, ui: &mut egui::Ui) {
        let lang = self.st.lang;
        ui.menu_button(t(lang, "ui-menu-help"), |ui| {
            if ui.button(t(lang, "ui-shortcuts")).clicked() {
                self.st.dialog = Some(Dialog::Shortcuts);
                ui.close();
            }
            ui.separator();
            ui.menu_button(t(lang, "ui-about"), |ui| {
                ui.strong(format!("TaskCraft {}", env!("CARGO_PKG_VERSION")));
                ui.add(egui::Label::new(t(lang, "ui-about-text")).wrap());
            });
        });
    }

    /// La actividad encima de la vista, cuando no cabe como panel.
    fn activity_overlay(
        &mut self,
        ctx: &egui::Context,
        area: egui::Rect,
        was_open: bool,
        out: &mut Vec<Intent>,
    ) {
        if !self.st.panels.activity_peek {
            return;
        }
        let scale = self.st.text_size.scale();
        let mut close = false;
        let shown = egui::Area::new(egui::Id::unique("taskcraft-activity-peek"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(area.right_top())
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(layout::ACTIVITY * scale);
                    ui.set_height((area.height() - 16.0).max(120.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        close = icons::button(ui, Icon::Close, true, &self.t("ui-close")).clicked();
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            self.activity_panel(ui, out);
                        });
                    });
                });
            });
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if close || escape || (was_open && shown.response.clicked_elsewhere()) {
            self.st.panels.activity_peek = false;
        }
    }

    /// Mensaje del último comando a la izquierda; a la derecha, agentes
    /// conectados y el interruptor de la actividad.
    fn status_bar(&mut self, ui: &mut egui::Ui, width: f32, plan: layout::Plan) {
        let lang = self.st.lang;
        let fits = layout::plan(
            width,
            self.st.text_size.scale(),
            self.st.panels.sidebar_collapsed,
            true,
            self.detail.is_some(),
        )
        .activity_side;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let on = plan.activity_side || self.st.panels.activity_peek;
            if ui
                .selectable_label(on, t(lang, "ui-activity"))
                .on_hover_text(t(lang, "ui-show-activity"))
                .clicked()
            {
                if fits {
                    self.st.show_activity = !plan.activity_side;
                    self.st.panels.activity_peek = false;
                } else {
                    self.st.panels.activity_peek = !self.st.panels.activity_peek;
                }
            }
            ui.weak(tr(
                lang,
                &Msg::new("ui-agents-connected")
                    .arg("n", self.snap.agents_connected())
                    .arg("total", self.snap.agents.len()),
            ));
            if self
                .snap
                .workspace
                .as_ref()
                .is_some_and(|w| w.settings.read_only)
            {
                ui.colored_label(theme::ORANGE, self.t("ui-read-only-on"));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let text = match &self.message {
                    Ok(msg) => RichText::new(msg).weak(),
                    Err(msg) => RichText::new(msg).color(ui.visuals().error_fg_color),
                };
                ui.add(egui::Label::new(text).truncate());
            });
        });
    }

    fn task_header(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        let title = match self.st.scope {
            Scope::All => t(lang, "ui-all-projects"),
            Scope::Project(id) => self
                .snap
                .project(id)
                .map(|p| format!("{} · {}", p.key, p.name))
                .unwrap_or_default(),
        };
        let compact = ui.available_width() < layout::COMPACT_CENTER * self.st.text_size.scale();
        // Título y vistas en una línea: con sus nombres si caben; si no, iconos.
        let names = ViewKind::TASK_VIEWS.map(|v| t(lang, v.label_key()));
        let named: f32 = names.iter().map(|n| icons::tab_width(ui, Some(n))).sum();
        let bare = icons::tab_width(ui, None) * names.len() as f32;
        let avail = ui.available_width();
        let with_names = avail - named >= 200.0;
        let tabs_w = if with_names { named } else { bare };
        ui.horizontal(|ui| {
            let title_w = (avail - tabs_w - 12.0).max(60.0);
            let h = ui.text_style_height(&egui::TextStyle::Heading);
            ui.allocate_ui_with_layout(
                egui::vec2(title_w, h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(title_w);
                    ui.add(egui::Label::new(RichText::new(&title).heading()).truncate());
                },
            );
            for (v, name) in ViewKind::TASK_VIEWS.into_iter().zip(&names) {
                let label = with_names.then_some(name.as_str());
                if icons::tab(ui, view_icon(v), label, self.st.view == v, name).clicked() {
                    self.st.view = v;
                }
            }
        });

        // Opciones de la vista y vistas guardadas.
        ui.horizontal_wrapped(|ui| {
            let options = match self.st.view {
                ViewKind::Board => {
                    egui::ComboBox::from_id_salt("lanes")
                        .selected_text(t(lang, self.st.lanes.label_key()))
                        .show_ui(ui, |ui| {
                            for l in Lanes::ALL {
                                ui.selectable_value(&mut self.st.lanes, l, t(lang, l.label_key()));
                            }
                        });
                    ui.checkbox(&mut self.st.roots_only, t(lang, "ui-roots-only"));
                    // Columnas nuevas, también la cola o la mesa de un agente (D20).
                    if let Scope::Project(project) = self.st.scope
                        && ui
                            .button(format!("+ {}", t(lang, "ui-add-column")))
                            .on_hover_text(t(lang, "ui-add-column-hint"))
                            .clicked()
                    {
                        self.st.dialog =
                            Some(Dialog::Workflow(crate::state::WorkflowForm::new(project)));
                    }
                    true
                }
                ViewKind::Calendar => {
                    views::calendar::toolbar(ui, &mut self.st);
                    true
                }
                ViewKind::Table => {
                    views::table::toolbar(ui, &self.snap, &mut self.st);
                    true
                }
                ViewKind::Gantt => {
                    views::gantt::toolbar(ui, &mut self.st);
                    true
                }
                ViewKind::Graph => {
                    views::graph::toolbar(ui, &mut self.st);
                    true
                }
                _ => false,
            };
            if options {
                ui.separator();
            }
            self.saved_views(ui, out);
        });

        // Nueva tarea, búsqueda y filtros (plegados tras un botón si no caben).
        let project = self.target_project();
        let avail = ui.available_width();
        let (new_w, search_w) = if compact {
            let w = ((avail - 150.0) / 2.0).clamp(110.0, 260.0);
            (w, w)
        } else {
            (260.0, 220.0)
        };
        let filters_inline = !compact;
        let row = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
            if filters_inline {
                ui.horizontal_wrapped(|ui| add(ui));
            } else {
                ui.horizontal(|ui| add(ui));
            }
        };
        row(ui, &mut |ui| {
            // Sin proyecto elegido no se puede crear: en estrecho, ni se muestra.
            if project.is_some() || !compact {
                let hint = if project.is_some() {
                    t(lang, "ui-new-task-hint")
                } else {
                    t(lang, "ui-pick-project")
                };
                let input = ui.add_enabled(
                    project.is_some(),
                    egui::TextEdit::singleline(&mut self.st.new_task)
                        .hint_text(hint)
                        .desired_width(new_w),
                );
                if self.st.focus_new_task {
                    input.request_focus();
                    self.st.focus_new_task = false;
                }
                let enter = input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter
                    && let Some(key) = &project
                    && !self.st.new_task.trim().is_empty()
                {
                    out.push(Intent::run(
                        "task.create",
                        &[("project", key), ("title", self.st.new_task.trim())],
                    ));
                    self.st.new_task.clear();
                }
                ui.separator();
            }
            icons::show(ui, Icon::Search, ui.visuals().weak_text_color());
            let search = ui
                .add(
                    egui::TextEdit::singleline(&mut self.st.filter.text)
                        .hint_text(t(lang, "ui-search-hint"))
                        .desired_width(search_w),
                )
                .on_hover_text(t(lang, "ui-filter-help"));
            if self.st.focus_search {
                search.request_focus();
                self.st.focus_search = false;
            }
            if filters_inline {
                self.filters(ui);
            } else {
                let picked = self.st.filter.picked();
                let label = if picked > 0 {
                    tr(lang, &Msg::new("ui-filters-count").arg("count", picked))
                } else {
                    t(lang, "ui-filters")
                };
                let open = self.st.panels.filters_open;
                if ui.selectable_label(open, label).clicked() {
                    self.st.panels.filters_open = !open;
                }
            }
        });
        if !filters_inline && self.st.panels.filters_open {
            ui.horizontal_wrapped(|ui| self.filters(ui));
        }
        // Filtro D12 mal escrito: se avisa y se busca como texto suelto.
        if let Err(e) = self.st.filter.query() {
            let msg = rules::RuleError::Filter {
                position: e.position,
                expected: e.detail,
            };
            ui.colored_label(
                ui.visuals().error_fg_color,
                i18n::tr(lang, &i18n::Localize::msg(&msg)),
            );
        }
        ui.add_space(4.0);
    }

    /// Selector de vistas guardadas (RF-VIS-12): aplicar, guardar y borrar.
    fn saved_views(&mut self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        let current = self
            .st
            .active_view
            .clone()
            .unwrap_or_else(|| t(lang, "ui-views-none"));
        let mut apply = None;
        egui::ComboBox::from_id_salt("saved-views")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for v in &self.snap.views {
                    let on = self.st.active_view.as_deref() == Some(v.name.as_str());
                    if ui
                        .selectable_label(on, &v.name)
                        .on_hover_text(&v.filter)
                        .clicked()
                    {
                        apply = Some(v.clone());
                    }
                }
                if self.snap.views.is_empty() {
                    ui.weak(t(lang, "ui-views-empty"));
                }
            });
        if let Some(v) = apply {
            self.apply_view(&v);
        }
        if icons::button(ui, Icon::Plus, true, &t(lang, "ui-view-save")).clicked() {
            self.st.dialog = Some(Dialog::SaveView(
                self.st.active_view.clone().unwrap_or_default(),
            ));
        }
        if let Some(name) = self.st.active_view.clone()
            && icons::button(ui, Icon::Trash, true, &t(lang, "ui-view-delete")).clicked()
        {
            out.push(Intent::run("view.delete", &[("view", &name)]));
            self.st.active_view = None;
        }
    }

    /// Lleva la app a una vista guardada: filtro, forma, columnas y proyecto.
    pub(crate) fn apply_view(&mut self, v: &model::SavedView) {
        self.st.filter = crate::state::Filter {
            text: v.filter.clone(),
            hide_closed: self.st.filter.hide_closed,
            ..Default::default()
        };
        if let Some(kind) = ViewKind::from_code(&v.layout)
            && ViewKind::TASK_VIEWS.contains(&kind)
        {
            self.st.view = kind;
        }
        if !v.columns.is_empty() {
            self.st.table.columns = v.columns.clone();
        }
        let scope = v.project_id.map_or(Scope::All, Scope::Project);
        if scope != self.st.scope {
            self.st.scope = scope;
            self.reload();
        }
        self.st.active_view = Some(v.name.clone());
    }

    fn filters(&mut self, ui: &mut egui::Ui) {
        let lang = self.st.lang;
        let snap = &self.snap;
        let f = &mut self.st.filter;
        let status_text = if f.statuses.is_empty() {
            t(lang, "ui-any-status")
        } else {
            f.statuses.len().to_string()
        };
        ui.menu_button(
            format!("{}: {status_text}", t(lang, "ui-col-status")),
            |ui| {
                for &s in Status::ALL {
                    let mut on = f.statuses.contains(&s);
                    if ui.checkbox(&mut on, t(lang, s.label_key())).changed() {
                        if on {
                            f.statuses.insert(s);
                        } else {
                            f.statuses.remove(&s);
                        }
                    }
                }
            },
        );
        egui::ComboBox::from_id_salt("f-prio")
            .selected_text(
                f.priority
                    .map_or_else(|| t(lang, "ui-any-priority"), |p| t(lang, p.label_key())),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.priority, None, t(lang, "ui-any-priority"));
                for &p in model::Priority::ALL {
                    ui.selectable_value(&mut f.priority, Some(p), t(lang, p.label_key()));
                }
            });
        egui::ComboBox::from_id_salt("f-label")
            .selected_text(f.label.clone().unwrap_or_else(|| t(lang, "ui-any-label")))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.label, None, t(lang, "ui-any-label"));
                for l in &snap.labels {
                    ui.selectable_value(&mut f.label, Some(l.name.clone()), &l.name);
                }
            });
        let assignee_text = match &f.assignee {
            None => t(lang, "ui-anyone"),
            Some(AssigneeFilter::Unassigned) => t(lang, "ui-unassigned"),
            Some(AssigneeFilter::Actor(model::Actor::Agent(id))) => snap
                .agents
                .iter()
                .find(|a| a.agent.id == *id)
                .map(|a| a.agent.name.clone())
                .unwrap_or_default(),
            Some(AssigneeFilter::Actor(a)) => a.to_string(),
        };
        egui::ComboBox::from_id_salt("f-assignee")
            .selected_text(assignee_text)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.assignee, None, t(lang, "ui-anyone"));
                ui.selectable_value(
                    &mut f.assignee,
                    Some(AssigneeFilter::Unassigned),
                    t(lang, "ui-unassigned"),
                );
                ui.selectable_value(
                    &mut f.assignee,
                    Some(AssigneeFilter::Actor(model::Actor::Supervisor)),
                    "supervisor",
                );
                for a in &snap.agents {
                    ui.selectable_value(
                        &mut f.assignee,
                        Some(AssigneeFilter::Actor(model::Actor::Agent(a.agent.id))),
                        &a.agent.name,
                    );
                }
            });
        let scope = self.st.scope;
        let groups: Vec<_> = snap
            .epics
            .iter()
            .filter(|e| match scope {
                Scope::Project(id) => e.epic.project_id == id,
                Scope::All => true,
            })
            .collect();
        if !groups.is_empty() || f.epic.is_some() {
            let epic_text = f
                .epic
                .and_then(|id| snap.epic(id))
                .map(|e| e.epic.name.clone())
                .unwrap_or_else(|| t(lang, "ui-any-epic"));
            egui::ComboBox::from_id_salt("f-epic")
                .selected_text(epic_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.epic, None, t(lang, "ui-any-epic"));
                    for g in &groups {
                        ui.selectable_value(&mut f.epic, Some(g.epic.id), &g.epic.name);
                    }
                });
        }
        // Ciclos sin cerrar del alcance; el elegido se ve aunque ya esté cerrado.
        let cycles: Vec<_> = snap
            .cycles
            .iter()
            .filter(|c| {
                c.closed_at.is_none()
                    && match scope {
                        Scope::Project(id) => c.project_id == id,
                        Scope::All => true,
                    }
            })
            .collect();
        if !cycles.is_empty() || f.cycle.is_some() {
            let cycle_text = f
                .cycle
                .and_then(|id| snap.cycles.iter().find(|c| c.id == id))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| t(lang, "ui-any-cycle"));
            egui::ComboBox::from_id_salt("f-cycle")
                .selected_text(cycle_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.cycle, None, t(lang, "ui-any-cycle"));
                    for c in &cycles {
                        let name = match scope {
                            Scope::All => snap.project(c.project_id).map_or_else(
                                || c.name.clone(),
                                |p| format!("{} · {}", p.key, c.name),
                            ),
                            Scope::Project(_) => c.name.clone(),
                        };
                        ui.selectable_value(&mut f.cycle, Some(c.id), name);
                    }
                });
        }
        ui.checkbox(&mut f.hide_closed, t(lang, "ui-hide-closed"));
        if f.is_active() && ui.button(t(lang, "ui-clear-filters")).clicked() {
            *f = crate::state::Filter {
                hide_closed: f.hide_closed,
                ..Default::default()
            };
        }
    }

    fn activity_panel(&self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        ui.heading(self.t("ui-activity"));
        ui.weak(tr(
            lang,
            &Msg::new("ui-activity-count").arg("count", self.snap.activity.len()),
        ));
        self.activity_filters(ui, out);
        ui.add_space(4.0);
        if self.snap.activity.is_empty() {
            ui.weak(self.t("ui-no-activity"));
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("activity")
            .show(ui, |ui| {
                for line in &self.snap.activity {
                    if activity_line_with_revert(ui, lang, line, out)
                        && let Some(task) = line.task
                    {
                        out.push(Intent::Select(Some(task)));
                    }
                    ui.add_space(2.0);
                }
            });
    }

    /// Filtros del feed por agente, proyecto y tipo de evento (RF-SUP-02).
    fn activity_filters(&self, ui: &mut egui::Ui, out: &mut Vec<Intent>) {
        let lang = self.st.lang;
        let mut f = self.st.activity_filter.clone();
        let all = t(lang, "ui-filter-all");
        ui.horizontal_wrapped(|ui| {
            let actor_text = match &f.actor {
                None => t(lang, "ui-filter-actor"),
                Some(model::Actor::Agent(id)) => self
                    .snap
                    .agents
                    .iter()
                    .find(|a| a.agent.id == *id)
                    .map(|a| a.agent.name.clone())
                    .unwrap_or_else(|| id.to_string()),
                Some(other) => other.to_string(),
            };
            egui::ComboBox::from_id_salt("activity_actor")
                .selected_text(actor_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.actor, None, &all);
                    ui.selectable_value(&mut f.actor, Some(model::Actor::Supervisor), "supervisor");
                    ui.selectable_value(&mut f.actor, Some(model::Actor::System), "system");
                    for a in &self.snap.agents {
                        ui.selectable_value(
                            &mut f.actor,
                            Some(model::Actor::Agent(a.agent.id)),
                            &a.agent.name,
                        );
                    }
                });
            let project_text = f
                .project
                .and_then(|id| self.snap.project(id))
                .map(|p| p.key.clone())
                .unwrap_or_else(|| t(lang, "ui-filter-project"));
            egui::ComboBox::from_id_salt("activity_project")
                .selected_text(project_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.project, None, &all);
                    for p in &self.snap.projects {
                        ui.selectable_value(
                            &mut f.project,
                            Some(p.id),
                            format!("{} · {}", p.key, p.name),
                        );
                    }
                });
            let kind_text = f
                .kind
                .map(|k| t(lang, k.label_key()))
                .unwrap_or_else(|| t(lang, "ui-filter-event"));
            egui::ComboBox::from_id_salt("activity_kind")
                .selected_text(kind_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.kind, None, &all);
                    for k in crate::state::EventKind::ALL {
                        ui.selectable_value(&mut f.kind, Some(k), t(lang, k.label_key()));
                    }
                });
            if f.is_active() && ui.button(t(lang, "ui-clear-filters")).clicked() {
                f = Default::default();
            }
        });
        if f != self.st.activity_filter {
            out.push(Intent::FilterActivity(f));
        }
    }

    fn undo_toast(&self, ctx: &egui::Context, out: &mut Vec<Intent>) {
        let Some(toast) = &self.toast else {
            return;
        };
        let left = toast.expires.saturating_duration_since(Instant::now());
        egui::Area::new(egui::Id::unique("taskcraft-undo-toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -36.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr(self.st.lang, &toast.message));
                        if ui.button(self.t("ui-undo")).clicked() {
                            out.push(Intent::Undo);
                        }
                        ui.weak(format!("{}s", left.as_secs() + 1));
                    });
                });
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

/// Una línea del historial con su menú para revertirla (RF-SUP-07).
pub fn activity_line_with_revert(
    ui: &mut egui::Ui,
    lang: Lang,
    line: &ActivityLine,
    out: &mut Vec<Intent>,
) -> bool {
    let clicked = activity_line(ui, lang, line);
    let response = ui.interact(
        ui.min_rect(),
        egui::Id::unique(("activity_line", line.id)),
        egui::Sense::click(),
    );
    response.context_menu(|ui| {
        let id = line.id.to_string();
        if ui.button(t(lang, "ui-revert-entry")).clicked() {
            out.push(Intent::run("activity.revert", &[("entry", &id)]));
            ui.close();
        }
        if ui.button(t(lang, "ui-revert-command")).clicked() {
            out.push(Intent::run(
                "activity.revert",
                &[("entry", &id), ("whole_command", "true")],
            ));
            ui.close();
        }
    });
    clicked
}

/// Una línea del historial; devuelve si se pulsó (para abrir la tarea).
pub fn activity_line(ui: &mut egui::Ui, lang: Lang, line: &ActivityLine) -> bool {
    let key = match (line.action, line.entity) {
        _ if line.command == "system.expire_leases" => "activity-lease-expired",
        (Action::Created, EntityKind::Comment) => "activity-commented",
        (Action::Created, _) => "activity-created",
        (Action::Updated, _) => "activity-updated",
        (Action::Deleted, _) => "activity-deleted",
        (Action::Linked, _) => "activity-linked",
        (Action::Unlinked, _) => "activity-unlinked",
    };
    // Nombre traducido del campo si existe; si no, el técnico.
    let fields: Vec<String> = line
        .fields
        .iter()
        .map(|f| i18n::lookup(lang, &format!("field-{f}")).unwrap_or_else(|| f.clone()))
        .collect();
    let text = tr(
        lang,
        &Msg::new(key)
            .arg("actor", &line.actor.name)
            .arg("target", &line.target)
            .arg("fields", fields.join(", ")),
    );
    let time = line
        .at
        .to_zoned(jiff::tz::TimeZone::system())
        .strftime("%H:%M")
        .to_string();
    let mut clicked = false;
    ui.horizontal_wrapped(|ui| {
        crate::widgets::avatar::avatar(ui, &line.actor.seed, line.actor.color, 14.0);
        ui.weak(time);
        let label = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        clicked = line.task.is_some() && label.clicked();
    });
    clicked
}

impl eframe::App for TaskCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let st = &self.st;
        storage.set_string(prefs::LANG, st.lang.code().to_owned());
        storage.set_string(prefs::THEME, theme_code(st.theme).to_owned());
        storage.set_string(prefs::VIEW, st.view.code().to_owned());
        storage.set_string(prefs::LANES, st.lanes.code().to_owned());
        storage.set_string(prefs::ROOTS_ONLY, st.roots_only.to_string());
        storage.set_string(prefs::ACTIVITY, st.show_activity.to_string());
        storage.set_string(prefs::WEEK, st.calendar_week.to_string());
        storage.set_string(prefs::TEXT_SIZE, st.text_size.code().to_owned());
        storage.set_string(prefs::LAST_VISIT, model::Timestamp::now().to_string());
        storage.set_string(prefs::OS_NOTIFY, self.os_notify.to_string());
        storage.set_string(prefs::TABLE_COLUMNS, st.table.columns.join(","));
        storage.set_string(prefs::SHORTCUTS, st.keymap.to_pref());
        storage.set_string(
            prefs::SIDEBAR_COLLAPSED,
            st.panels.sidebar_collapsed.to_string(),
        );
    }
}

impl TaskCraftApp {
    /// Un frame completo de la interfaz. Separado de `eframe` para poder
    /// medirlo sin ventana (`cargo xtask bench`, RNF-06).
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.applied_theme != Some(self.st.theme) {
            ctx.set_theme(self.st.theme);
            self.applied_theme = Some(self.st.theme);
        }
        if self.applied_text != Some(self.st.text_size) {
            theme::apply_text_size(&ctx, self.st.text_size);
            self.applied_text = Some(self.st.text_size);
        }
        self.poll();
        if self.snap.search.0 != self.st.filter.text.trim() {
            self.refresh_search();
        }
        ctx.request_repaint_after(POLL);
        self.md.prepare(&self.snap, self.st.lang);
        if let Some(text) = self.clipboard.take() {
            ctx.copy_text(text);
        }
        let mut out = Vec::new();
        self.shortcuts(&ctx, &mut out);

        let scale = self.st.text_size.scale();
        let width = ctx.content_rect().width();
        let plan = layout::plan(
            width,
            scale,
            self.st.panels.sidebar_collapsed,
            self.st.show_activity,
            self.detail.is_some(),
        );
        // Lo que estaba abierto encima antes de este frame: solo eso se
        // cierra al pulsar fuera (no el clic que acaba de abrirlo).
        let sidebar_was_open = self.st.panels.sidebar_peek;
        let activity_was_open = self.st.panels.activity_peek;

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui, &mut out));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui, width, plan));
        if plan.sidebar_full {
            self.st.panels.sidebar_peek = false;
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(layout::SIDEBAR * scale)
                .size_range(180.0 * scale..=360.0 * scale)
                .show(ui, |ui| self.sidebar(ui, true, &mut out));
        } else {
            // Si la ventana da para la barra completa, el botón la vuelve a fijar.
            let can_dock = layout::plan(width, scale, false, false, false).sidebar_full;
            egui::Panel::left("rail")
                .resizable(false)
                .exact_size(layout::RAIL * scale)
                .show(ui, |ui| self.rail(ui, can_dock));
        }
        if plan.activity_side {
            self.st.panels.activity_peek = false;
            egui::Panel::right("activity")
                .resizable(true)
                .default_size(layout::ACTIVITY * scale)
                .size_range(220.0 * scale..=420.0 * scale)
                .show(ui, |ui| self.activity_panel(ui, &mut out));
        }
        if let (Some(d), Some(layout::DetailPlace::Side { width, max })) =
            (&self.detail, plan.detail)
        {
            egui::Panel::right("detail")
                .resizable(true)
                .default_size(width)
                .size_range((300.0 * scale).min(max)..=max)
                .show(ui, |ui| {
                    detail::show(ui, d, &self.snap, &mut self.st, &mut self.md, &mut out)
                });
        }
        let central = egui::CentralPanel::default().show(ui, |ui| {
            // Métricas recuerda si se dibujó el fotograma anterior.
            let metrics_shown = std::mem::take(&mut self.st.metrics.shown);
            if let (Some(d), Some(layout::DetailPlace::Full)) = (&self.detail, plan.detail) {
                // Sin sitio al lado: el detalle ocupa la vista, con la vuelta a mano.
                let back = tr(
                    self.st.lang,
                    &Msg::new("ui-back-to").arg("view", t(self.st.lang, self.st.view.label_key())),
                );
                if icons::tab(ui, Icon::Left, Some(&back), false, &back).clicked() {
                    out.push(Intent::Select(None));
                }
                ui.separator();
                detail::show(ui, d, &self.snap, &mut self.st, &mut self.md, &mut out);
                return;
            }
            if ViewKind::TASK_VIEWS.contains(&self.st.view) {
                self.task_header(ui, &mut out);
            } else {
                ui.heading(self.t(self.st.view.label_key()));
                ui.add_space(4.0);
            }
            match self.st.view {
                ViewKind::List => views::list::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Board => views::board::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Calendar => views::calendar::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Table => views::table::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Gantt => views::gantt::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Today => views::today::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Matrix => views::matrix::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Graph => views::graph::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Review => views::review::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Inbox => views::inbox::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Agents => views::agents::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Trash => views::trash::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Cycles => views::cycles::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Epics => views::epics::show(ui, &self.snap, &mut self.st, &mut out),
                ViewKind::Diagrams => {
                    views::diagrams::show(ui, &self.snap, &mut self.st, &mut self.md, &mut out)
                }
                ViewKind::Metrics => {
                    // Al entrar en la vista o cambiar de alcance, se recalculan.
                    if !metrics_shown || self.st.metrics.scope != Some(self.st.scope) {
                        self.st.metrics.data = None;
                    }
                    self.st.metrics.shown = true;
                    views::metrics::show(ui, &self.snap, &mut self.st, &mut out)
                }
            }
        });
        let area = central.response.rect;
        self.sidebar_overlay(&ctx, area, sidebar_was_open, &mut out);
        self.activity_overlay(&ctx, area, activity_was_open, &mut out);
        dialogs::show(&ctx, &self.snap, &mut self.st, &mut out);
        self.intercept_task_links(&ctx, &mut out);
        self.show_notices(&ctx, &mut out);
        self.dropped_files(&ctx, &mut out);
        palette::show(&ctx, &mut self.st, &mut out);
        self.undo_toast(&ctx, &mut out);

        // La mutación ocurre solo aquí, siempre a través del engine.
        for intent in out {
            self.apply(intent);
        }
    }

    /// Los enlaces `taskcraft://task/API-42` y `taskcraft://diagram/API-D3`
    /// abren la tarea o el diagrama en la app, no en el navegador.
    fn intercept_task_links(&self, ctx: &egui::Context, out: &mut Vec<Intent>) {
        use crate::widgets::markdown::{DIAGRAM_LINK, TASK_LINK};
        ctx.output_mut(|o| {
            o.commands.retain(|cmd| match cmd {
                egui::OutputCommand::OpenUrl(url) => {
                    if let Some(key) = url.url.strip_prefix(TASK_LINK) {
                        out.push(Intent::OpenKey(key.to_owned()));
                        false
                    } else if let Some(key) = url.url.strip_prefix(DIAGRAM_LINK) {
                        out.push(Intent::OpenDiagram(key.to_owned()));
                        false
                    } else {
                        true
                    }
                }
                _ => true,
            });
        });
    }

    /// Archivos soltados sobre la ventana con una tarea abierta: se adjuntan (RF-MOD-17).
    fn dropped_files(&self, ctx: &egui::Context, out: &mut Vec<Intent>) {
        let Some(detail) = &self.detail else {
            return;
        };
        let paths: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_string_lossy().into_owned())
                .filter(|p| !p.is_empty())
                .collect()
        });
        for path in paths {
            out.push(Intent::run(
                "attachment.add",
                &[("task", &detail.row.key), ("path", &path)],
            ));
        }
    }

    /// Cambia de vista y de alcance (benchmarks y `taskcraft API-3`).
    pub fn show_board_of(&mut self, project_key: &str) {
        if let Some(p) = self.snap.projects.iter().find(|p| p.key == project_key) {
            self.st.scope = Scope::Project(p.id);
            self.st.view = ViewKind::Board;
            self.reload_now();
        }
    }

    /// Lista de todos los proyectos (benchmarks).
    pub fn show_all_list(&mut self) {
        self.st.scope = Scope::All;
        self.st.view = ViewKind::List;
        self.reload_now();
    }

    /// Tarjetas en el alcance actual (benchmarks).
    pub fn visible_tasks(&self) -> usize {
        self.snap.rows.len()
    }

    /// Vuelve a leer todo, como tras un evento del daemon (benchmarks).
    pub fn force_reload(&mut self) {
        self.reload_now();
    }
}

#[path = "sidebar.rs"]
mod sidebar;

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "gallery_tests.rs"]
mod gallery;

pub(crate) fn view_icon(v: ViewKind) -> Icon {
    match v {
        ViewKind::Board => Icon::Board,
        ViewKind::Calendar => Icon::Calendar,
        ViewKind::Table => Icon::Table,
        ViewKind::Gantt => Icon::Gantt,
        ViewKind::Today => Icon::Sun,
        ViewKind::Matrix => Icon::Matrix,
        ViewKind::Graph => Icon::Graph,
        _ => Icon::List,
    }
}
