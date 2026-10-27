//! Calendario mensual y semanal (RF-VIS-09): las tareas aparecen el día que
//! vencen; arrastrarlas a otro día las reprograma y las que no tienen fecha
//! esperan en una columna lateral.
//!
//! La cuadrícula se calcula una vez por frame a partir del espacio disponible y
//! cada celda tiene un rectángulo fijo con su contenido recortado: nada de lo
//! que haya dentro puede cambiar el tamaño de la cuadrícula.

use eframe::egui::{self, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use i18n::t;
use jiff::civil::Date;
use model::When;

use crate::data::{Snapshot, TaskRow};
use crate::hierarchy;
use crate::state::{Intent, UiState, flat_rows};
use crate::theme;
use crate::widgets::date_picker::{
    add_days, add_months, grid_start, month_key, today, weekday_keys,
};
use crate::widgets::icons::{self, Icon};
use crate::widgets::inline::{self, Outcome};

/// Ancho de la columna "Sin fecha" con el texto a tamaño normal.
const SIDE_WIDTH: f32 = 220.0;
const GAP: f32 = 3.0;
/// Ancho de día por debajo del cual la columna "Sin fecha" deja su sitio a la
/// cuadrícula y pasa a abrirse con un botón.
const MIN_DAY: f32 = 80.0;
/// Celdas más estrechas solo muestran el "+" al pasar por encima.
const PLUS_FROM: f32 = 64.0;

/// Medidas que dependen del tamaño de letra elegido; fijas durante el frame.
#[derive(Clone, Copy)]
struct Metrics {
    /// Alto de la fila de nombres de los días.
    header: f32,
    /// Alto de la fila con el número del día.
    day_header: f32,
    chip: f32,
}

impl Metrics {
    fn of(ui: &Ui) -> Self {
        let line = ui.text_style_height(&egui::TextStyle::Body);
        Self {
            header: line + 8.0,
            day_header: line + 6.0,
            chip: line + 6.0,
        }
    }
}

#[derive(Clone)]
struct Drag {
    key: String,
    due: Option<When>,
}

/// Fecha en la que se muestra la tarea: vencimiento, o si no, inicio.
fn day_of(row: &TaskRow) -> Option<Date> {
    row.task
        .due
        .as_ref()
        .or(row.task.start.as_ref())
        .map(When::date)
}

/// Nuevo vencimiento en `day`, conservando la hora y la zona si las tenía.
fn moved(due: Option<&When>, day: Date) -> String {
    match due {
        Some(When::At(z)) => jiff::civil::DateTime::from_parts(day, z.time())
            .to_zoned(z.time_zone().clone())
            .map(|z| z.to_string())
            .unwrap_or_else(|_| day.to_string()),
        _ => day.to_string(),
    }
}

pub fn toolbar(ui: &mut Ui, st: &mut UiState) {
    let lang = st.lang;
    let step = |st: &mut UiState, n: i64| {
        st.calendar_month = if st.calendar_week {
            add_days(st.calendar_month, 7 * n)
        } else {
            add_months(st.calendar_month, n)
        };
    };
    if icons::button(ui, Icon::Left, true, &t(lang, "ui-previous")).clicked() {
        step(st, -1);
    }
    if ui.button(t(lang, "ui-today")).clicked() {
        st.calendar_month = today();
    }
    if icons::button(ui, Icon::Right, true, &t(lang, "ui-next")).clicked() {
        step(st, 1);
    }
    let m = st.calendar_month;
    ui.strong(format!("{} {}", t(lang, month_key(m.month())), m.year()));
    ui.separator();
    ui.selectable_value(&mut st.calendar_week, false, t(lang, "ui-month"));
    ui.selectable_value(&mut st.calendar_week, true, t(lang, "ui-week"));
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let rows = flat_rows(snap, st, false);
    let (start, weeks) = if st.calendar_week {
        let d = st.calendar_month;
        (
            add_days(d, -i64::from(d.weekday().to_monday_zero_offset())),
            1,
        )
    } else {
        (grid_start(st.calendar_month), 6)
    };

    // Todo el espacio que queda, repartido una sola vez: cuadrícula y columna lateral.
    let m = Metrics::of(ui);
    let scale = st.text_size.scale();
    let side_width = SIDE_WIDTH * scale;
    let lang = st.lang;
    // Estrecho: la cuadrícula ocupa todo y lo que no tiene fecha se ve aparte.
    let narrow = ui.available_width() < 7.0 * MIN_DAY * scale + side_width + GAP * 2.0;
    let pending = rows
        .iter()
        .filter(|r| day_of(r).is_none() && !r.is_closed())
        .count();
    if narrow {
        let label = format!("{} ({pending})", t(lang, "ui-unscheduled"));
        let on = st.panels.calendar_unscheduled;
        if ui.selectable_label(on, label).clicked() {
            st.panels.calendar_unscheduled = !on;
        }
    }
    // Nunca más ancho que lo visible del panel, aunque algo de arriba lo haya ensanchado.
    let mut area = ui.available_rect_before_wrap();
    area.max.x = area.max.x.min(ui.clip_rect().max.x);
    ui.allocate_rect(area, Sense::hover());
    if narrow && st.panels.calendar_unscheduled {
        unscheduled(ui, snap, area, m, &rows, st, false, out);
        return;
    }
    let grid = if narrow {
        area
    } else {
        Rect::from_min_max(
            area.min,
            pos2(area.max.x - side_width - GAP * 2.0, area.max.y),
        )
    };
    let side = Rect::from_min_max(pos2(grid.max.x + GAP * 2.0, area.min.y), area.max);

    let cell_w = ((grid.width() - GAP * 6.0) / 7.0).max(1.0);
    let rows_n = weeks as f32;
    let cell_h = ((grid.height() - m.header - GAP * rows_n) / rows_n).max(1.0);

    for (i, key) in weekday_keys().into_iter().enumerate() {
        let x = grid.min.x + (cell_w + GAP) * i as f32;
        let rect = Rect::from_min_size(pos2(x, grid.min.y), vec2(cell_w, m.header));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            t(lang, key),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().strong_text_color(),
        );
    }
    for week in 0..weeks {
        for wd in 0..7 {
            let day = add_days(start, week * 7 + wd);
            let min = pos2(
                grid.min.x + (cell_w + GAP) * wd as f32,
                grid.min.y + m.header + (cell_h + GAP) * week as f32,
            );
            let rect = Rect::from_min_size(min, vec2(cell_w, cell_h));
            let tasks: Vec<&&TaskRow> = rows.iter().filter(|r| day_of(r) == Some(day)).collect();
            cell(ui, snap, st, m, day, rect, &tasks, out);
        }
    }

    if !narrow {
        unscheduled(ui, snap, side, m, &rows, st, true, out);
    }
}

