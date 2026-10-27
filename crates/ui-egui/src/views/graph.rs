//! Grafo de dependencias (RF-REL-05) con la ruta crítica resaltada
//! (RF-REL-06): una columna por capa, de lo que no depende de nada a lo que
//! espera por más. Clic en un nodo para abrirlo.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, vec2};
use i18n::{Msg, t, tr};
use model::{Task, TaskId};

use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, UiState, flat_rows};
use crate::theme;
use crate::widgets::text::fitted;

const NODE_W: f32 = 200.0;
const NODE_H: f32 = 44.0;
const GAP_X: f32 = 70.0;
const GAP_Y: f32 = 14.0;
/// Separación entre el grafo y la rejilla de tareas sueltas (con su título).
const LOOSE_GAP: f32 = 36.0;

pub fn toolbar(ui: &mut Ui, st: &mut UiState) {
    ui.checkbox(&mut st.graph_closed, t(st.lang, "ui-graph-closed"));
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let rows: Vec<&TaskRow> = flat_rows(snap, st, false)
        .into_iter()
        .filter(|r| st.graph_closed || !r.is_closed())
        .collect();
    let tasks: Vec<Task> = rows.iter().map(|r| r.task.clone()).collect();
    let ids: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    let deps: Vec<model::Dependency> = snap
        .dependencies
        .iter()
        .filter(|d| ids.contains(&d.blocker) && ids.contains(&d.blocked))
        .copied()
        .collect();
    if deps.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-graph-empty")));
        return;
    }
    let layers = rules::graph::layers(&tasks, &deps);
    let open: Vec<Task> = tasks
        .iter()
        .filter(|t| !t.status.category().is_closed())
        .cloned()
        .collect();
    let (path, total) = rules::graph::critical_path(&open, &deps);
    let on_path: HashSet<TaskId> = path.iter().copied().collect();
    ui.weak(tr(
        lang,
        &Msg::new("ui-graph-critical")
            .arg("n", path.len())
            .arg("total", format!("{total:.1}")),
    ));

    // Posición de cada nodo. Lo enlazado: columna = capa y fila = orden en
    // la capa. Lo que no depende de nada ni bloquea nada va debajo, en una
    // rejilla que aprovecha el ancho.
    let linked: HashSet<TaskId> = deps.iter().flat_map(|d| [d.blocker, d.blocked]).collect();
    let (connected, loose): (Vec<&TaskRow>, Vec<&TaskRow>) =
        rows.iter().partition(|r| linked.contains(&r.id()));
    let mut next_row: HashMap<usize, usize> = HashMap::new();
    let mut place: HashMap<TaskId, egui::Vec2> = HashMap::new();
    for r in &connected {
        let layer = layers.get(&r.id()).copied().unwrap_or(0);
        let row = next_row.entry(layer).or_default();
        place.insert(
            r.id(),
            vec2(
                layer as f32 * (NODE_W + GAP_X),
                *row as f32 * (NODE_H + GAP_Y),
            ),
        );
        *row += 1;
    }
    let graph_w = (layers.values().max().copied().unwrap_or(0) + 1) as f32 * (NODE_W + GAP_X);
    let graph_h = next_row.values().max().copied().unwrap_or(1) as f32 * (NODE_H + GAP_Y);
    let bar = crate::layout::scrollbar(ui);
    let visible = ui.available_size() - vec2(bar, bar);
    let per_row = (((visible.x - 20.0 + GAP_Y) / (NODE_W + GAP_Y)).floor() as usize).max(1);
    let loose_top = graph_h + LOOSE_GAP;
    for (i, r) in loose.iter().enumerate() {
        place.insert(
            r.id(),
            vec2(
                (i % per_row) as f32 * (NODE_W + GAP_Y),
                loose_top + (i / per_row) as f32 * (NODE_H + GAP_Y),
            ),
        );
    }
    let loose_h = if loose.is_empty() {
        0.0
    } else {
        LOOSE_GAP + loose.len().div_ceil(per_row) as f32 * (NODE_H + GAP_Y)
    };
    let width = graph_w.max(per_row.min(loose.len()) as f32 * (NODE_W + GAP_Y));
    let height = graph_h + loose_h;

    egui::ScrollArea::both()
        .id_salt("graph")
        // Lo que no cabe a la derecha se anuncia con su barra siempre visible.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            // El lienzo llena la vista aunque el grafo sea pequeño.
            let size = vec2(width + 20.0, height + 20.0).max(visible);
            let (area, _) = ui.allocate_exact_size(size, Sense::hover());
            let origin = area.min + vec2(10.0, 10.0);
            let rect_of = |id: &TaskId| {
                place
                    .get(id)
                    .map(|at| Rect::from_min_size(origin + *at, vec2(NODE_W, NODE_H)))
            };
            let painter = ui.painter_at(area);
            painter.rect_filled(area, 6.0, ui.visuals().faint_bg_color);
            if !loose.is_empty() {
                painter.text(
                    origin + vec2(0.0, loose_top - 8.0),
                    Align2::LEFT_BOTTOM,
                    t(lang, "ui-graph-loose"),
                    egui::TextStyle::Small.resolve(ui.style()),
                    ui.visuals().weak_text_color(),
                );
            }
            let line = ui.visuals().widgets.noninteractive.fg_stroke.color;
            for d in &deps {
                let (Some(a), Some(b)) = (rect_of(&d.blocker), rect_of(&d.blocked)) else {
                    continue;
                };
                let critical = path
                    .windows(2)
                    .any(|w| w.first() == Some(&d.blocker) && w.get(1) == Some(&d.blocked));
                let color = if critical { theme::RED } else { line };
                arrow(&painter, a.right_center(), b.left_center(), color, critical);
            }
            for r in &rows {
                let Some(rect) = rect_of(&r.id()) else {
                    continue;
                };
                node(
                    ui,
                    &painter,
                    rect,
                    r,
                    st.selected == Some(r.id()),
                    on_path.contains(&r.id()),
                    out,
                );
            }
        });
}

