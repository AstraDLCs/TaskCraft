//! Bandeja de triaje (RF-VIS-07): hallazgos de agentes que el supervisor
//! convierte en tareas o descarta.

use eframe::egui::{self, RichText, Ui};
use i18n::t;
use model::ProjectStatus;

use super::common;
use crate::data::Snapshot;
use crate::state::{Intent, UiState};
use crate::theme;
use crate::widgets::chip;

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    if snap.inbox.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-inbox-empty")));
        return;
    }
    let projects: Vec<_> = snap
        .projects
        .iter()
        .filter(|p| p.status == ProjectStatus::Active)
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("inbox")
        .show(ui, |ui| {
            for row in &snap.inbox {
                let item = &row.item;
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let color = match item.kind {
                            model::InboxKind::Bug => theme::RED,
                            model::InboxKind::Debt => theme::ORANGE,
                            model::InboxKind::Idea => theme::BLUE,
                        };
                        chip::chip(ui, &t(lang, item.kind.label_key()), color);
                        ui.strong(&item.title);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(common::ago(lang, item.created_at));
                            common::who(ui, &row.author, 16.0);
                        });
                    });
                    if !item.body.is_empty() {
                        ui.label(RichText::new(&item.body).weak());
                    }
                    ui.horizontal(|ui| {
                        let target = st
                            .inbox_target
                            .get(&item.id)
                            .copied()
                            .or_else(|| projects.first().map(|p| p.id));
                        let mut chosen = target;
                        let shown = target
                            .and_then(|id| projects.iter().find(|p| p.id == id))
                            .map(|p| p.key.clone())
                            .unwrap_or_else(|| "—".into());
                        egui::ComboBox::from_id_salt(("inbox_target", item.id))
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for p in &projects {
                                    ui.selectable_value(
                                        &mut chosen,
                                        Some(p.id),
                                        format!("{} · {}", p.key, p.name),
                                    );
                                }
                            });
                        if let Some(id) = chosen {
                            st.inbox_target.insert(item.id, id);
                        }
                        let project_key = chosen
                            .and_then(|id| projects.iter().find(|p| p.id == id))
                            .map(|p| p.key.clone());
                        if ui
                            .add_enabled(
                                project_key.is_some(),
                                egui::Button::new(t(lang, "ui-convert")),
                            )
                            .clicked()
                            && let Some(key) = project_key
                        {
                            out.push(Intent::run(
                                "inbox.convert",
                                &[("item", &item.id.to_string()), ("project", &key)],
                            ));
                        }
                        if ui.button(t(lang, "ui-discard")).clicked() {
                            out.push(Intent::run(
                                "inbox.discard",
                                &[("item", &item.id.to_string())],
                            ));
                        }
                    });
                });
                ui.add_space(6.0);
            }
        });
}
