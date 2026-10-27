//! Panel de detalle de una tarea (RF-VIS-03): todo lo que tiene una tarea se
//! ve y se cambia aquí.

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, Msg, t, tr};
use model::{Actor, CheckItem};

use crate::data::{Detail, Snapshot, TaskRef};
use crate::state::{Intent, ItemKind, UiState};
use crate::views::common;
use crate::widgets::icons::{self, Icon};
use crate::widgets::inline::{self, Outcome};
use crate::widgets::markdown::Markdown;
use crate::widgets::{chip, date_picker};
use crate::{hierarchy, theme};

/// Sincroniza los textos editables cuando cambia la tarea seleccionada.
pub fn sync(st: &mut UiState, detail: &Detail) {
    let task = &detail.row.task;
    if st.edit.task != Some(task.id) {
        st.edit = crate::state::DetailEdit {
            task: Some(task.id),
            title: task.title.clone(),
            description: task.description.clone(),
            ..Default::default()
        };
    } else if !st.edit.editing_description {
        st.edit.title = task.title.clone();
        st.edit.description = task.description.clone();
        // Lo guardado manda: se vuelve a leer al refrescar.
        st.edit.estimate = None;
        st.edit.field_values.clear();
    }
}

pub fn show(
    ui: &mut Ui,
    detail: &Detail,
    snap: &Snapshot,
    st: &mut UiState,
    md: &mut Markdown,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let row = &detail.row;
    let task = &row.task;
    let key = row.key.clone();

    ui.horizontal(|ui| {
        ui.label(RichText::new(&key).monospace().strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Close, true, &t(lang, "ui-close")).clicked() {
                out.push(Intent::Select(None));
            }
        });
    });
    // Ruta de ancestros, clicable.
    if !detail.ancestors.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for a in &detail.ancestors {
                if ui
                    .link(RichText::new(&a.key).small())
                    .on_hover_text(&a.title)
                    .clicked()
                {
                    out.push(Intent::Select(Some(a.id)));
                }
                ui.label(RichText::new("/").small().weak());
            }
        });
    }

    let title = ui.add(
        egui::TextEdit::singleline(&mut st.edit.title)
            .font(egui::TextStyle::Heading)
            .desired_width(f32::INFINITY),
    );
    if title.lost_focus() && st.edit.title.trim() != task.title && !st.edit.title.trim().is_empty()
    {
        out.push(Intent::run(
            "task.update",
            &[("task", &key), ("title", st.edit.title.trim())],
        ));
    }

    egui::ScrollArea::vertical()
        .id_salt("detail")
        .show(ui, |ui| {
            alerts(ui, detail, st, out);
            fields(ui, detail, snap, st, out);
            crate::detail_extra::planning(ui, detail, snap, st, out);
            ui.separator();
            description(ui, detail, st, md, out);
            items(ui, &key, &task.checklist, ItemKind::Checklist, st, out);
            items(ui, &key, &task.criteria, ItemKind::Criteria, st, out);
            subtasks(ui, detail, snap, st, out);
            relations(ui, detail, st, out);
            crate::detail_extra::links(ui, detail, snap, st, out);
            crate::detail_extra::diagrams(ui, detail, snap, lang, out);
            crate::detail_extra::attachments(ui, detail, st, out);
            crate::detail_extra::time(ui, detail, st, out);
            crate::detail_extra::cost(ui, detail, lang);
            crate::detail_extra::git(ui, detail, lang);
            comments(ui, detail, st, md, out);
            history(ui, detail, lang, out);
            ui.separator();
            actions(ui, detail, snap, st, out);
        });
}