#[allow(clippy::too_many_arguments)]
fn cell(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    m: Metrics,
    day: Date,
    rect: Rect,
    tasks: &[&&TaskRow],
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let in_month = st.calendar_week || day.month() == st.calendar_month.month();
    let is_today = day == today();

    // Zona donde soltar: se detecta sobre el rectángulo fijo, sin marco que crezca.
    let zone = ui.interact(
        rect,
        ui.make_persistent_id(("cal-day", day)),
        Sense::hover(),
    );
    let hovering = zone.dnd_hover_payload::<Drag>().is_some();
    let fill = if hovering {
        ui.visuals().selection.bg_fill.gamma_multiply(0.5)
    } else if is_today {
        ui.visuals().selection.bg_fill.gamma_multiply(0.25)
    } else if in_month {
        ui.visuals().faint_bg_color
    } else {
        ui.visuals().extreme_bg_color
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    if hovering {
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.5, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(drag) = zone.dnd_release_payload::<Drag>() {
        out.push(Intent::run(
            "task.update",
            &[("task", &drag.key), ("due", &moved(drag.due.as_ref(), day))],
        ));
    }

    let inner = rect.shrink(4.0);
    if inner.width() < 1.0 || inner.height() < 1.0 {
        return;
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(("cal-cell", day))
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    let ui = &mut child;

    ui.allocate_ui_with_layout(
        vec2(inner.width(), m.day_header),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let mut n = RichText::new(day.day().to_string());
            if !in_month {
                n = n.weak();
            }
            if is_today {
                n = n.strong().color(theme::BLUE);
            }
            ui.label(n);
            let roomy = inner.width() >= PLUS_FROM * st.text_size.scale();
            let hovered = ui.rect_contains_pointer(rect);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if (roomy || hovered)
                    && icons::button_sized(
                        ui,
                        Icon::Plus,
                        true,
                        &t(lang, "ui-new-task-on-day"),
                        m.chip - 4.0,
                    )
                    .clicked()
                {
                    st.quick_add = Some((day, String::new()));
                }
            });
        },
    );

    let mut used = m.day_header;
    if let Some((d, text)) = st.quick_add.as_mut()
        && *d == day
    {
        let edit = ui.add_sized(
            vec2(inner.width(), m.chip),
            egui::TextEdit::singleline(text).hint_text(t(lang, "ui-new-task-hint")),
        );
        let outcome = inline::outcome(ui, &edit);
        if outcome == Outcome::Submit && !text.trim().is_empty() {
            out.push(Intent::CreateOnDay {
                title: text.trim().to_owned(),
                day,
            });
        }
        if outcome != Outcome::Editing {
            st.quick_add = None;
        }
        used += m.chip + 2.0;
    }

    // Cuántas tareas caben; el resto se resume en "+N más".
    let room = ((inner.height() - used) / (m.chip + 2.0)).floor().max(0.0) as usize;
    let fits = if tasks.len() > room {
        room.saturating_sub(1)
    } else {
        tasks.len()
    };
    for row in tasks.iter().take(fits) {
        chip(ui, snap, st, row, inner.width(), m, out);
    }
    if tasks.len() > fits {
        ui.weak(i18n::tr(
            lang,
            &i18n::Msg::new("ui-more").arg("n", tasks.len() - fits),
        ));
    }
}

