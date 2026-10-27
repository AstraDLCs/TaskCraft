//! Tabla editable (RF-VIS-05): columnas a elección, incluidos los campos
//! propios de los proyectos. Cada edición en una celda es un comando.

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, t};

use super::common;
use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, UiState, flat_rows};
use crate::widgets::date_picker;
use crate::widgets::inline::{self, Outcome};

/// Columnas fijas, en el orden del selector.
const BUILTIN: [&str; 14] = [
    "key",
    "title",
    "status",
    "priority",
    "assignee",
    "labels",
    "due",
    "start",
    "estimate",
    "epic",
    "milestone",
    "project",
    "updated",
    "progress",
];

fn width(column: &str) -> f32 {
    match column {
        "title" => 280.0,
        "key" | "progress" => 80.0,
        "labels" | "status" | "assignee" | "epic" | "milestone" => 140.0,
        _ => 115.0,
    }
}

/// Nombre visible de una columna; los campos propios, por su nombre.
pub fn column_name(snap: &Snapshot, column: &str, lang: Lang) -> String {
    if let Some(key) = column.strip_prefix("field.") {
        return snap
            .projects
            .iter()
            .flat_map(|p| &p.fields)
            .find(|f| f.key == key)
            .map_or_else(|| key.to_owned(), |f| f.name.clone());
    }
    t(
        lang,
        match column {
            "key" => "ui-col-key",
            "title" => "ui-col-title",
            "status" => "ui-col-status",
            "priority" => "ui-col-priority",
            "assignee" => "ui-col-assignee",
            "labels" => "ui-col-labels",
            "due" => "ui-col-due",
            "start" => "ui-col-start",
            "estimate" => "ui-col-estimate",
            "epic" => "ui-col-epic",
            "milestone" => "ui-col-milestone",
            "project" => "ui-col-project",
            "updated" => "ui-col-updated",
            "progress" => "ui-col-progress",
            _ => return column.to_owned(),
        },
    )
}

/// Todas las columnas que se pueden mostrar con el alcance actual.
fn available(snap: &Snapshot) -> Vec<String> {
    let mut out: Vec<String> = BUILTIN.map(str::to_owned).to_vec();
    for f in snap.projects.iter().flat_map(|p| &p.fields) {
        let code = format!("field.{}", f.key);
        if !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

pub fn toolbar(ui: &mut Ui, snap: &Snapshot, st: &mut UiState) {
    let lang = st.lang;
    ui.menu_button(t(lang, "ui-columns"), |ui| {
        for code in available(snap) {
            let mut on = st.table.columns.contains(&code);
            if ui
                .checkbox(&mut on, column_name(snap, &code, lang))
                .changed()
            {
                if on {
                    st.table.columns.push(code);
                } else {
                    st.table.columns.retain(|c| *c != code);
                }
            }
        }
    });
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let rows = flat_rows(snap, st, false);
    if rows.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-no-tasks")));
        return;
    }
    let columns = st.table.columns.clone();
    let row_h = ui.spacing().interact_size.y + 6.0;
    egui::ScrollArea::horizontal()
        .id_salt("table-h")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for c in &columns {
                    cell(ui, width(c), row_h, |ui| {
                        ui.strong(column_name(snap, c, lang));
                    });
                }
            });
            ui.separator();
            // Solo las filas visibles: miles de tareas sin perder fluidez (RNF-06).
            egui::ScrollArea::vertical().id_salt("table").show_rows(
                ui,
                row_h,
                rows.len(),
                |ui, range| {
                    for row in rows.iter().skip(range.start).take(range.len()) {
                        ui.horizontal(|ui| {
                            for c in &columns {
                                cell(ui, width(c), row_h, |ui| {
                                    value(ui, snap, st, out, row, c);
                                });
                            }
                        });
                    }
                },
            );
        });
}

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

