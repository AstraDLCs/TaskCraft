//! Épicas e hitos (RF-MOD-13): progreso de cada una, alta, cierre, bloqueo
//! humano y acceso a sus tareas.

use eframe::egui::{self, RichText, Ui};
use i18n::{Msg, t, tr};
use model::{EpicKind, EpicStatus, ProjectStatus};

use crate::data::{Scope, Snapshot};
use crate::state::{GoTo, Intent, UiState};
use crate::theme;
use crate::widgets::{chip, date_picker};

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    new_form(ui, snap, st, out);
    ui.separator();
    let lines: Vec<_> = snap
        .epics
        .iter()
        .filter(|e| match st.scope {
            Scope::Project(id) => e.epic.project_id == id,
            Scope::All => true,
        })
        .collect();
    if lines.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-no-epics")));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("epics")
        .show(ui, |ui| {
            for kind in EpicKind::ALL {
                let of_kind: Vec<_> = lines.iter().filter(|e| e.epic.kind == *kind).collect();
                if of_kind.is_empty() {
                    continue;
                }
                ui.heading(t(lang, kind.label_key()));
                for line in of_kind {
                    let e = &line.epic;
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let icon = match e.kind {
                                EpicKind::Epic => crate::widgets::icons::Icon::Epic,
                                EpicKind::Milestone => crate::widgets::icons::Icon::Milestone,
                            };
                            crate::widgets::icons::show(ui, icon, theme::VIOLET);
                            let mut name = RichText::new(&e.name).strong();
                            if e.status == EpicStatus::Closed {
                                name = name.strikethrough().weak();
                            }
                            ui.label(name);
                            chip::chip(ui, &line.project_key, theme::GRAY);
                            if let Some(target) = &e.target {
                                ui.weak(target.date().to_string());
                            }
                            if e.locked {
                                crate::widgets::icons::show(
                                    ui,
                                    crate::widgets::icons::Icon::Lock,
                                    theme::GRAY,
                                );
                            }
                        });
                        ui.add(
                            egui::ProgressBar::new(line.fraction())
                                .desired_width(ui.available_width())
                                .text(tr(
                                    lang,
                                    &Msg::new("ui-epic-progress")
                                        .arg("done", line.done)
                                        .arg("total", line.total),
                                )),
                        );
                        ui.horizontal(|ui| {
                            if ui.button(t(lang, "ui-epic-tasks")).clicked() {
                                out.push(Intent::GoTo(GoTo::epic(e)));
                            }
                            let id = e.id.to_string();
                            let (label, cmd) = match e.status {
                                EpicStatus::Open => ("ui-epic-close", "epic.close"),
                                EpicStatus::Closed => ("ui-epic-reopen", "epic.reopen"),
                            };
                            if ui.button(t(lang, label)).clicked() {
                                out.push(Intent::run(cmd, &[("epic", &id)]));
                            }
                            let (label, cmd) = if e.locked {
                                ("ui-unlock", "lock.clear")
                            } else {
                                ("ui-lock", "lock.set")
                            };
                            if ui.button(t(lang, label)).clicked() {
                                out.push(Intent::run(cmd, &[("epic", &id)]));
                            }
                            if ui
                                .button(RichText::new(t(lang, "ui-delete")).color(theme::RED))
                                .clicked()
                            {
                                out.push(Intent::run("epic.delete", &[("epic", &id)]));
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
}

fn new_form(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let projects: Vec<_> = snap
        .projects
        .iter()
        .filter(|p| p.status == ProjectStatus::Active)
        .collect();
    let f = &mut st.epic_form;
    if let Scope::Project(id) = st.scope {
        f.project = Some(id);
    }
    if f.project.is_none() {
        f.project = projects.first().map(|p| p.id);
    }
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("epic_kind")
            .selected_text(t(lang, f.kind.label_key()))
            .show_ui(ui, |ui| {
                for k in EpicKind::ALL {
                    ui.selectable_value(&mut f.kind, *k, t(lang, k.label_key()));
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut f.name)
                .hint_text(t(lang, "ui-epic-name-hint"))
                .desired_width(220.0),
        );
        let shown = f
            .project
            .and_then(|id| projects.iter().find(|p| p.id == id))
            .map(|p| p.key.clone())
            .unwrap_or_else(|| "—".into());
        egui::ComboBox::from_id_salt("epic_project")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for p in &projects {
                    ui.selectable_value(&mut f.project, Some(p.id), &p.key);
                }
            });
        ui.weak(t(lang, "param-target"));
        if let Some(new) = date_picker::date_button(ui, "epic_target", f.target, lang) {
            f.target = new;
        }
        let project = f
            .project
            .and_then(|id| projects.iter().find(|p| p.id == id))
            .map(|p| p.key.clone());
        let ready = !f.name.trim().is_empty() && project.is_some();
        if ui
            .add_enabled(ready, egui::Button::new(t(lang, "ui-add")))
            .clicked()
            && let Some(project) = project
        {
            let mut pairs: Vec<(&str, String)> = vec![
                ("project", project),
                ("name", f.name.trim().to_owned()),
                ("kind", f.kind.as_str().to_owned()),
            ];
            if let Some(d) = f.target {
                pairs.push(("target", d.to_string()));
            }
            let args = pairs
                .iter()
                .fold(engine::Args::none(), |a, (k, v)| a.with(*k, v.as_str()));
            out.push(Intent::Run {
                id: "epic.create",
                args,
            });
            f.name.clear();
            f.target = None;
        }
    });
}