fn chip(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    row: &TaskRow,
    width: f32,
    m: Metrics,
    out: &mut Vec<Intent>,
) {
    let selected = st.selected;
    let drag_id = egui::Id::unique(("cal-task", row.id()));
    let payload = Drag {
        key: row.key.clone(),
        due: row.task.due.clone(),
    };
    let inner = ui.dnd_drag_source(drag_id, payload, |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, m.chip), Sense::hover());
        let color = theme::priority(row.task.priority);
        let fill = if selected == Some(row.id()) {
            ui.visuals().selection.bg_fill
        } else {
            color.gamma_multiply(0.18)
        };
        ui.painter().rect_filled(rect, 3.0, fill);
        ui.painter().rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        // La clave va en el aviso al pasar el ratón: en la celda cabe más título.
        let mut text = RichText::new(&row.task.title);
        if row.is_closed() {
            text = text.strikethrough().weak();
        }
        let mut label_ui = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(4.0, 1.0))));
        label_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        label_ui.add(egui::Label::new(text).truncate());
    });
    let click = ui.interact(inner.response.rect, drag_id.with("click"), Sense::click());
    if click.clicked() {
        out.push(Intent::Select(Some(row.id())));
    }
    click.context_menu(|ui| hierarchy::context_menu(ui, snap, row, st, out));
    click.on_hover_text(format!("{}  {}", row.key, row.task.title));
    ui.add_space(2.0);
}

/// Tareas abiertas sin fecha: se programan arrastrándolas a un día
/// (`beside_grid`) o abriéndolas, si la cuadrícula no está al lado.
#[allow(clippy::too_many_arguments)]
fn unscheduled(
    ui: &mut Ui,
    snap: &Snapshot,
    side: Rect,
    m: Metrics,
    rows: &[&TaskRow],
    st: &mut UiState,
    beside_grid: bool,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    ui.painter()
        .rect_filled(side, 4.0, ui.visuals().faint_bg_color);
    let inner = side.shrink(6.0);
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt("cal-unscheduled")
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    let ui = &mut child;
    ui.strong(t(lang, "ui-unscheduled"));
    // Sin la cuadrícula al lado no hay dónde soltarlas: se abren y se les pone fecha.
    let hint = if beside_grid {
        "ui-unscheduled-hint"
    } else {
        "ui-unscheduled-open-hint"
    };
    ui.add(egui::Label::new(RichText::new(t(lang, hint)).weak()).wrap());
    ui.add_space(4.0);
    let pending: Vec<&&TaskRow> = rows
        .iter()
        .filter(|r| day_of(r).is_none() && !r.is_closed())
        .collect();
    if pending.is_empty() {
        ui.weak("—");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("unscheduled")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in pending {
                chip(ui, snap, st, row, inner.width() - 4.0, m, out);
            }
        });
}
