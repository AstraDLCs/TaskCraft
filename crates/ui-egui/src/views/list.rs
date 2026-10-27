//! Lista jerárquica (RF-VIS-01): tareas con subtareas plegables a cualquier
//! profundidad, edición en la fila y subtareas nuevas en el sitio.

use eframe::egui::{self, RichText, Ui};
use i18n::{Msg, t};

use super::common;
use crate::data::{Snapshot, TaskRow};
use crate::hierarchy::{self, Reparent};
use crate::state::{Intent, UiState, list_rows};
use crate::widgets::date_picker;
use crate::widgets::icons::{self, Icon};
use crate::widgets::inline::{self, Outcome};

const INDENT: f32 = 18.0;

/// Anchos de las columnas tras el título; el título ocupa lo que queda.
const COLUMNS: [(&str, f32); 8] = [
    ("ui-col-status", 130.0),
    ("ui-col-priority", 110.0),
    ("ui-col-assignee", 130.0),
    ("ui-col-due", 110.0),
    ("ui-col-labels", 150.0),
    ("ui-col-progress", 50.0),
    ("", 64.0),
    ("", 52.0),
];
const MIN_TITLE: f32 = 240.0;
/// Columnas que se quitan, en orden, cuando no caben: etiquetas, progreso,
/// asignado, prioridad y vencimiento. Estado y acciones se quedan siempre.
const DROP_ORDER: [usize; 5] = [4, 5, 2, 1, 3];

/// Qué columnas caben en `width` junto a un título de al menos `MIN_TITLE`.
fn visible_columns(width: f32, spacing: f32) -> [bool; 8] {
    let mut on = [true; 8];
    let need = |on: &[bool; 8]| -> f32 {
        COLUMNS
            .iter()
            .zip(on)
            .filter(|(_, v)| **v)
            .map(|((_, w), _)| w + spacing)
            .sum::<f32>()
            + MIN_TITLE
            + 16.0
    };
    for i in DROP_ORDER {
        if need(&on) <= width {
            break;
        }
        if let Some(column) = on.get_mut(i) {
            *column = false;
        }
    }
    on
}

/// Una línea de la lista: una tarea o la caja para escribir una subtarea nueva
/// justo debajo de su padre.
enum Line<'a> {
    Task(&'a TaskRow, bool),
    NewChild(&'a TaskRow),
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let rows = list_rows(snap, st);
    if rows.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-no-tasks")));
        return;
    }
    let mut lines = Vec::with_capacity(rows.len() + 1);
    for (row, direct) in rows {
        lines.push(Line::Task(row, direct));
        if st.add_child.as_ref().is_some_and(|(p, _)| *p == row.id()) {
            lines.push(Line::NewChild(row));
        }
    }
    root_drop_zone(ui, lang, out);
    let spacing = ui.spacing().item_spacing.x;
    let on = visible_columns(ui.available_width(), spacing);
    let shown = || COLUMNS.iter().zip(on).filter(|(_, v)| *v).map(|(c, _)| *c);
    let fixed: f32 = shown().map(|(_, w)| w + spacing).sum();
    let title_w = (ui.available_width() - fixed - 16.0).max(MIN_TITLE);
    let row_h = ui.spacing().interact_size.y + 6.0;

    egui::ScrollArea::horizontal()
        .id_salt("list-h")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                cell(ui, title_w, row_h, |ui| {
                    ui.strong(t(lang, "ui-col-title"));
                });
                for (key, w) in shown() {
                    cell(ui, w, row_h, |ui| {
                        if !key.is_empty() {
                            ui.strong(t(lang, key));
                        }
                    });
                }
            });
            ui.separator();
            // Solo se dibujan las líneas visibles: 50 000 tareas siguen a 60 fps (RNF-06).
            egui::ScrollArea::vertical().id_salt("list").show_rows(
                ui,
                row_h,
                lines.len(),
                |ui, range| {
                    for (i, line) in lines.iter().enumerate().skip(range.start).take(range.len()) {
                        let rect = ui.available_rect_before_wrap();
                        if i % 2 == 1 {
                            let stripe = egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(title_w + fixed + spacing, row_h),
                            );
                            ui.painter()
                                .rect_filled(stripe, 0.0, ui.visuals().faint_bg_color);
                        }
                        match line {
                            Line::Task(row, direct) => {
                                task_line(ui, snap, st, out, row, *direct, title_w, row_h, on)
                            }
                            Line::NewChild(row) => new_child_line(ui, st, out, row, row_h),
                        }
                    }
                },
            );
        });
}