/// Claim, escalado y bloqueo humano, con su acción.
fn alerts(ui: &mut Ui, detail: &Detail, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let row = &detail.row;
    let key = &row.key;
    if let Some(claim) = &row.claim {
        egui::Frame::new()
            .fill(theme::ORANGE.gamma_multiply(0.15))
            .corner_radius(4.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    common::who(ui, &claim.agent, 18.0);
                    ui.label(tr(
                        lang,
                        &Msg::new("ui-claimed-since").arg("ago", common::ago(lang, claim.since)),
                    ));
                    ui.weak(common::remaining(lang, claim.until));
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut st.edit.release_reason)
                            .hint_text(t(lang, "ui-release-reason"))
                            .desired_width(180.0),
                    );
                    if ui.button(t(lang, "ui-release")).clicked() {
                        let reason = st.edit.release_reason.trim().to_owned();
                        let mut args = vec![("task", key.as_str())];
                        if !reason.is_empty() {
                            args.push(("reason", reason.as_str()));
                        }
                        out.push(Intent::run("task.release", &args));
                        st.edit.release_reason.clear();
                    }
                });
            });
    }
    if let Some(question) = &row.task.needs_human {
        egui::Frame::new()
            .fill(theme::VIOLET.gamma_multiply(0.15))
            .corner_radius(4.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::show(ui, Icon::Warning, theme::VIOLET);
                    ui.strong(t(lang, "ui-needs-human"));
                });
                ui.label(question);
                ui.add(
                    egui::TextEdit::multiline(&mut st.edit.answer)
                        .hint_text(t(lang, "ui-answer-hint"))
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if ui
                    .add_enabled(
                        !st.edit.answer.trim().is_empty(),
                        egui::Button::new(t(lang, "ui-answer")),
                    )
                    .clicked()
                {
                    out.push(Intent::run(
                        "task.answer",
                        &[("task", key), ("answer", st.edit.answer.trim())],
                    ));
                    st.edit.answer.clear();
                }
            });
    }
    if row.task.locked {
        ui.horizontal(|ui| {
            icons::show(ui, Icon::Lock, theme::GRAY);
            ui.label(t(lang, "ui-locked-hint"));
        });
    }
}

fn fields(ui: &mut Ui, detail: &Detail, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let row = &detail.row;
    let task = &row.task;
    let key = row.key.as_str();
    egui::Grid::new("detail-fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.weak(t(lang, "ui-col-status"));
            if let Some(s) = crate::flow::status_combo(ui, ("d-st", task.id), task, snap, lang) {
                out.push(Intent::run("task.move", &[("task", key), ("status", &s)]));
            }
            ui.end_row();

            ui.weak(t(lang, "ui-col-priority"));
            if let Some(p) = common::priority_combo(ui, ("d-pr", task.id), task.priority, lang) {
                out.push(Intent::run(
                    "task.update",
                    &[("task", key), ("priority", p.as_str())],
                ));
            }
            ui.end_row();

            // Tantos agentes como haga falta, sin papeles (D20).
            ui.weak(t(lang, "ui-assignees"));
            ui.vertical(|ui| assignees(ui, detail, key, snap, lang, out));
            ui.end_row();

            for (label, field, value) in [
                ("ui-col-start", "start", task.start.as_ref()),
                ("ui-col-due", "due", task.due.as_ref()),
            ] {
                ui.weak(t(lang, label));
                if let Some(new) = date_picker::date_button(
                    ui,
                    ("d-date", field, task.id),
                    value.map(|w| w.date()),
                    lang,
                ) {
                    let value = new.map(|d| d.to_string()).unwrap_or_default();
                    out.push(Intent::run(
                        "task.update",
                        &[("task", key), (field, &value)],
                    ));
                }
                ui.end_row();
            }

            ui.weak(t(lang, "ui-col-labels"));
            ui.horizontal_wrapped(|ui| {
                for (name, color) in &row.labels {
                    chip::chip(ui, name, *color);
                    if icons::button_sized(ui, Icon::Close, true, &t(lang, "ui-remove"), 12.0)
                        .clicked()
                    {
                        out.push(Intent::run(
                            "task.label",
                            &[("task", key), ("remove", name)],
                        ));
                    }
                }
                let mut pick: Option<String> = None;
                egui::ComboBox::from_id_salt(("d-lab", task.id))
                    .selected_text(t(lang, "ui-add-label"))
                    .show_ui(ui, |ui| {
                        for l in &snap.labels {
                            if !row.labels.iter().any(|(n, _)| *n == l.name)
                                && ui.selectable_label(false, &l.name).clicked()
                            {
                                pick = Some(l.name.clone());
                            }
                        }
                        ui.separator();
                        ui.add(
                            egui::TextEdit::singleline(&mut st.edit.new_label)
                                .hint_text(t(lang, "ui-new-label-hint")),
                        );
                        if ui.button(t(lang, "ui-add")).clicked()
                            && !st.edit.new_label.trim().is_empty()
                        {
                            pick = Some(st.edit.new_label.trim().to_owned());
                            st.edit.new_label.clear();
                        }
                    });
                if let Some(name) = pick {
                    out.push(Intent::run("task.label", &[("task", key), ("add", &name)]));
                }
            });
            ui.end_row();

            ui.weak(t(lang, "ui-parent"));
            let current = row.parent_key.clone().unwrap_or_else(|| t(lang, "ui-root"));
            let mut chosen: Option<Option<String>> = None;
            ui.menu_button(current, |ui| {
                if task.parent_id.is_some() && ui.button(t(lang, "ui-make-root")).clicked() {
                    chosen = Some(None);
                }
                let candidates = hierarchy::parent_candidates(snap, task.id, &st.parent_query);
                if let Some(p) = hierarchy::task_picker(ui, &candidates, &mut st.parent_query, lang)
                {
                    chosen = Some(Some(p));
                }
                if chosen.is_some() {
                    st.parent_query.clear();
                    ui.close();
                }
            });
            if let Some(parent) = chosen {
                out.push(hierarchy::set_parent(key, parent.as_deref()));
            }
            ui.end_row();

            ui.weak(t(lang, "ui-created-by"));
            ui.label(format!(
                "{} · {}",
                match &task.creator {
                    Actor::Agent(id) => snap
                        .agents
                        .iter()
                        .find(|a| a.agent.id == *id)
                        .map(|a| a.agent.name.clone())
                        .unwrap_or_else(|| id.to_string()),
                    other => other.to_string(),
                },
                common::ago(lang, task.created_at)
            ));
            ui.end_row();
        });
}