/// Texto actual de una celda editable como texto.
fn text_of(snap: &Snapshot, row: &TaskRow, column: &str) -> String {
    let t = &row.task;
    match column {
        "title" => t.title.clone(),
        "estimate" => t
            .estimate
            .zip(snap.project(t.project_id))
            .map(|(e, p)| rules::format_estimate(p.estimate_unit, e))
            .unwrap_or_default(),
        c => c
            .strip_prefix("field.")
            .and_then(|k| t.fields.get(k).cloned())
            .unwrap_or_default(),
    }
}

/// Celda de texto: se edita con un clic y se guarda con Intro.
fn text_cell(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    out: &mut Vec<Intent>,
    row: &TaskRow,
    column: &str,
) {
    let id = row.id();
    if let Some((_, _, buffer)) = st
        .table
        .editing
        .as_mut()
        .filter(|(t, c, _)| *t == id && c == column)
    {
        let edit = ui.add(egui::TextEdit::singleline(buffer).desired_width(f32::INFINITY));
        let outcome = inline::outcome(ui, &edit);
        if outcome != Outcome::Editing {
            if outcome == Outcome::Submit {
                let value = buffer.trim().to_owned();
                out.push(match column.strip_prefix("field.") {
                    Some(field) => Intent::run(
                        "task.set_field",
                        &[("task", &row.key), ("field", field), ("value", &value)],
                    ),
                    None => Intent::run("task.update", &[("task", &row.key), (column, &value)]),
                });
            }
            st.table.editing = None;
        }
        return;
    }
    let text = text_of(snap, row, column);
    let shown = if text.is_empty() {
        "—".to_owned()
    } else {
        text.clone()
    };
    let mut rich = RichText::new(shown);
    if column == "title" && row.is_closed() {
        rich = rich.strikethrough().weak();
    }
    let response = ui
        .add(
            egui::Label::new(rich)
                .truncate()
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::Text);
    if response.clicked() {
        st.table.editing = Some((id, column.to_owned(), text));
    }
    if column == "title" && response.secondary_clicked() {
        out.push(Intent::Select(Some(id)));
    }
}

fn value(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    out: &mut Vec<Intent>,
    row: &TaskRow,
    column: &str,
) {
    let lang = st.lang;
    let id = row.id();
    let key = &row.key;
    let t = &row.task;
    let group = |g: Option<model::EpicId>| {
        g.and_then(|g| snap.epic(g))
            .map(|e| e.epic.name.clone())
            .unwrap_or_default()
    };
    match column {
        "key" => {
            if ui
                .selectable_label(st.selected == Some(id), RichText::new(key).monospace())
                .clicked()
            {
                out.push(Intent::Select(Some(id)));
            }
        }
        "title" | "estimate" => text_cell(ui, snap, st, out, row, column),
        c if c.starts_with("field.") => text_cell(ui, snap, st, out, row, column),
        "status" => {
            if let Some(s) = crate::flow::status_combo(ui, ("tst", id), t, snap, lang) {
                out.push(Intent::run("task.move", &[("task", key), ("status", &s)]));
            }
        }
        "priority" => {
            if let Some(p) = common::priority_combo(ui, ("tpr", id), t.priority, lang) {
                out.push(Intent::run(
                    "task.update",
                    &[("task", key), ("priority", p.as_str())],
                ));
            }
        }
        "assignee" => {
            common::assignees_combo(ui, ("tas", id), t, key, snap, lang, out);
        }
        "due" | "start" => {
            let current = if column == "due" { &t.due } else { &t.start };
            let day = current.as_ref().map(model::When::date);
            if let Some(new) = date_picker::date_button(ui, (column, id), day, lang) {
                let v = new.map(|d| d.to_string()).unwrap_or_default();
                out.push(Intent::run("task.update", &[("task", key), (column, &v)]));
            }
        }
        "labels" => {
            for (name, color) in &row.labels {
                crate::widgets::chip::chip(ui, name, *color);
            }
        }
        "epic" => {
            ui.label(group(t.epic_id));
        }
        "milestone" => {
            ui.label(group(t.milestone_id));
        }
        "project" => {
            ui.label(&row.project_key);
        }
        "updated" => {
            ui.weak(common::ago(lang, t.updated_at));
        }
        "progress" => {
            ui.label(common::progress(row).unwrap_or_default());
        }
        _ => {}
    }
}
