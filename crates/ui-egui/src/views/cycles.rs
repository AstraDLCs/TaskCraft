//! Ciclos o sprints (RF-INT-05): los de cada proyecto con su progreso, alta y
//! edición, cierre con traspaso de lo abierto al siguiente y los ya cerrados.

use eframe::egui::{self, RichText, Ui};
use i18n::{Msg, t, tr};
use model::{CycleState, ProjectStatus};

use crate::data::{Scope, Snapshot};
use crate::state::{CycleForm, Dialog, GoTo, Intent, UiState};
use crate::theme;
use crate::widgets::chip;
use crate::widgets::icons::{self, Icon};

fn state_key(s: CycleState) -> &'static str {
    match s {
        CycleState::Planned => "ui-cycle-planned",
        CycleState::Active => "ui-cycle-active",
        CycleState::Overdue => "ui-cycle-overdue",
        CycleState::Closed => "ui-cycle-closed",
    }
}

pub fn state_color(s: CycleState) -> egui::Color32 {
    match s {
        CycleState::Planned => theme::GRAY,
        CycleState::Active => theme::GREEN,
        CycleState::Overdue => theme::ORANGE,
        CycleState::Closed => theme::GRAY,
    }
}

/// El proyecto elegido o, con todos, cada proyecto activo.
pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    ui.add(egui::Label::new(RichText::new(t(lang, "ui-cycles-hint")).weak()).wrap());
    ui.add_space(4.0);
    let projects: Vec<_> = snap
        .projects
        .iter()
        .filter(|p| match st.scope {
            Scope::Project(id) => p.id == id,
            Scope::All => p.status == ProjectStatus::Active,
        })
        .collect();
    if projects.is_empty() {
        ui.weak(t(lang, "ui-no-projects"));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("cycles")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for p in projects {
                ui.horizontal(|ui| {
                    ui.heading(format!("{} · {}", p.key, p.name));
                    if ui.button(t(lang, "ui-cycle-new")).clicked() {
                        let form =
                            CycleForm::after_last(snap, p.id, crate::widgets::date_picker::today());
                        st.dialog = Some(Dialog::Cycle(form));
                    }
                });
                project(ui, snap, st, p.id, out);
                ui.add_space(8.0);
            }
        });
}

fn project(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    project: model::ProjectId,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let today = crate::widgets::date_picker::today();
    let open: Vec<_> = snap.open_cycles(project).collect();
    if open.is_empty() {
        ui.weak(t(lang, "ui-cycles-empty"));
    }
    for c in open {
        let state = c.state(today);
        let (done, total) = snap.cycle_progress(c.id);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                icons::show(ui, Icon::Cycle, state_color(state));
                ui.label(RichText::new(&c.name).strong());
                chip::chip(ui, &t(lang, state_key(state)), state_color(state));
                ui.weak(format!("{} – {}", c.start, c.end));
            });
            let fraction = if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_width(ui.available_width())
                    .text(tr(
                        lang,
                        &Msg::new("ui-epic-progress")
                            .arg("done", done)
                            .arg("total", total),
                    )),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button(t(lang, "ui-cycle-tasks")).clicked() {
                    out.push(Intent::GoTo(GoTo::cycle(c)));
                }
                if ui.button(t(lang, "ui-cycle-edit")).clicked() {
                    st.dialog = Some(Dialog::Cycle(CycleForm::edit(c)));
                }
                if ui
                    .button(t(lang, "ui-cycle-close"))
                    .on_hover_text(t(lang, "ui-cycle-close-hint"))
                    .clicked()
                {
                    out.push(Intent::run("cycle.close", &[("cycle", &c.id.to_string())]));
                }
                if ui
                    .button(RichText::new(t(lang, "ui-cycle-delete")).color(theme::RED))
                    .clicked()
                {
                    st.dialog = Some(Dialog::ConfirmCycleDelete(c.id, c.name.clone()));
                }
            });
        });
        ui.add_space(4.0);
    }
    let closed: Vec<_> = snap
        .cycles
        .iter()
        .filter(|c| c.project_id == project && c.closed_at.is_some())
        .collect();
    if closed.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(tr(
        lang,
        &Msg::new("ui-cycles-closed").arg("count", closed.len()),
    ))
    .id_salt(("cycles-closed", project))
    .show(ui, |ui| {
        for c in closed.into_iter().rev() {
            ui.horizontal_wrapped(|ui| {
                icons::show(ui, Icon::Cycle, theme::GRAY);
                ui.label(RichText::new(&c.name).weak());
                ui.weak(format!("{} – {}", c.start, c.end));
                if ui.small_button(t(lang, "ui-cycle-tasks")).clicked() {
                    out.push(Intent::GoTo(GoTo::cycle(c)));
                }
                if icons::button(ui, Icon::Trash, true, &t(lang, "ui-cycle-delete")).clicked() {
                    st.dialog = Some(Dialog::ConfirmCycleDelete(c.id, c.name.clone()));
                }
            });
        }
    });
}