fn description(
    ui: &mut Ui,
    detail: &Detail,
    st: &mut UiState,
    md: &mut Markdown,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let key = &detail.row.key;
    ui.horizontal(|ui| {
        ui.strong(t(lang, "ui-description"));
        if !st.edit.editing_description
            && icons::button_sized(ui, Icon::Edit, true, &t(lang, "ui-edit"), 16.0).clicked()
        {
            st.edit.editing_description = true;
        }
    });
    if st.edit.editing_description {
        ui.add(
            egui::TextEdit::multiline(&mut st.edit.description)
                .code_editor()
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.button(t(lang, "ui-save")).clicked() {
                out.push(Intent::run(
                    "task.update",
                    &[("task", key), ("description", &st.edit.description)],
                ));
                st.edit.editing_description = false;
            }
            if ui.button(t(lang, "ui-cancel")).clicked() {
                st.edit.description = detail.row.task.description.clone();
                st.edit.editing_description = false;
            }
            ui.weak(t(lang, "ui-markdown-hint"));
        });
    } else if detail.row.task.description.trim().is_empty() {
        ui.weak(t(lang, "ui-no-description"));
    } else {
        md.show(ui, &detail.row.task.description);
    }
}

fn items(
    ui: &mut Ui,
    key: &str,
    list: &[CheckItem],
    kind: ItemKind,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let (title, prefix) = match kind {
        ItemKind::Checklist => ("ui-checklist", "checklist"),
        ItemKind::Criteria => ("ui-criteria", "criteria"),
    };
    let done = list.iter().filter(|i| i.done).count();
    egui::CollapsingHeader::new(format!("{} ({done}/{})", t(lang, title), list.len()))
        .id_salt((title, key))
        .default_open(true)
        .show(ui, |ui| {
            for (i, item) in list.iter().enumerate() {
                let n = (i + 1).to_string();
                ui.horizontal(|ui| {
                    let mut checked = item.done;
                    if ui.checkbox(&mut checked, "").changed() {
                        let id: &'static str = if prefix == "checklist" {
                            "checklist.check"
                        } else {
                            "criteria.check"
                        };
                        out.push(Intent::run(
                            id,
                            &[
                                ("task", key),
                                ("item", &n),
                                ("done", if checked { "true" } else { "false" }),
                            ],
                        ));
                    }
                    match st.edit.editing_item.as_mut() {
                        Some((k, idx, text)) if *k == kind && *idx == i => {
                            let edit = ui.text_edit_singleline(text);
                            let outcome = inline::outcome(ui, &edit);
                            if outcome != Outcome::Editing {
                                if outcome == Outcome::Submit && !text.trim().is_empty() {
                                    let id: &'static str = if prefix == "checklist" {
                                        "checklist.edit"
                                    } else {
                                        "criteria.edit"
                                    };
                                    out.push(Intent::run(
                                        id,
                                        &[("task", key), ("item", &n), ("text", text.trim())],
                                    ));
                                }
                                st.edit.editing_item = None;
                            }
                        }
                        _ => {
                            let mut text = RichText::new(&item.text);
                            if item.done {
                                text = text.strikethrough().weak();
                            }
                            if ui
                                .add(egui::Label::new(text).sense(egui::Sense::click()))
                                .double_clicked()
                            {
                                st.edit.editing_item = Some((kind, i, item.text.clone()));
                            }
                        }
                    }
                    if icons::button_sized(ui, Icon::Trash, true, &t(lang, "ui-remove"), 14.0)
                        .clicked()
                    {
                        let remove: &'static str = if prefix == "checklist" {
                            "checklist.remove"
                        } else {
                            "criteria.remove"
                        };
                        let add: &'static str = if prefix == "checklist" {
                            "checklist.add"
                        } else {
                            "criteria.add"
                        };
                        out.push(Intent::Delete {
                            id: remove,
                            args: engine::Args::none().with("task", key).with("item", &n),
                            undo: (
                                add,
                                engine::Args::none()
                                    .with("task", key)
                                    .with("text", &item.text),
                            ),
                            message: Msg::new("ui-item-removed").arg("item", &item.text),
                        });
                    }
                });
            }
            let buffer = match kind {
                ItemKind::Checklist => &mut st.edit.new_check,
                ItemKind::Criteria => &mut st.edit.new_criterion,
            };
            let edit =
                ui.add(egui::TextEdit::singleline(buffer).hint_text(t(lang, "ui-add-item-hint")));
            if edit.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !buffer.trim().is_empty()
            {
                let id: &'static str = if prefix == "checklist" {
                    "checklist.add"
                } else {
                    "criteria.add"
                };
                out.push(Intent::run(id, &[("task", key), ("text", buffer.trim())]));
                buffer.clear();
            }
        });
}

