//! Línea de tiempo / Gantt (RF-VIS-10): una barra por tarea entre su inicio
//! y su vencimiento, flechas de dependencia, hitos (RF-MOD-13) y la línea
//! de hoy. Arrastrar una barra la reprograma; sus bordes cambian una fecha.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use i18n::t;
use model::{Date, TaskId};

use crate::data::{Snapshot, TaskRow};
use crate::state::{GanttEdge, Intent, UiState, flat_rows};
use crate::theme;
use crate::widgets::date_picker::{add_days, month_key};
use crate::widgets::text::fitted;

/// Columna de nombres: hasta este ancho, o un 30 % de la vista si es estrecha.
const LABEL_W: f32 = 260.0;
const LABEL_MIN: f32 = 140.0;
const HEADER_H: f32 = 40.0;
const EDGE: f32 = 6.0;
/// Margen alrededor de las fechas, y tope para no dibujar años vacíos.
const PAD_DAYS: i64 = 3;
const MAX_DAYS: i64 = 730;

/// Días de la barra: de inicio a vencimiento; con una sola fecha, un día.
pub fn span(task: &model::Task) -> Option<(Date, Date)> {
    let start = task.start.as_ref().map(model::When::date);
    let due = task.due.as_ref().map(model::When::date);
    match (start, due) {
        (Some(s), Some(d)) => Some((s.min(d), s.max(d))),
        (Some(x), None) | (None, Some(x)) => Some((x, x)),
        (None, None) => None,
    }
}

/// Días de `a` a `b` (negativo si `b` es anterior).
fn days_between(a: Date, b: Date) -> i64 {
    a.until((jiff::Unit::Day, b))
        .map_or(0, |s| i64::from(s.get_days()))
}

/// Fechas nuevas tras arrastrar `days`; `edge` = solo ese extremo.
pub fn moved(task: &model::Task, edge: Option<GanttEdge>, days: i64) -> Option<(Date, Date)> {
    let (s, d) = span(task)?;
    Some(match edge {
        None => (add_days(s, days), add_days(d, days)),
        Some(GanttEdge::Start) => (add_days(s, days).min(d), d),
        Some(GanttEdge::End) => (s, add_days(d, days).max(s)),
    })
}