/// Celda de ancho fijo, centrada en vertical.
fn cell(ui: &mut Ui, width: f32, height: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(egui::vec2(width, height));
            ui.set_max_width(width);
            add(ui);
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn task_line(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    out: &mut Vec<Intent>,
    row: &TaskRow,
    direct: bool,
    title_w: f32,
    row_h: f32,
    on: [bool; 8],
) {
    let lang = st.lang;
    let id = row.id();
    let key = row.key.clone();
    ui.horizontal(|ui| {
        // Título con sangría, flecha de plegado y clave (que se arrastra
        // sobre otra fila para colgarla de ella).
        let title = ui.allocate_ui_with_layout(
            egui::vec2(title_w, row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(egui::vec2(title_w, row_h));
                ui.set_max_width(title_w);
                ui.add_space(row.depth as f32 * INDENT);
                if row.children_total > 0 {
                    let collapsed = st.collapsed.contains(&id);
                    let icon = if collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    };
                    let tip = t(
                        lang,
                        if collapsed {
                            "ui-expand"
                        } else {
                            "ui-collapse"
                        },
                    );
                    if icons::button_sized(ui, icon, true, &tip, 16.0).clicked() {
                        if collapsed {
                            st.collapsed.remove(&id);
                        } else {
                            st.collapsed.insert(id);
                        }
                    }
                } else {
                    ui.add_space(20.0);
                }
                let payload = Reparent {
                    id,
                    key: key.clone(),
                };
                ui.dnd_drag_source(egui::Id::unique(("list-drag", id)), payload, |ui| {
                    ui.label(RichText::new(&key).monospace().weak())
                        .on_hover_cursor(egui::CursorIcon::Grab)
                        .on_hover_text(t(lang, "ui-drag-to-nest"));
                });
                let selected = st.selected == Some(id);
                if direct {
                    common::title_link(ui, row, selected, out);
                } else if ui
                    // Ancestro mostrado solo para dar contexto al filtro.
                    .selectable_label(selected, RichText::new(&row.task.title).weak())
                    .clicked()
                {
                    out.push(Intent::Select(Some(id)));
                }
            },
        );
        let rect = title.response.rect;
        if let (true, dropped) = hierarchy::drop_on(ui, rect) {
            let dragged = egui::DragAndDrop::payload::<Reparent>(ui.ctx());
            if dragged.is_some_and(|d| d.id != id) {
                ui.painter().rect_stroke(
                    rect.expand(2.0),
                    3.0,
                    egui::Stroke::new(1.5, ui.visuals().selection.stroke.color),
                    egui::StrokeKind::Outside,
                );
            }
            if let Some(d) = dropped.filter(|d| d.id != id) {
                out.push(hierarchy::set_parent(&d.key, Some(&key)));
            }
        }
        title
            .response
            .container_context_menu(|ui| hierarchy::context_menu(ui, snap, row, st, out));

        let [
            status_w,
            priority_w,
            assignee_w,
            due_w,
            labels_w,
            progress_w,
            badges_w,
            actions_w,
        ] = COLUMNS.map(|(_, w)| w);
        cell(ui, status_w, row_h, |ui| {
            if let Some(s) = crate::flow::status_combo(ui, ("st", id), &row.task, snap, lang) {
                out.push(Intent::run("task.move", &[("task", &key), ("status", &s)]));
            }
        });
        if on[1] {
            cell(ui, priority_w, row_h, |ui| {
                if let Some(p) = common::priority_combo(ui, ("pr", id), row.task.priority, lang) {
                    out.push(Intent::run(
                        "task.update",
                        &[("task", &key), ("priority", p.as_str())],
                    ));
                }
            });
        }
        if on[2] {
            cell(ui, assignee_w, row_h, |ui| {
                common::assignees_combo(ui, ("as", id), &row.task, &key, snap, lang, out);
            });
        }
        if on[3] {
            cell(ui, due_w, row_h, |ui| {
                let due = row.task.due.as_ref().map(|d| d.date());
                if let Some(new) = date_picker::date_button(ui, ("due", id), due, lang) {
                    let value = new.map(|d| d.to_string()).unwrap_or_default();
                    out.push(Intent::run(
                        "task.update",
                        &[("task", &key), ("due", &value)],
                    ));
                }
            });
        }
        if on[4] {
            cell(ui, labels_w, row_h, |ui| {
                for (name, color) in &row.labels {
                    crate::widgets::chip::chip(ui, name, *color);
                }
            });
        }
        if on[5] {
            cell(ui, progress_w, row_h, |ui| {
                ui.label(RichText::new(common::progress(row).unwrap_or_default()).small());
            });
        }
        cell(ui, badges_w, row_h, |ui| common::badges(ui, row, lang));
        cell(ui, actions_w, row_h, |ui| {
            if icons::button(ui, Icon::Plus, true, &t(lang, "ui-add-subtask")).clicked() {
                st.add_child = Some((id, String::new()));
                st.collapsed.remove(&id);
            }
            if icons::button(ui, Icon::Trash, true, &t(lang, "ui-delete-task")).clicked() {
                out.push(delete_intent(&key));
            }
        });
    });
}