fn task_ref(ui: &mut Ui, r: &TaskRef, lang: Lang, out: &mut Vec<Intent>) {
    common_ref(ui, r, lang, out);
}

fn common_ref(ui: &mut Ui, r: &TaskRef, lang: Lang, out: &mut Vec<Intent>) {
    icons::dot(ui, theme::status(r.status)).on_hover_text(t(lang, r.status.label_key()));
    ui.label(RichText::new(&r.key).monospace().weak());
    let mut text = RichText::new(&r.title);
    if r.status.category().is_closed() {
        text = text.strikethrough().weak();
    }
    if ui.link(text).clicked() {
        out.push(Intent::Select(Some(r.id)));
    }
}

fn subtasks(
    ui: &mut Ui,
    detail: &Detail,
    snap: &Snapshot,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let key = &detail.row.key;
    egui::CollapsingHeader::new(format!(
        "{} ({})",
        t(lang, "ui-subtasks"),
        detail.subtasks.len()
    ))
    .id_salt(("subtasks", key))
    .default_open(true)
    .show(ui, |ui| {
        // Sacar una subtarea la sube un nivel: queda al lado de esta tarea.
        let up = detail.row.parent_key.as_deref();
        for s in &detail.subtasks {
            ui.horizontal(|ui| {
                task_ref(ui, s, lang, out);
                if icons::button_sized(ui, Icon::Left, true, &t(lang, "ui-detach-subtask"), 16.0)
                    .clicked()
                {
                    out.push(hierarchy::set_parent(&s.key, up));
                }
            });
        }
        ui.menu_button(t(lang, "ui-adopt-subtask"), |ui| {
            let candidates = hierarchy::child_candidates(snap, detail.row.id(), &st.parent_query);
            if let Some(child) = hierarchy::task_picker(ui, &candidates, &mut st.parent_query, lang)
            {
                out.push(hierarchy::set_parent(&child, Some(key)));
                st.parent_query.clear();
                ui.close();
            }
        });
        let (edit, add) = ui
            .horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut st.edit.new_subtask)
                        .hint_text(t(lang, "ui-new-subtask-hint")),
                );
                let empty = st.edit.new_subtask.trim().is_empty();
                let add = ui.add_enabled(!empty, egui::Button::new(t(lang, "ui-add")));
                (edit, add.clicked())
            })
            .inner;
        if std::mem::take(&mut st.focus_subtask) {
            edit.request_focus();
            edit.scroll_to_me(Some(egui::Align::Center));
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (enter || add) && !st.edit.new_subtask.trim().is_empty() {
            out.push(Intent::run(
                "task.create",
                &[("parent", key), ("title", st.edit.new_subtask.trim())],
            ));
            st.edit.new_subtask.clear();
        }
    });
}