pub fn toolbar(ui: &mut Ui, st: &mut UiState) {
    ui.label(t(st.lang, "ui-zoom"));
    ui.add(egui::Slider::new(&mut st.gantt.day_width, 6.0..=60.0).show_value(false));
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let today = rules::filter::today();
    let all = flat_rows(snap, st, false);
    let rows: Vec<(&TaskRow, Date, Date)> = all
        .iter()
        .filter_map(|r| span(&r.task).map(|(s, d)| (*r, s, d)))
        .collect();
    let undated = all.len() - rows.len();
    // Ruta crítica de lo abierto (RF-REL-06): sus barras van con borde rojo.
    let open: Vec<model::Task> = all
        .iter()
        .filter(|r| !r.is_closed())
        .map(|r| r.task.clone())
        .collect();
    let (path, _) = rules::graph::critical_path(&open, &snap.dependencies);
    let critical: std::collections::HashSet<model::TaskId> = if path.len() > 1 {
        path.into_iter().collect()
    } else {
        Default::default()
    };
    let milestones: Vec<(&str, Date, bool)> = snap
        .epics
        .iter()
        .filter(|e| e.epic.kind == model::EpicKind::Milestone)
        .filter(|e| match st.scope {
            crate::data::Scope::All => true,
            crate::data::Scope::Project(p) => e.epic.project_id == p,
        })
        .filter_map(|e| {
            let done = e.epic.status != model::EpicStatus::Open;
            e.epic
                .target
                .as_ref()
                .map(|w| (e.epic.name.as_str(), w.date(), done))
        })
        .collect();
    if rows.is_empty() && milestones.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-gantt-empty")));
        return;
    }
    let dates = rows
        .iter()
        .flat_map(|(_, s, d)| [*s, *d])
        .chain(milestones.iter().map(|m| m.1))
        .chain([today]);
    let first = dates.clone().min().unwrap_or(today);
    let last = dates.max().unwrap_or(today);
    let origin = add_days(first, -PAD_DAYS);
    let days = (days_between(origin, last) + PAD_DAYS * 2).clamp(14, MAX_DAYS);

    if undated > 0 {
        ui.weak(i18n::tr(
            lang,
            &i18n::Msg::new("ui-gantt-undated").arg("count", undated),
        ));
    }
    let day_w = st.gantt.day_width;
    let row_h = ui.spacing().interact_size.y + 4.0;
    let label_w = (ui.available_width() * 0.3).clamp(LABEL_MIN, LABEL_W);
    // El lienzo llena la vista: más días a la derecha y la cuadrícula hasta abajo.
    let bar = crate::layout::scrollbar(ui);
    let visible = ui.available_size() - vec2(bar, bar);
    let days = fill_days(days, day_w, visible.x, label_w);
    let width = label_w + days as f32 * day_w;
    let height = (HEADER_H + row_h * (rows.len() as f32 + 1.0)).max(visible.y);

    egui::ScrollArea::both()
        .id_salt("gantt")
        // Lo que no cabe a la derecha se anuncia con su barra siempre visible.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            let (canvas, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            let clip = ui.clip_rect();
            let painter = ui.painter_at(canvas);
            let visuals = ui.visuals().clone();
            let x_of = |d: Date| canvas.left() + label_w + days_between(origin, d) as f32 * day_w;
            let y_of = |i: usize| canvas.top() + HEADER_H + row_h * (i as f32 + 1.0);

            // Cabecera: meses y días, fines de semana sombreados.
            let font = egui::FontId::proportional(11.0);
            for i in 0..days {
                let d = add_days(origin, i);
                let x = x_of(d);
                if x + day_w < clip.left() || x > clip.right() {
                    continue;
                }
                let weekend = matches!(
                    d.weekday(),
                    jiff::civil::Weekday::Saturday | jiff::civil::Weekday::Sunday
                );
                if weekend {
                    painter.rect_filled(
                        Rect::from_min_max(
                            pos2(x, canvas.top() + HEADER_H),
                            pos2(x + day_w, canvas.bottom()),
                        ),
                        0.0,
                        visuals.faint_bg_color,
                    );
                }
                if d.day() == 1 || i == 0 {
                    painter.text(
                        pos2(x + 2.0, canvas.top() + 2.0),
                        egui::Align2::LEFT_TOP,
                        format!("{} {}", t(lang, month_key(d.month())), d.year()),
                        font.clone(),
                        visuals.strong_text_color(),
                    );
                    painter.line_segment(
                        [pos2(x, canvas.top()), pos2(x, canvas.bottom())],
                        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
                    );
                }
                if day_w >= 14.0 {
                    painter.text(
                        pos2(x + day_w / 2.0, canvas.top() + 22.0),
                        egui::Align2::CENTER_TOP,
                        d.day().to_string(),
                        font.clone(),
                        visuals.weak_text_color(),
                    );
                }
            }
            // Hoy.
            let tx = x_of(today) + day_w / 2.0;
            painter.line_segment(
                [pos2(tx, canvas.top() + HEADER_H), pos2(tx, canvas.bottom())],
                Stroke::new(1.5, theme::RED),
            );

            // Hitos: rombo en la fila de arriba y línea discontinua.
            let my = canvas.top() + HEADER_H + row_h / 2.0;
            for (name, date, done) in &milestones {
                let x = x_of(*date) + day_w / 2.0;
                let color = if *done { theme::GREEN } else { theme::VIOLET };
                let r = 6.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        pos2(x, my - r),
                        pos2(x + r, my),
                        pos2(x, my + r),
                        pos2(x - r, my),
                    ],
                    color,
                    Stroke::NONE,
                ));
                painter.text(
                    pos2(x + r + 3.0, my),
                    egui::Align2::LEFT_CENTER,
                    *name,
                    font.clone(),
                    color,
                );
                painter.add(egui::Shape::dashed_line(
                    &[pos2(x, my + r), pos2(x, canvas.bottom())],
                    Stroke::new(1.0, color.gamma_multiply(0.5)),
                    4.0,
                    4.0,
                ));
            }

            // Barras (solo las visibles) y su interacción.
            let mut bars: HashMap<TaskId, Rect> = HashMap::new();
            let mut labels: Vec<(&TaskRow, f32)> = Vec::new();
            let drag = st.gantt.drag;
            for (i, (row, s, d)) in rows.iter().enumerate() {
                let y = y_of(i);
                if y + row_h < clip.top() - row_h * 20.0 || y > clip.bottom() + row_h * 20.0 {
                    continue;
                }
                let (mut s, mut d) = (*s, *d);
                if let Some((id, edge, delta)) = drag
                    && id == row.id()
                    && let Some((ns, nd)) = moved(&row.task, edge, delta)
                {
                    (s, d) = (ns, nd);
                }
                let bar = Rect::from_min_max(
                    pos2(x_of(s) + 1.0, y + 4.0),
                    pos2(x_of(d) + day_w - 1.0, y + row_h - 4.0),
                );
                bars.insert(row.id(), bar);
                if i % 2 == 1 {
                    painter.rect_filled(
                        Rect::from_min_max(pos2(canvas.left(), y), pos2(canvas.right(), y + row_h)),
                        0.0,
                        visuals.faint_bg_color.gamma_multiply(0.6),
                    );
                }
                labels.push((*row, y));

                let color = theme::status(row.task.status);
                let fill = if row.is_closed() {
                    color.gamma_multiply(0.45)
                } else {
                    color
                };
                painter.rect_filled(bar, 4.0, fill);
                if critical.contains(&row.id()) {
                    painter.rect_stroke(
                        bar.expand(1.5),
                        4.0,
                        Stroke::new(2.0, theme::RED),
                        egui::StrokeKind::Outside,
                    );
                }
                if day_w * (days_between(s, d) + 1) as f32 > 40.0 {
                    let text = fitted(
                        ui,
                        &row.task.title,
                        font.clone(),
                        theme::on(fill),
                        bar.width() - 8.0,
                    );
                    painter.galley(
                        pos2(bar.left() + 4.0, bar.center().y - text.size().y / 2.0),
                        text,
                        theme::on(fill),
                    );
                }
                let response = ui
                    .interact(
                        bar,
                        egui::Id::unique(("gantt-bar", row.id())),
                        Sense::click_and_drag(),
                    )
                    .on_hover_text(format!("{} · {s} – {d}", row.key));
                let edge = response
                    .interact_pointer_pos()
                    .or(response.hover_pos())
                    .and_then(|p| {
                        if p.x <= bar.left() + EDGE {
                            Some(GanttEdge::Start)
                        } else if p.x >= bar.right() - EDGE {
                            Some(GanttEdge::End)
                        } else {
                            None
                        }
                    });
                if response.hovered() {
                    ui.ctx().set_cursor_icon(match edge {
                        Some(_) => egui::CursorIcon::ResizeHorizontal,
                        None => egui::CursorIcon::Grab,
                    });
                }
                if response.drag_started() {
                    st.gantt.drag = Some((row.id(), edge, 0));
                }
                if response.dragged()
                    && let Some((id, e, _)) = st.gantt.drag
                    && id == row.id()
                    && let (Some(origin_pos), Some(now)) = (
                        ui.input(|i| i.pointer.press_origin()),
                        ui.input(|i| i.pointer.latest_pos()),
                    )
                {
                    let delta = ((now.x - origin_pos.x) / day_w).round() as i64;
                    st.gantt.drag = Some((id, e, delta));
                }
                if response.drag_stopped()
                    && let Some((id, e, delta)) = st.gantt.drag.take()
                    && id == row.id()
                    && delta != 0
                {
                    out.push(reschedule(row, e, delta));
                }
                if response.clicked() {
                    out.push(Intent::Select(Some(row.id())));
                }
            }

            // Flechas de dependencia: del final de la bloqueadora al inicio de la bloqueada.
            let arrow = Stroke::new(1.2, visuals.weak_text_color());
            for dep in &snap.dependencies {
                let (Some(a), Some(b)) = (bars.get(&dep.blocker), bars.get(&dep.blocked)) else {
                    continue;
                };
                let from = pos2(a.right(), a.center().y);
                let to = pos2(b.left(), b.center().y);
                let mid = from.x + 8.0;
                let path = [
                    from,
                    pos2(mid, from.y),
                    pos2(mid, to.y),
                    pos2(to.x - 2.0, to.y),
                ];
                painter.add(egui::Shape::line(path.to_vec(), arrow));
                head(&painter, to, arrow.color);
            }

            // Nombres: una columna fija a la izquierda aunque se desplace en
            // horizontal; tapa las barras que pasan por debajo.
            let left = canvas.left().max(clip.left());
            let column = Rect::from_min_max(
                pos2(left, canvas.top()),
                pos2(left + label_w, canvas.bottom()),
            );
            painter.rect_filled(column, 0.0, visuals.panel_fill);
            painter.line_segment(
                [column.right_top(), column.right_bottom()],
                Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
            );
            // El mes del primer día visible, que al desplazar ya no tiene su etiqueta.
            let first_visible = add_days(origin, ((left - canvas.left()) / day_w).floor() as i64);
            if left > canvas.left() && first_visible.day() != 1 {
                let month = painter.layout_no_wrap(
                    format!(
                        "{} {}",
                        t(lang, month_key(first_visible.month())),
                        first_visible.year()
                    ),
                    font.clone(),
                    visuals.strong_text_color(),
                );
                let at = pos2(column.right() + 2.0, canvas.top() + 2.0);
                painter.rect_filled(
                    Rect::from_min_size(at, month.size()).expand(2.0),
                    0.0,
                    visuals.panel_fill,
                );
                painter.galley(at, month, visuals.strong_text_color());
            }
            painter.text(
                pos2(left + 6.0, my),
                egui::Align2::LEFT_CENTER,
                t(lang, "ui-gantt-milestones"),
                font.clone(),
                visuals.weak_text_color(),
            );
            for (row, y) in labels {
                let label = Rect::from_min_size(pos2(left, y), vec2(label_w - 6.0, row_h));
                let lr = ui.interact(
                    label,
                    egui::Id::unique(("gantt-label", row.id())),
                    Sense::click(),
                );
                if st.selected == Some(row.id()) || lr.hovered() {
                    painter.rect_filled(label, 2.0, visuals.selection.bg_fill.gamma_multiply(0.4));
                }
                let text = fitted(
                    ui,
                    &format!("{}  {}", row.key, row.task.title),
                    egui::FontId::proportional(13.0),
                    visuals.text_color(),
                    label.width() - 12.0,
                );
                painter.galley(
                    pos2(label.left() + 6.0, y + (row_h - text.size().y) / 2.0),
                    text,
                    visuals.text_color(),
                );
                if lr.on_hover_text(&row.task.title).clicked() {
                    out.push(Intent::Select(Some(row.id())));
                }
            }
        });
}