/// Caja para escribir una subtarea nueva justo debajo de su padre.
fn new_child_line(
    ui: &mut Ui,
    st: &mut UiState,
    out: &mut Vec<Intent>,
    parent: &TaskRow,
    row_h: f32,
) {
    let lang = st.lang;
    let Some((_, text)) = st.add_child.as_mut() else {
        return;
    };
    let mut done = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        ui.set_height(row_h);
        ui.add_space((parent.depth + 1) as f32 * INDENT + 20.0);
        let edit = ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(t(lang, "ui-new-subtask-hint"))
                .desired_width(340.0),
        );
        match inline::outcome(ui, &edit) {
            Outcome::Submit if !text.trim().is_empty() => done = true,
            Outcome::Editing => {}
            _ => cancel = true,
        }
    });
    if done {
        out.push(Intent::run(
            "task.create",
            &[("parent", &parent.key), ("title", text.trim())],
        ));
    }
    if done || cancel {
        st.add_child = None;
    }
}

/// Franja que aparece al arrastrar una fila: soltarla aquí la deja sin padre.
/// Se dibuja encima de los encabezados para que la lista no se desplace bajo
/// el cursor al empezar a arrastrar.
fn root_drop_zone(ui: &mut Ui, lang: i18n::Lang, out: &mut Vec<Intent>) {
    if !egui::DragAndDrop::has_payload_of_type::<Reparent>(ui.ctx()) {
        return;
    }
    let top = ui.available_rect_before_wrap();
    let rect = egui::Rect::from_min_size(top.min, egui::vec2(top.width(), 30.0));
    let (hovering, dropped) = hierarchy::drop_on(ui, rect);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        ui.scope_id().with("root-drop"),
    ));
    let visuals = ui.visuals();
    let fill = if hovering {
        visuals.selection.bg_fill
    } else {
        visuals.window_fill
    };
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, visuals.selection.stroke.color),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        t(lang, "ui-drop-to-make-root"),
        egui::TextStyle::Body.resolve(ui.style()),
        visuals.text_color(),
    );
    if let Some(d) = dropped {
        out.push(hierarchy::set_parent(&d.key, None));
    }
}

pub fn delete_intent(key: &str) -> Intent {
    Intent::Delete {
        id: "task.delete",
        args: engine::Args::none().with("task", key),
        undo: ("task.restore", engine::Args::none().with("task", key)),
        message: Msg::new("ui-task-deleted").arg("task", key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_lists_drop_the_least_needed_columns_first() {
        assert_eq!(visible_columns(2000.0, 8.0), [true; 8]);
        let mid = visible_columns(900.0, 8.0);
        assert!(!mid[4] && !mid[5], "sin etiquetas ni progreso");
        assert!(mid[0] && mid[3], "estado y vencimiento se quedan");
        let narrow = visible_columns(500.0, 8.0);
        assert_eq!(
            narrow,
            [true, false, false, false, false, false, true, true]
        );
    }
}
