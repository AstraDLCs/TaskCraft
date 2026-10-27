//! La app web (D14): supervisar desde otra máquina. Revisión y preguntas,
//! tareas con el lenguaje de filtros (D12), agentes y actividad. Todo pasa por
//! los comandos del registro, así que valen las mismas reglas que en
//! escritorio.

use std::collections::HashMap;

use egui::{Color32, RichText};
use i18n::{Lang, Msg, t, tr};
use serde_json::{Value, json};

use crate::api::{Inbox, Purpose, Reply, Transport, take};

/// Crea el transporte para un token (en el navegador, `fetch` al daemon).
pub type Connect = Box<dyn Fn(&str) -> Box<dyn Transport>>;

const BASE_STATUSES: [&str; 6] = [
    "backlog",
    "todo",
    "in_progress",
    "in_review",
    "done",
    "cancelled",
];
const REVIEW_FILTER: &str = "status:in_review OR needs_human:true";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Review,
    Tasks,
    Agents,
    Activity,
}

impl View {
    const ALL: [View; 4] = [View::Review, View::Tasks, View::Agents, View::Activity];

    fn label_key(self) -> &'static str {
        match self {
            Self::Review => "ui-view-review",
            Self::Tasks => "web-tasks",
            Self::Agents => "ui-view-agents",
            Self::Activity => "web-activity",
        }
    }
}

#[derive(Default)]
struct Data {
    projects: Vec<String>,
    tasks: Vec<Value>,
    review: Vec<Value>,
    agents: Vec<Value>,
    activity: Vec<Value>,
}

pub struct WebApp {
    pub lang: Lang,
    pub view: View,
    pub token_input: String,
    pub remember: bool,
    connect: Connect,
    transport: Option<Box<dyn Transport>>,
    inbox: Inbox,
    /// Nombre de quien está conectado; `None` mientras no se verifica el token.
    pub actor: Option<String>,
    change_token: Option<u64>,
    data: Data,
    pub project: Option<String>,
    pub filter: String,
    pub reasons: HashMap<String, String>,
    pub answers: HashMap<String, String>,
    pub error: Option<String>,
}

impl WebApp {
    pub fn new(connect: Connect, inbox: Inbox, lang: Lang) -> Self {
        Self {
            lang,
            view: View::Review,
            token_input: String::new(),
            remember: false,
            connect,
            transport: None,
            inbox,
            actor: None,
            change_token: None,
            data: Data::default(),
            project: None,
            filter: String::new(),
            reasons: HashMap::new(),
            answers: HashMap::new(),
            error: None,
        }
    }

    /// Prueba el token y, si vale, empieza a cargar y a escuchar cambios.
    pub fn login(&mut self) {
        let token = self.token_input.trim().to_owned();
        if token.is_empty() {
            return;
        }
        let transport = (self.connect)(&token);
        transport.whoami();
        self.transport = Some(transport);
        self.error = None;
    }

    pub fn logout(&mut self) {
        self.transport = None;
        self.actor = None;
        self.change_token = None;
        self.data = Data::default();
    }

    pub fn logged_in(&self) -> bool {
        self.actor.is_some()
    }

    fn run(&self, purpose: Purpose, command: &str, args: Value) {
        if let Some(t) = &self.transport {
            t.command(purpose, command, args);
        }
    }

    /// Vuelve a pedir todo lo que se ve (es poco: la web supervisa).
    pub fn reload(&self) {
        self.run(Purpose::Projects, "project.list", json!({ "limit": 200 }));
        self.run(
            Purpose::Review,
            "task.list",
            json!({ "filter": REVIEW_FILTER, "limit": 200 }),
        );
        let mut args = serde_json::Map::new();
        args.insert("limit".into(), json!(500));
        if let Some(p) = &self.project {
            args.insert("project".into(), json!(p));
        }
        if !self.filter.trim().is_empty() {
            args.insert("filter".into(), json!(self.filter.trim()));
        }
        self.run(Purpose::Tasks, "task.list", Value::Object(args));
        self.run(Purpose::Agents, "agent.list", json!({ "limit": 200 }));
        self.run(Purpose::Activity, "activity.list", json!({ "limit": 50 }));
    }