/// Días que se dibujan: los de las tareas o, si no llenan el ancho visible,
/// los que hagan falta para llegar al borde derecho.
fn fill_days(days: i64, day_w: f32, visible_w: f32, label_w: f32) -> i64 {
    let fits = ((visible_w - label_w) / day_w.max(1.0)).ceil() as i64;
    days.max(fits).min(MAX_DAYS)
}

/// Comando que guarda el arrastre.
pub fn reschedule(row: &TaskRow, edge: Option<GanttEdge>, days: i64) -> Intent {
    let Some((s, d)) = moved(&row.task, edge, days) else {
        return Intent::Select(Some(row.id()));
    };
    let has_start = row.task.start.is_some();
    let has_due = row.task.due.is_some();
    let (s, d) = (s.to_string(), d.to_string());
    let mut args = vec![("task", row.key.as_str())];
    if has_start || edge == Some(GanttEdge::Start) {
        args.push(("start", &s));
    }
    if has_due || edge == Some(GanttEdge::End) || !has_start {
        args.push(("due", &d));
    }
    Intent::run("task.update", &args)
}

fn head(painter: &egui::Painter, tip: Pos2, color: Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            pos2(tip.x - 6.0, tip.y - 4.0),
            pos2(tip.x - 6.0, tip.y + 4.0),
        ],
        color,
        Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timeline_reaches_the_right_edge() {
        assert_eq!(
            fill_days(10, 24.0, 1260.0, 260.0),
            42,
            "1000 px / 24 px por día"
        );
        assert_eq!(fill_days(90, 24.0, 1260.0, 260.0), 90, "ya pasa del borde");
        assert_eq!(fill_days(10, 6.0, 100_000.0, 260.0), MAX_DAYS);
    }

    #[test]
    fn counts_days_between_dates() {
        let d = |s: &str| s.parse::<Date>().unwrap();
        assert_eq!(days_between(d("2026-10-26"), d("2026-11-15")), 20);
        assert_eq!(days_between(d("2026-10-26"), d("2026-10-21")), -5);
        assert_eq!(days_between(d("2026-03-28"), d("2026-03-30")), 2);
    }

    #[test]
    fn dragging_moves_both_dates_or_one_edge() {
        let d = |s: &str| s.parse::<Date>().unwrap();
        let mut task: model::Task = serde_json::from_value(serde_json::json!({
            "id": model::TaskId::generate(), "project_id": model::ProjectId::generate(),
            "number": 1, "parent_id": null, "title": "t", "description": "",
            "status": "todo", "priority": "medium", "start": "2026-03-02", "due": "2026-03-05",
            "labels": [], "assignee": null, "creator": "supervisor", "checklist": [],
            "criteria": [], "revision": 1, "created_at": "2026-03-01T00:00:00Z",
            "updated_at": "2026-03-01T00:00:00Z", "archived_at": null, "deleted_at": null,
            "claim": null, "needs_human": null, "locked": false
        }))
        .unwrap();
        assert_eq!(
            moved(&task, None, 2),
            Some((d("2026-03-04"), d("2026-03-07")))
        );
        assert_eq!(
            moved(&task, Some(GanttEdge::Start), 5),
            Some((d("2026-03-05"), d("2026-03-05"))),
            "el inicio no pasa del vencimiento"
        );
        assert_eq!(
            moved(&task, Some(GanttEdge::End), -1),
            Some((d("2026-03-02"), d("2026-03-04")))
        );
        task.start = None;
        assert_eq!(span(&task), Some((d("2026-03-05"), d("2026-03-05"))));
    }
}
