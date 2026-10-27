//! Tablero kanban (RF-VIS-02) con arrastrar y soltar entre las columnas del
//! flujo (D10) y carriles por asignado, prioridad, proyecto o épica (RF-VIS-06).

use std::collections::BTreeMap;

use super::common;
use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, Lanes, UiState, flat_rows};
use crate::theme;
use crate::widgets::chip;
use eframe::egui::{self, RichText, Ui};
use i18n::t;

/// Ancho de columna: se reparte el sitio entre ellas dentro de estos límites
/// (por debajo, el tablero se desplaza en horizontal).
const COLUMN_MIN: f32 = 210.0;
const COLUMN_MAX: f32 = 340.0;

/// Ancho de cada una de `n` columnas en `available`, a la escala del texto.
fn column_width(available: f32, n: usize, spacing: f32, scale: f32) -> f32 {
    let n = n.max(1) as f32;
    ((available - spacing * n) / n).clamp(COLUMN_MIN * scale, COLUMN_MAX * scale)
}

/// Lo que viaja al arrastrar una tarjeta: su clave y la columna de origen.
#[derive(Clone)]
pub struct Drag {
    pub key: String,
    pub column: String,
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let rows = flat_rows(snap, st, st.roots_only);
    let lanes = group(snap, &rows, st.lanes, lang);
    // Un proyecto: sus columnas, con etapas. Varios: los 6 estados base.
    let project = match st.scope {
        crate::data::Scope::Project(id) => Some(id),
        crate::data::Scope::All => None,
    };
    let columns = crate::flow::columns(snap, project, lang);
    let stages = project.is_some();
    let shown = columns
        .iter()
        .filter(|c| !(c.base.category().is_closed() && st.filter.hide_closed))
        .count();
    let width = column_width(
        ui.available_width(),
        shown,
        ui.spacing().item_spacing.x,
        st.text_size.scale(),
    );
    // Sin carriles, las columnas llegan hasta abajo y cada una se desplaza por
    // su cuenta, con la cabecera siempre a la vista. Con carriles, cada carril
    // es una fila de su altura y es el tablero entero el que se desplaza.
    let fill = (st.lanes == Lanes::None).then(|| {
        ui.available_height() - crate::layout::scrollbar(ui) - ui.spacing().item_spacing.y
    });
    let scroll = if fill.is_some() {
        egui::ScrollArea::horizontal()
    } else {
        egui::ScrollArea::both()
    };
    scroll
        .id_salt("board")
        // Lo que no cabe a la derecha se anuncia con su barra siempre visible.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            for (i, (lane_name, lane_rows)) in lanes.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                if fill.is_none() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.strong(lane_name);
                        ui.weak(format!("({})", lane_rows.len()));
                    });
                }
                ui.horizontal_top(|ui| {
                    for col in &columns {
                        if col.base.category().is_closed() && st.filter.hide_closed {
                            continue;
                        }
                        let place = Place {
                            lane: lane_name,
                            width,
                            fill,
                        };
                        column(ui, snap, st, col, stages, lane_rows, place, out);
                    }
                });
            }
        });
}

/// Dónde va una columna: su carril, su ancho y, sin carriles, el alto que
/// debe llenar.
#[derive(Clone, Copy)]
struct Place<'a> {
    lane: &'a str,
    width: f32,
    fill: Option<f32>,
}

#[allow(clippy::too_many_arguments)]
fn column(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    col: &crate::flow::Column,
    stages: bool,
    rows: &[&TaskRow],
    place: Place<'_>,
    out: &mut Vec<Intent>,
) {
    let Place { lane, width, fill } = place;
    let cards: Vec<&&TaskRow> = rows.iter().filter(|r| col.holds(&r.task, stages)).collect();
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            crate::widgets::icons::dot(ui, theme::status(col.base));
            // La columna de un agente lleva su avatar (D20).
            if let Some(owner) = col.owner {
                common::agent_avatar(ui, snap, owner, 16.0);
            }
            ui.strong(&col.name);
            match col.wip_limit {
                // Por encima del límite, en rojo (RF-COO-08).
                Some(limit) if cards.len() > limit as usize => {
                    ui.label(RichText::new(format!("{}/{limit}", cards.len())).color(theme::RED));
                }
                Some(limit) => {
                    ui.weak(format!("{}/{limit}", cards.len()));
                }
                None => {
                    ui.weak(cards.len().to_string());
                }
            }
        });
        let margin = 6.0;
        let frame = egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6.0)
            .inner_margin(margin);
        // Lo que queda hasta abajo, descontada la cabecera ya dibujada.
        let height = fill.map(|h| (h - ui.min_rect().height() - margin * 2.0).max(60.0));
        let (_, dropped) = ui.dnd_drop_zone::<Drag, ()>(frame, |ui| {
            ui.set_min_size(egui::vec2(width - 12.0, height.unwrap_or(60.0)));
            let mut cards_ui = |ui: &mut Ui| {
                for row in &cards {
                    card(ui, snap, st, row, lane, width, stages, out);
                    ui.add_space(4.0);
                }
            };
            match height {
                Some(h) => {
                    egui::ScrollArea::vertical()
                        .id_salt(("board-column", lane, &col.key))
                        .max_height(h)
                        .auto_shrink([true, false])
                        .show(ui, |ui| cards_ui(ui));
                }
                None => cards_ui(ui),
            }
        });
        // Si la transición no es válida, el engine la rechaza y explica por qué.
        if let Some(drag) = dropped
            && drag.column != col.key
        {
            out.push(Intent::run(
                "task.move",
                &[("task", &drag.key), ("status", &col.key)],
            ));
        }
    });
}