    /// Respuestas que llegaron desde el último frame.
    pub fn absorb(&mut self) {
        for Reply { purpose, result } in take(&self.inbox) {
            let value = match result {
                Ok(v) => v,
                Err(e) => {
                    if purpose == Purpose::WhoAmI {
                        self.transport = None;
                    }
                    if purpose == Purpose::Changes {
                        // Se reintenta con el siguiente frame.
                        self.change_token = None;
                        self.listen();
                    }
                    self.error = Some(e);
                    continue;
                }
            };
            let items = || {
                value
                    .get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            };
            match purpose {
                Purpose::WhoAmI => {
                    self.actor = value
                        .get("actor")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    self.reload();
                    self.listen();
                }
                Purpose::Projects => {
                    self.data.projects = items()
                        .iter()
                        .filter_map(|p| p["key"].as_str().map(str::to_owned))
                        .collect();
                }
                Purpose::Tasks => self.data.tasks = items(),
                Purpose::Review => self.data.review = items(),
                Purpose::Agents => self.data.agents = items(),
                Purpose::Activity => self.data.activity = items(),
                Purpose::Action => {
                    self.error = None;
                    self.reload();
                }
                Purpose::Changes => {
                    let token = value.get("token").and_then(Value::as_u64);
                    if self.change_token.is_some() && token != self.change_token {
                        self.reload();
                    }
                    self.change_token = token;
                    self.listen();
                }
            }
        }
    }

