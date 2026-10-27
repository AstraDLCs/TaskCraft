//! Cola de revisión (RF-SUP-06): entregas por aprobar o rechazar, y preguntas
//! de agentes que esperan respuesta.

use eframe::egui::{self, RichText, Ui};
use i18n::t;
use model::Status;

use super::common;
use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, UiState};
use crate::theme;

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let review: Vec<&TaskRow> = snap
        .review
        .iter()
        .filter(|r| r.task.status == Status::InReview)
        .collect();
    let questions: Vec<&TaskRow> = snap
        .review
        .iter()
        .filter(|r| r.task.needs_human.is_some())
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("review")
        .show(ui, |ui| {
            ui.heading(t(lang, "ui-review-questions"));
            if questions.is_empty() {
                ui.weak(t(lang, "ui-review-none"));
            }
            for row in questions {
                card(ui, st, row, out, |ui, st, out| {
                    ui.label(
                        RichText::new(row.task.needs_human.clone().unwrap_or_default())
                            .color(theme::VIOLET)
                            .strong(),
                    );
                    let answer = st.answers.entry(row.id()).or_default();
                    ui.add(
                        egui::TextEdit::multiline(answer)
                            .hint_text(t(lang, "ui-answer-hint"))
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(
                            !answer.trim().is_empty(),
                            egui::Button::new(t(lang, "ui-answer")),
                        )
                        .clicked()
                    {
                        out.push(Intent::run(
                            "task.answer",
                            &[("task", &row.key), ("answer", answer.trim())],
                        ));
                        st.answers.remove(&row.id());
                    }
                });
            }

            ui.add_space(12.0);
            ui.heading(t(lang, "ui-review-submissions"));
            if review.is_empty() {
                ui.weak(t(lang, "ui-review-none"));
            }
            for row in review {
                card(ui, st, row, out, |ui, st, out| {
                    let done = row.task.criteria.iter().filter(|c| c.done).count();
                    if !row.task.criteria.is_empty() {
                        ui.label(format!(
                            "{} {done}/{}",
                            t(lang, "ui-criteria"),
                            row.task.criteria.len()
                        ));
                        for c in &row.task.criteria {
                            let icon = if c.done { "[x]" } else { "[ ]" };
                            ui.label(RichText::new(format!("{icon} {}", c.text)).small());
                        }
                    }
                    match st.rejecting.as_mut() {
                        Some((id, reason, keep)) if *id == row.id() => {
                            ui.add(
                                egui::TextEdit::multiline(reason)
                                    .hint_text(t(lang, "ui-reject-hint"))
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.checkbox(keep, t(lang, "ui-reject-keep"));
                            let mut finished = false;
                            ui.horizontal(|ui| {
                                let ok = !reason.trim().is_empty();
                                if ui
                                    .add_enabled(ok, egui::Button::new(t(lang, "ui-reject")))
                                    .clicked()
                                {
                                    out.push(Intent::run(
                                        "task.reject",
                                        &[
                                            ("task", &row.key),
                                            ("reason", reason.trim()),
                                            ("keep_assignee", if *keep { "true" } else { "false" }),
                                        ],
                                    ));
                                    finished = true;
                                } else if ui.button(t(lang, "ui-cancel")).clicked() {
                                    finished = true;
                                }
                            });
                            if finished {
                                st.rejecting = None;
                            }
                        }
                        _ => {
                            ui.horizontal(|ui| {
                                if ui
                                    .button(
                                        RichText::new(t(lang, "ui-approve")).color(theme::GREEN),
                                    )
                                    .clicked()
                                {
                                    out.push(Intent::run(
                                        "task.move",
                                        &[("task", &row.key), ("status", "done")],
                                    ));
                                }
                                if ui
                                    .button(RichText::new(t(lang, "ui-reject")).color(theme::RED))
                                    .clicked()
                                {
                                    st.rejecting = Some((row.id(), String::new(), true));
                                }
                            });
                        }
                    }
                });
            }
        });
}

fn card(
    ui: &mut Ui,
    st: &mut UiState,
    row: &TaskRow,
    out: &mut Vec<Intent>,
    body: impl FnOnce(&mut Ui, &mut UiState, &mut Vec<Intent>),
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(&row.key).monospace().weak());
            common::title_link(ui, row, st.selected == Some(row.id()), out);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(who) = &row.assignee {
                    common::who(ui, who, 18.0);
                }
            });
        });
        body(ui, st, out);
    });
    ui.add_space(6.0);
}
