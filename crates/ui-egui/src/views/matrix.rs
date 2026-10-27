//! Matriz de Eisenhower (RF-VIS-15): las tareas abiertas en cuatro cuadrantes
//! según su prioridad (importante) y su vencimiento (urgente). Arrastrar una
//! tarjeta entre la fila de arriba y la de abajo cambia su prioridad.

use eframe::egui::{self, RichText, Ui};
use i18n::{Msg, t, tr};
use model::Status;
use rules::matrix::{Quadrant, priority_for, quadrant};

use super::common;
use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, UiState, flat_rows};
use crate::theme;

/// Lo que viaja al arrastrar una tarjeta.
#[derive(Clone)]
struct Drag {
    key: String,
    priority: model::Priority,
}

/// Ancho mínimo de un cuadrante para poner dos en la misma fila.
const MIN_QUADRANT: f32 = 300.0;

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let urgent_days = snap
        .workspace
        .as_ref()
        .map_or(3, |w| w.settings.urgent_days);
    let today = crate::widgets::date_picker::today();
    let rows: Vec<&TaskRow> = flat_rows(snap, st, false)
        .into_iter()
        .filter(|r| !r.is_closed() && r.task.status != Status::Backlog)
        .collect();
    ui.weak(tr(
        lang,
        &Msg::new("ui-matrix-hint").arg("days", u64::from(urgent_days)),
    ));
    // Dos cuadrantes por fila si caben; si no, uno debajo de otro.
    let per_row = if ui.available_width() >= 2.0 * MIN_QUADRANT * st.text_size.scale() {
        2
    } else {
        1
    };
    let width = (ui.available_width() - 12.0) / per_row as f32;
    let height = if per_row == 2 {
        ((ui.available_height() - 40.0) / 2.0).max(160.0)
    } else {
        160.0 * st.text_size.scale()
    };
    egui::ScrollArea::vertical()
        .id_salt("matrix")
        .show(ui, |ui| {
            for pair in Quadrant::ALL.chunks(per_row) {
                ui.horizontal_top(|ui| {
                    for q in pair {
                        let cards: Vec<&TaskRow> = rows
                            .iter()
                            .copied()
                            .filter(|r| quadrant(&r.task, today, urgent_days) == *q)
                            .collect();
                        cell(ui, st, *q, &cards, width, height, out);
                    }
                });
                ui.add_space(8.0);
            }
        });
}

fn color(q: Quadrant) -> egui::Color32 {
    match q {
        Quadrant::DoFirst => theme::RED,
        Quadrant::Schedule => theme::BLUE,
        Quadrant::Delegate => theme::ORANGE,
        Quadrant::Drop => theme::GRAY,
    }
}

fn cell(
    ui: &mut Ui,
    st: &UiState,
    q: Quadrant,
    cards: &[&TaskRow],
    width: f32,
    height: f32,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            crate::widgets::icons::dot(ui, color(q));
            ui.strong(t(lang, q.label_key()));
            ui.weak(cards.len().to_string());
        });
        let frame = egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6.0)
            .inner_margin(6.0);
        let (_, dropped) = ui.dnd_drop_zone::<Drag, ()>(frame, |ui| {
            ui.set_min_size(egui::vec2(width - 12.0, height));
            egui::ScrollArea::vertical()
                .id_salt(("matrix-cell", q.key()))
                .max_height(height)
                .show(ui, |ui| {
                    for row in cards {
                        card(ui, st, row, out);
                    }
                });
        });
        if let Some(drag) = dropped
            && let Some(p) = priority_for(drag.priority, q.important())
        {
            out.push(Intent::run(
                "task.update",
                &[("task", &drag.key), ("priority", p.as_str())],
            ));
        }
    });
}

fn card(ui: &mut Ui, st: &UiState, row: &TaskRow, out: &mut Vec<Intent>) {
    let selected = st.selected == Some(row.id());
    let payload = Drag {
        key: row.key.clone(),
        priority: row.task.priority,
    };
    ui.dnd_drag_source(egui::Id::unique(("matrix-card", row.id())), payload, |ui| {
        ui.horizontal(|ui| {
            crate::widgets::icons::dot(ui, theme::priority(row.task.priority));
            ui.label(RichText::new(&row.key).monospace().small().weak());
            // La fecha a la derecha y el título, recortado, en lo que quede:
            // un título largo nunca ensancha el cuadrante.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(due) = common::due_text(&row.task) {
                    ui.label(due);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let mut text = RichText::new(&row.task.title);
                    if row.is_closed() {
                        text = text.strikethrough().weak();
                    }
                    if ui
                        .add(egui::Button::selectable(selected, text).truncate())
                        .on_hover_text(&row.task.title)
                        .clicked()
                    {
                        out.push(Intent::Select(Some(row.id())));
                    }
                });
            });
        });
    });
}