    /// Sondeo largo: el daemon responde cuando cambia algo (o a los 25 s).
    fn listen(&self) {
        if let Some(t) = &self.transport {
            t.changes(self.change_token);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.absorb();
        if !self.logged_in() {
            self.login_ui(ui);
            return;
        }
        let lang = self.lang;
        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("TaskCraft");
                ui.separator();
                for v in View::ALL {
                    ui.selectable_value(&mut self.view, v, t(lang, v.label_key()));
                }
                ui.separator();
                self.language(ui);
                if let Some(actor) = &self.actor {
                    ui.weak(actor);
                }
                if ui.button(t(lang, "web-logout")).clicked() {
                    self.logout();
                }
            });
            if let Some(e) = &self.error {
                ui.colored_label(Color32::from_rgb(0xe5, 0x48, 0x4d), e);
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.view {
                View::Review => self.review_ui(ui),
                View::Tasks => self.tasks_ui(ui),
                View::Agents => self.agents_ui(ui),
                View::Activity => self.activity_ui(ui),
            });
        });
    }

    fn language(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("lang")
            .selected_text(self.lang.native_name())
            .show_ui(ui, |ui| {
                for l in Lang::ALL {
                    ui.selectable_value(&mut self.lang, l, l.native_name());
                }
            });
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("TaskCraft");
                ui.label(t(lang, "web-login-hint"));
                ui.add_space(12.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.token_input)
                        .password(true)
                        .hint_text("tc_…")
                        .desired_width(360.0),
                );
                ui.checkbox(&mut self.remember, t(lang, "web-remember"));
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button(t(lang, "web-login")).clicked() || enter {
                    self.login();
                }
                if self.transport.is_some() {
                    ui.spinner();
                }
                if let Some(e) = &self.error {
                    ui.colored_label(Color32::from_rgb(0xe5, 0x48, 0x4d), e);
                }
                ui.add_space(8.0);
                self.language(ui);
            });
        });
    }

    fn review_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        if self.data.review.is_empty() {
            ui.weak(t(lang, "web-review-empty"));
        }
        let mut actions = Vec::new();
        for task in &self.data.review {
            let key = task["key"].as_str().unwrap_or_default().to_owned();
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&key).monospace().strong());
                    ui.label(task["title"].as_str().unwrap_or_default());
                    if let Some(who) = task["claimed_by"].as_str() {
                        ui.weak(who);
                    }
                });
                if task["needs_human"].as_bool() == Some(true) {
                    ui.label(t(lang, "web-question"));
                    let answer = self.answers.entry(key.clone()).or_default();
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(answer);
                        if ui.button(t(lang, "web-answer")).clicked() && !answer.trim().is_empty() {
                            actions.push((
                                "task.answer",
                                json!({ "task": key, "answer": answer.trim() }),
                            ));
                        }
                    });
                } else {
                    let reason = self.reasons.entry(key.clone()).or_default();
                    ui.horizontal(|ui| {
                        if ui.button(t(lang, "web-approve")).clicked() {
                            actions.push(("task.move", json!({ "task": key, "status": "done" })));
                        }
                        ui.add(
                            egui::TextEdit::singleline(reason)
                                .hint_text(t(lang, "web-reject-reason")),
                        );
                        if ui
                            .add_enabled(
                                !reason.trim().is_empty(),
                                egui::Button::new(t(lang, "web-reject")),
                            )
                            .clicked()
                        {
                            actions.push((
                                "task.reject",
                                json!({ "task": key, "reason": reason.trim() }),
                            ));
                        }
                    });
                }
            });
        }
        for (command, args) in actions {
            self.run(Purpose::Action, command, args);
        }
    }

    fn tasks_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let mut apply = false;
        ui.horizontal_wrapped(|ui| {
            let current = self
                .project
                .clone()
                .unwrap_or_else(|| t(lang, "ui-all-projects"));
            egui::ComboBox::from_id_salt("project")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    apply |= ui
                        .selectable_value(&mut self.project, None, t(lang, "ui-all-projects"))
                        .changed();
                    for p in &self.data.projects {
                        apply |= ui
                            .selectable_value(&mut self.project, Some(p.clone()), p)
                            .changed();
                    }
                });
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("status:todo priority>=high")
                    .desired_width(320.0),
            );
            apply |= edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            apply |= ui.button(t(lang, "web-apply")).clicked();
        });
        if apply {
            self.reload();
        }
        ui.separator();
        let mut moves = Vec::new();
        egui::Grid::new("tasks").striped(true).show(ui, |ui| {
            for task in &self.data.tasks {
                let key = task["key"].as_str().unwrap_or_default();
                ui.label(RichText::new(key).monospace());
                ui.label(task["title"].as_str().unwrap_or_default());
                let status = task["status"].as_str().unwrap_or_default();
                let name = |s: &str| {
                    i18n::lookup(lang, &format!("status-{s}")).unwrap_or_else(|| s.to_owned())
                };
                egui::ComboBox::from_id_salt(("status", key))
                    .selected_text(name(status))
                    .show_ui(ui, |ui| {
                        for s in BASE_STATUSES {
                            if ui.selectable_label(s == status, name(s)).clicked() && s != status {
                                moves.push(json!({ "task": key, "status": s }));
                            }
                        }
                    });
                ui.weak(task["priority"].as_str().unwrap_or_default());
                ui.end_row();
            }
        });
        if self.data.tasks.is_empty() {
            ui.weak(t(lang, "ui-no-tasks"));
        }
        for args in moves {
            self.run(Purpose::Action, "task.move", args);
        }
    }

    fn agents_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let mut actions = Vec::new();
        egui::Grid::new("agents").striped(true).show(ui, |ui| {
            for a in &self.data.agents {
                let name = a["name"].as_str().unwrap_or_default();
                ui.label(RichText::new(name).strong());
                ui.weak(a["status"].as_str().unwrap_or_default());
                let paused = a["paused"].as_bool() == Some(true);
                let (key, command) = if paused {
                    ("web-resume", "agent.resume")
                } else {
                    ("web-pause", "agent.pause")
                };
                if ui.button(t(lang, key)).clicked() {
                    actions.push((command, json!({ "agent": name })));
                }
                ui.end_row();
            }
        });
        for (command, args) in actions {
            self.run(Purpose::Action, command, args);
        }
    }

    fn activity_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        for e in &self.data.activity {
            let line = tr(
                lang,
                &Msg::new("web-activity-line")
                    .arg(
                        "at",
                        e["at"]
                            .as_str()
                            .unwrap_or_default()
                            .get(..16)
                            .unwrap_or_default(),
                    )
                    .arg("actor", e["actor_name"].as_str().unwrap_or_default())
                    .arg("action", e["action"].as_str().unwrap_or_default())
                    .arg("target", e["target"].as_str().unwrap_or_default()),
            );
            ui.label(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::api::push;

    /// Transporte de prueba: apunta las peticiones.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Vec<String>>>);

    impl Transport for Fake {
        fn command(&self, _: Purpose, command: &str, args: Value) {
            self.0.lock().unwrap().push(format!("{command} {args}"));
        }
        fn changes(&self, since: Option<u64>) {
            self.0.lock().unwrap().push(format!("changes {since:?}"));
        }
        fn whoami(&self) {
            self.0.lock().unwrap().push("whoami".into());
        }
    }

    fn app() -> (WebApp, Fake, Inbox) {
        let fake = Fake::default();
        let inbox: Inbox = Arc::default();
        let f = fake.clone();
        let app = WebApp::new(
            Box::new(move |_| Box::new(f.clone())),
            inbox.clone(),
            Lang::Es,
        );
        (app, fake, inbox)
    }

    #[test]
    fn logging_in_loads_everything_and_listens_for_changes() {
        let (mut app, fake, inbox) = app();
        app.token_input = "tc_x".into();
        app.login();
        assert_eq!(fake.0.lock().unwrap().as_slice(), ["whoami"]);
        push(
            &inbox,
            Reply {
                purpose: Purpose::WhoAmI,
                result: Ok(json!({ "actor": "supervisor" })),
            },
        );
        app.absorb();
        assert!(app.logged_in());
        let calls = fake.0.lock().unwrap().clone();
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("task.list") && c.contains("in_review"))
        );
        assert_eq!(calls.last().map(String::as_str), Some("changes None"));

        // Un cambio en el daemon recarga y vuelve a escuchar.
        push(
            &inbox,
            Reply {
                purpose: Purpose::Changes,
                result: Ok(json!({ "token": 7 })),
            },
        );
        app.absorb();
        fake.0.lock().unwrap().clear();
        push(
            &inbox,
            Reply {
                purpose: Purpose::Changes,
                result: Ok(json!({ "token": 8 })),
            },
        );
        app.absorb();
        let calls = fake.0.lock().unwrap().clone();
        assert!(calls.iter().any(|c| c.starts_with("agent.list")));
        assert_eq!(calls.last().map(String::as_str), Some("changes Some(8)"));
    }

    #[test]
    fn a_bad_token_goes_back_to_the_login() {
        let (mut app, _, inbox) = app();
        app.token_input = "tc_malo".into();
        app.login();
        push(
            &inbox,
            Reply {
                purpose: Purpose::WhoAmI,
                result: Err("invalid token".into()),
            },
        );
        app.absorb();
        assert!(!app.logged_in());
        assert_eq!(app.error.as_deref(), Some("invalid token"));
    }

    /// Un frame completo sin ventana, con datos de ejemplo en cada vista.
    #[test]
    fn every_view_renders() {
        let (mut app, _, inbox) = app();
        app.token_input = "tc_x".into();
        app.login();
        push(
            &inbox,
            Reply {
                purpose: Purpose::WhoAmI,
                result: Ok(json!({ "actor": "supervisor" })),
            },
        );
        let task = json!({ "key": "API-1", "title": "t", "status": "in_review", "priority": "high", "needs_human": false });
        push(
            &inbox,
            Reply {
                purpose: Purpose::Review,
                result: Ok(json!({ "items": [task.clone()] })),
            },
        );
        push(
            &inbox,
            Reply {
                purpose: Purpose::Tasks,
                result: Ok(json!({ "items": [task] })),
            },
        );
        push(
            &inbox,
            Reply {
                purpose: Purpose::Agents,
                result: Ok(
                    json!({ "items": [{ "name": "bot", "status": "active", "paused": false }] }),
                ),
            },
        );
        push(
            &inbox,
            Reply {
                purpose: Purpose::Activity,
                result: Ok(
                    json!({ "items": [{ "at": "2026-10-26T10:00:00Z", "actor_name": "bot", "action": "updated", "target": "API-1" }] }),
                ),
            },
        );
        let ctx = egui::Context::default();
        for view in View::ALL {
            app.view = view;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui));
            out.textures_delta.clear();
            assert!(!out.shapes.is_empty(), "{view:?}");
        }
    }
}