/// Quien la tiene tomada y el resto de asignados, hasta tres avatares y
/// cuántos más quedan (dibujado de derecha a izquierda).
fn assignee_avatars(ui: &mut Ui, row: &TaskRow) {
    const SHOWN: usize = 3;
    let holder = row.claim.as_ref().map(|c| &c.agent);
    let mut people: Vec<&crate::data::Who> = holder.into_iter().collect();
    people.extend(
        row.assignees
            .iter()
            .filter(|w| holder.is_none_or(|h| h.seed != w.seed)),
    );
    if people.len() > SHOWN {
        ui.label(
            RichText::new(format!("+{}", people.len() - SHOWN))
                .small()
                .weak(),
        );
    }
    for who in people.iter().take(SHOWN).rev() {
        common::who_avatar(ui, who, 18.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn card(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    row: &TaskRow,
    lane: &str,
    width: f32,
    stages: bool,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let selected = st.selected == Some(row.id());
    let drag_id = egui::Id::unique(("card", row.id(), lane));
    let column = match (&row.task.stage, stages) {
        (Some(stage), true) => stage.clone(),
        _ => row.task.status.as_str().to_owned(),
    };
    let payload = Drag {
        key: row.key.clone(),
        column,
    };
    let inner = ui.dnd_drag_source(drag_id, payload, |ui| {
        let stroke = if selected {
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color)
        } else {
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
        };
        egui::Frame::new()
            .fill(ui.visuals().window_fill)
            .stroke(stroke)
            .corner_radius(6.0)
            .inner_margin(egui::Margin {
                left: 10,
                right: 8,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.set_width(width - 30.0);
                ui.horizontal(|ui| {
                    crate::widgets::icons::dot(ui, theme::priority(row.task.priority));
                    ui.label(RichText::new(&row.key).monospace().small().weak());
                    if let Some(parent) = &row.parent_key {
                        let text =
                            i18n::tr(lang, &i18n::Msg::new("ui-subtask-of").arg("parent", parent));
                        ui.label(RichText::new(text).small().weak());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        assignee_avatars(ui, row);
                        common::badges(ui, row, lang);
                    });
                });
                let mut title = RichText::new(&row.task.title);
                if row.is_closed() {
                    title = title.strikethrough().weak();
                }
                ui.add(egui::Label::new(title).wrap());
                // Con varios proyectos las columnas son las base: la etapa va en la tarjeta.
                if let (Some(_), false) = (&row.task.stage, stages) {
                    ui.label(
                        RichText::new(crate::flow::status_name(snap, &row.task, lang))
                            .small()
                            .color(theme::status(row.task.status)),
                    );
                }
                if !row.labels.is_empty()
                    || row.task.due.is_some()
                    || common::progress(row).is_some()
                {
                    ui.horizontal_wrapped(|ui| {
                        for (name, color) in &row.labels {
                            chip::chip(ui, name, *color);
                        }
                        if let Some(due) = common::due_text(&row.task) {
                            ui.label(due);
                        }
                        if let Some(p) = common::progress(row) {
                            ui.label(RichText::new(p).small().weak());
                        }
                        if row.comments > 0 {
                            crate::widgets::icons::show(
                                ui,
                                crate::widgets::icons::Icon::Comment,
                                theme::GRAY,
                            );
                            ui.label(RichText::new(row.comments.to_string()).small().weak());
                        }
                    });
                }
                if let Some(claim) = &row.claim {
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            claim.agent.name,
                            common::remaining(lang, claim.until)
                        ))
                        .small()
                        .color(theme::ORANGE),
                    );
                }
            });
    });
    let click = ui.interact(
        inner.response.rect,
        drag_id.with("click"),
        egui::Sense::click(),
    );
    if click.clicked() {
        out.push(Intent::Select(Some(row.id())));
    }
    click.context_menu(|ui| crate::hierarchy::context_menu(ui, snap, row, st, out));
}

/// Agrupa las tarjetas en carriles, en un orden estable.
fn group<'a>(
    snap: &Snapshot,
    rows: &[&'a TaskRow],
    lanes: Lanes,
    lang: i18n::Lang,
) -> Vec<(String, Vec<&'a TaskRow>)> {
    if lanes == Lanes::None {
        return vec![(String::new(), rows.to_vec())];
    }
    let mut map: BTreeMap<(u8, String), Vec<&TaskRow>> = BTreeMap::new();
    for &row in rows {
        let key = match lanes {
            Lanes::Assignee => match row.assignee.as_ref() {
                Some(w) => (1, w.name.clone()),
                None => (0, t(lang, "ui-unassigned")),
            },
            Lanes::Priority => (
                row.task.priority.rank(),
                t(lang, row.task.priority.label_key()),
            ),
            Lanes::Project => (0, row.project_key.clone()),
            Lanes::Epic => match row.task.epic_id.and_then(|id| snap.epic(id)) {
                Some(e) => (0, e.epic.name.clone()),
                None => (1, t(lang, "ui-no-epic")),
            },
            Lanes::None => (0, String::new()),
        };
        map.entry(key).or_default().push(row);
    }
    map.into_iter()
        .map(|((_, name), rows)| (name, rows))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_share_the_width_within_limits() {
        assert_eq!(column_width(1400.0, 6, 8.0, 1.0), (1400.0 - 48.0) / 6.0);
        assert_eq!(column_width(600.0, 6, 8.0, 1.0), COLUMN_MIN);
        assert_eq!(column_width(3000.0, 3, 8.0, 1.0), COLUMN_MAX);
        assert_eq!(column_width(600.0, 6, 8.0, 1.3), COLUMN_MIN * 1.3);
    }
}