fn relations(ui: &mut Ui, detail: &Detail, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let key = &detail.row.key;
    egui::CollapsingHeader::new(format!(
        "{} ({} / {})",
        t(lang, "ui-dependencies"),
        detail.blocked_by.len(),
        detail.blocks.len()
    ))
    .id_salt(("deps", key))
    .default_open(!detail.blocked_by.is_empty() || !detail.blocks.is_empty())
    .show(ui, |ui| {
        ui.weak(t(lang, "ui-blocked-by"));
        for b in &detail.blocked_by {
            ui.horizontal(|ui| {
                task_ref(ui, b, lang, out);
                if icons::button_sized(ui, Icon::Close, true, &t(lang, "ui-unlink"), 12.0).clicked()
                {
                    out.push(Intent::Delete {
                        id: "task.unlink",
                        args: engine::Args::none()
                            .with("blocker", &b.key)
                            .with("blocked", key),
                        undo: (
                            "task.link",
                            engine::Args::none()
                                .with("blocker", &b.key)
                                .with("blocked", key),
                        ),
                        message: Msg::new("ui-unlinked")
                            .arg("blocker", &b.key)
                            .arg("blocked", key),
                    });
                }
            });
        }
        let mut pick: Option<String> = None;
        egui::ComboBox::from_id_salt(("add-blocker", key))
            .selected_text(t(lang, "ui-add-blocker"))
            .width(220.0)
            .show_ui(ui, |ui| {
                for o in &detail.blocker_options {
                    if !detail.blocked_by.iter().any(|b| b.id == o.id)
                        && ui
                            .selectable_label(false, format!("{} {}", o.key, o.title))
                            .clicked()
                    {
                        pick = Some(o.key.clone());
                    }
                }
            });
        if let Some(blocker) = pick {
            out.push(Intent::run(
                "task.link",
                &[("blocker", &blocker), ("blocked", key)],
            ));
        }
        ui.add_space(4.0);
        ui.weak(t(lang, "ui-blocks"));
        for b in &detail.blocks {
            ui.horizontal(|ui| task_ref(ui, b, lang, out));
        }
    });
}