fn arrow(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, bold: bool) {
    let stroke = Stroke::new(if bold { 2.2 } else { 1.2 }, color);
    let mid = (from.x + to.x) / 2.0;
    let points = [from, Pos2::new(mid, from.y), Pos2::new(mid, to.y), to];
    for pair in points.windows(2) {
        if let [a, b] = pair {
            painter.line_segment([*a, *b], stroke);
        }
    }
    painter.line_segment([to, to + vec2(-7.0, -4.0)], stroke);
    painter.line_segment([to, to + vec2(-7.0, 4.0)], stroke);
}

fn node(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    row: &TaskRow,
    selected: bool,
    critical: bool,
    out: &mut Vec<Intent>,
) {
    let response = ui.interact(
        rect,
        egui::Id::unique(("graph-node", row.id())),
        Sense::click(),
    );
    let visuals = ui.visuals();
    let stroke = match (selected, critical) {
        (true, _) => Stroke::new(2.0, visuals.selection.stroke.color),
        (false, true) => Stroke::new(2.0, theme::RED),
        _ => Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
    };
    let fill = if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.window_fill
    };
    painter.rect(rect, 6.0, fill, stroke, egui::StrokeKind::Inside);
    painter.circle_filled(
        rect.left_top() + vec2(10.0, 12.0),
        4.0,
        theme::status(row.task.status),
    );
    let font = egui::TextStyle::Small.resolve(ui.style());
    painter.text(
        rect.left_top() + vec2(20.0, 5.0),
        Align2::LEFT_TOP,
        &row.key,
        font.clone(),
        visuals.weak_text_color(),
    );
    let title = fitted(
        ui,
        &row.task.title,
        egui::TextStyle::Body.resolve(ui.style()),
        visuals.text_color(),
        rect.width() - 16.0,
    );
    painter.galley(
        rect.left_top() + vec2(8.0, 22.0),
        title,
        visuals.text_color(),
    );
    if response.on_hover_text(&row.task.title).clicked() {
        out.push(Intent::Select(Some(row.id())));
    }
}