fn comments(
    ui: &mut Ui,
    detail: &Detail,
    st: &mut UiState,
    md: &mut Markdown,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let key = &detail.row.key;
    egui::CollapsingHeader::new(format!(
        "{} ({})",
        t(lang, "ui-comments"),
        detail.comments.len()
    ))
    .id_salt(("comments", key))
    .default_open(true)
    .show(ui, |ui| {
        for c in &detail.comments {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(4.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        common::who(ui, &c.author, 16.0);
                        ui.weak(common::ago(lang, c.at));
                    });
                    md.show(ui, &c.body);
                });
            ui.add_space(4.0);
        }
        ui.add(
            egui::TextEdit::multiline(&mut st.edit.comment)
                .hint_text(t(lang, "ui-comment-hint"))
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                !st.edit.comment.trim().is_empty(),
                egui::Button::new(t(lang, "ui-comment")),
            )
            .clicked()
        {
            out.push(Intent::run(
                "comment.add",
                &[("task", key), ("body", st.edit.comment.trim())],
            ));
            st.edit.comment.clear();
        }
    });
}

fn history(ui: &mut Ui, detail: &Detail, lang: Lang, out: &mut Vec<Intent>) {
    egui::CollapsingHeader::new(format!(
        "{} ({})",
        t(lang, "ui-history"),
        detail.history.len()
    ))
    .id_salt(("history", &detail.row.key))
    .default_open(false)
    .show(ui, |ui| {
        for line in &detail.history {
            ui.scope(|ui| {
                crate::app::activity_line_with_revert(ui, lang, line, out);
            });
        }
    });
}

fn actions(ui: &mut Ui, detail: &Detail, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let row = &detail.row;
    let key = row.key.as_str();
    ui.horizontal_wrapped(|ui| {
        if row.task.archived_at.is_some() {
            if ui.button(t(lang, "ui-unarchive")).clicked() {
                out.push(Intent::run("task.unarchive", &[("task", key)]));
            }
        } else if row.is_closed() && ui.button(t(lang, "ui-archive")).clicked() {
            out.push(Intent::run("task.archive", &[("task", key)]));
        }
        let (lock_text, lock_cmd) = if row.task.locked {
            ("ui-unlock", "lock.clear")
        } else {
            ("ui-lock", "lock.set")
        };
        if ui.button(t(lang, lock_text)).clicked() {
            out.push(Intent::run(lock_cmd, &[("task", key)]));
        }
        // RF-VIS-08: elegirla para hoy.
        if !row.is_closed() {
            let today = snap.today.contains(&row.id());
            let text = if today {
                "ui-today-unmark"
            } else {
                "ui-today-mark"
            };
            if ui.button(t(lang, text)).clicked() {
                out.push(crate::views::today::toggle(snap, row));
            }
        }
        if ui
            .button(RichText::new(t(lang, "ui-delete-task")).color(theme::RED))
            .clicked()
        {
            out.push(crate::views::list::delete_intent(key));
        }
    });
}

/// Asignados con su avatar (la función del agente, al pasar por encima) y
/// un menú para sumar o quitar agentes.
fn assignees(
    ui: &mut Ui,
    detail: &Detail,
    key: &str,
    snap: &Snapshot,
    lang: Lang,
    out: &mut Vec<Intent>,
) {
    let task = &detail.row.task;
    ui.horizontal_wrapped(|ui| {
        for (actor, who) in task.assignees.iter().zip(&detail.row.assignees) {
            // Avatar, nombre y quitar van juntos al partirse la línea.
            ui.horizontal(|ui| {
                common::who_avatar(ui, who, 18.0);
                ui.label(&who.name).on_hover_text(who.hover());
                if icons::button_sized(ui, Icon::Close, true, &t(lang, "ui-remove"), 14.0).clicked()
                {
                    out.push(Intent::run(
                        "task.remove_assignee",
                        &[("task", key), ("agent", &actor.to_string())],
                    ));
                }
            });
            ui.add_space(6.0);
        }
        if task.assignees.is_empty() {
            ui.weak(t(lang, "ui-unassigned"));
        }
    });
    ui.menu_button(t(lang, "ui-assign-agents"), |ui| {
        common::assignee_checks(ui, task, key, snap, out);
    });
}
