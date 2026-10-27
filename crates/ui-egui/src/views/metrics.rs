//! Métricas (RF-INT-06) y hojas de horas (RF-HIS-05): resumen del flujo,
//! throughput, burndown y flujo acumulado, y tablas por agente y por columna.
//! Sin proyecto elegido se ven los agregados de todos. Se calculan del
//! historial al entrar en la vista, al cambiar de alcance o al pedirlo.

use eframe::egui::{self, RichText, Ui};
use egui_plot::{Bar, BarChart, Legend, Line, Plot};
use i18n::{Lang, Msg, t, tr};
use serde_json::Value;

use crate::data::{Scope, Snapshot};
use crate::state::{Intent, MetricsData, MetricsTab, UiState};
use crate::theme;
use crate::views::common::{self, duration, hours};
use crate::widgets::icons;

/// Ancho a partir del cual los gráficos y las tablas van de dos en dos.
const TWO_COLUMNS: f32 = 1000.0;
/// Ancho mínimo de una tarjeta del resumen.
const CARD_MIN: f32 = 150.0;
/// Alto de cada gráfico.
const CHART_HEIGHT: f32 = 220.0;
/// Margen interior de una tarjeta.
const CARD_MARGIN: f32 = 10.0;
/// Lo que ocupa el marco de una tarjeta además de su contenido: margen y
/// borde a cada lado.
const CARD_CHROME: f32 = 2.0 * (CARD_MARGIN + 1.0);
/// Con más días que estos, el throughput va por semanas.
const DAILY_MAX: usize = 31;

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut st.metrics.tab,
            MetricsTab::Charts,
            t(lang, "ui-metrics-charts"),
        );
        ui.selectable_value(
            &mut st.metrics.tab,
            MetricsTab::Hours,
            t(lang, "ui-metrics-hours"),
        );
        ui.separator();
        let mut days = st.metrics.days;
        egui::ComboBox::from_id_salt("metrics_days")
            .selected_text(tr(lang, &Msg::new("ui-metrics-days").arg("n", days)))
            .show_ui(ui, |ui| {
                for d in [7u32, 14, 30, 90, 180] {
                    ui.selectable_value(
                        &mut days,
                        d,
                        tr(lang, &Msg::new("ui-metrics-days").arg("n", d)),
                    );
                }
            });
        if st.metrics.tab == MetricsTab::Hours {
            let mut period = st.metrics.period;
            egui::ComboBox::from_id_salt("metrics_period")
                .selected_text(t(lang, period_key(period)))
                .show_ui(ui, |ui| {
                    for p in ["day", "week", "month"] {
                        ui.selectable_value(&mut period, p, t(lang, period_key(p)));
                    }
                });
            if period != st.metrics.period {
                st.metrics.period = period;
                out.push(Intent::LoadMetrics);
            }
        }
        if days != st.metrics.days {
            st.metrics.days = days;
            out.push(Intent::LoadMetrics);
        }
        if ui.button(t(lang, "ui-refresh")).clicked() {
            out.push(Intent::LoadMetrics);
        }
    });
    ui.separator();
    let Some(data) = &st.metrics.data else {
        ui.weak(t(lang, "ui-loading"));
        out.push(Intent::LoadMetrics);
        return;
    };
    for e in &data.errors {
        ui.colored_label(theme::RED, e);
    }
    egui::ScrollArea::vertical()
        .id_salt("metrics")
        .show(ui, |ui| match st.metrics.tab {
            MetricsTab::Charts => charts(ui, snap, st.scope, lang, data),
            MetricsTab::Hours => sheet(ui, lang, &data.sheet),
        });
}

fn period_key(p: &str) -> &'static str {
    match p {
        "week" => "ui-period-week",
        "month" => "ui-period-month",
        _ => "ui-period-day",
    }
}

/// Tarjetas, gráficos y tablas; de dos en dos si hay anchura.
fn charts(ui: &mut Ui, snap: &Snapshot, scope: Scope, lang: Lang, data: &MetricsData) {
    let scope_name = match scope {
        Scope::Project(id) => snap.project(id).map(|p| p.key.clone()),
        Scope::All => None,
    }
    .unwrap_or_else(|| t(lang, "ui-all-projects"));
    cards(ui, lang, data.flow.get("summary").unwrap_or(&Value::Null));
    ui.add_space(12.0);
    let burndown_title = tr(
        lang,
        &Msg::new("ui-burndown").arg("project", scope_name.as_str()),
    );
    pair(
        ui,
        "metrics_charts",
        |ui| throughput(ui, lang, &data.flow),
        |ui| {
            section(ui, &burndown_title);
            burndown(ui, lang, &data.burndown);
        },
    );
    ui.add_space(12.0);
    section(ui, &t(lang, "ui-cfd"));
    cfd(ui, lang, &data.cfd);
    ui.add_space(12.0);
    pair(
        ui,
        "metrics_tables",
        |ui| {
            section(ui, &t(lang, "ui-flow-by-agent"));
            agents(ui, lang, &data.flow);
        },
        |ui| {
            section(ui, &t(lang, "ui-flow-by-column"));
            columns(ui, lang, &data.flow);
        },
    );
}

/// Dos bloques lado a lado si caben; si no, uno debajo del otro.
fn pair(ui: &mut Ui, id: &str, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    if ui.available_width() >= TWO_COLUMNS {
        ui.push_id(id, |ui| {
            ui.columns(2, |cols| {
                if let [a, b] = cols {
                    left(a);
                    b.push_id("right", right);
                }
            });
        });
    } else {
        left(ui);
        ui.add_space(12.0);
        right(ui);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).heading().size(17.0));
    ui.add_space(4.0);
}

fn no_data(ui: &mut Ui, lang: Lang) {
    ui.weak(t(lang, "ui-metrics-no-data"));
}

/// Una cifra del resumen: título, valor, una línea de apoyo y qué mide.
struct Card {
    title: String,
    value: Option<String>,
    note: Option<String>,
    hint: String,
}

fn opt_f64(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

/// Hechas, throughput, lead y cycle (p50, con su p85), devoluciones y WIP.
/// Lo que falta se ve como «—».
fn cards(ui: &mut Ui, lang: Lang, s: &Value) {
    let p85 = |key: &str| {
        opt_f64(s, key).map(|m| {
            tr(
                lang,
                &Msg::new("ui-flow-p85").arg("time", duration(m as i64)),
            )
        })
    };
    let list = [
        Card {
            title: t(lang, "ui-flow-done"),
            value: opt_f64(s, "done").map(|n| format!("{n:.0}")),
            note: opt_f64(s, "handoffs")
                .map(|n| tr(lang, &Msg::new("ui-flow-handoffs").arg("n", n as i64))),
            hint: t(lang, "ui-flow-done-hint"),
        },
        Card {
            title: t(lang, "ui-flow-throughput-week"),
            value: opt_f64(s, "throughput_per_week").map(|n| format!("{n:.1}")),
            note: None,
            hint: t(lang, "ui-flow-throughput-week-hint"),
        },
        Card {
            title: t(lang, "ui-flow-lead"),
            value: opt_f64(s, "lead_p50_minutes").map(|m| duration(m as i64)),
            note: p85("lead_p85_minutes"),
            hint: t(lang, "ui-flow-lead-hint"),
        },
        Card {
            title: t(lang, "ui-flow-cycle"),
            value: opt_f64(s, "cycle_p50_minutes").map(|m| duration(m as i64)),
            note: p85("cycle_p85_minutes"),
            hint: t(lang, "ui-flow-cycle-hint"),
        },
        Card {
            title: t(lang, "ui-flow-returned"),
            value: opt_f64(s, "returned_pct").map(|p| format!("{p:.0} %")),
            note: opt_f64(s, "returns")
                .map(|n| tr(lang, &Msg::new("ui-flow-returns").arg("n", n as i64))),
            hint: t(lang, "ui-flow-returned-hint"),
        },
        Card {
            title: t(lang, "ui-flow-wip"),
            value: opt_f64(s, "wip_avg").map(|n| format!("{n:.1}")),
            note: None,
            hint: t(lang, "ui-flow-wip-hint"),
        },
    ];
    let gap = ui.spacing().item_spacing.x;
    let avail = ui.available_width();
    // Filas parejas: 6, 3 o 2 por fila (o una) según quepan.
    let fits = ((avail + gap) / (CARD_MIN + gap)).floor() as usize;
    let per_row = [6, 3, 2].into_iter().find(|n| *n <= fits).unwrap_or(1);
    let width = ((avail - gap * (per_row as f32 - 1.0)) / per_row as f32).floor() - 1.0;
    for row in list.chunks(per_row) {
        ui.horizontal(|ui| {
            for c in row {
                card(ui, width, c);
            }
        });
    }
}

/// Una tarjeta; para la accesibilidad (y los tests) se anuncia como
/// «título: valor».
fn card(ui: &mut Ui, width: f32, c: &Card) {
    let visuals = ui.visuals();
    let value = c.value.as_deref().unwrap_or("—");
    let frame = egui::Frame::new()
        .fill(visuals.faint_bg_color)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(6.0)
        .inner_margin(CARD_MARGIN)
        .show(ui, |ui| {
            ui.set_width((width - CARD_CHROME).max(40.0));
            ui.vertical(|ui| {
                ui.add(egui::Label::new(RichText::new(&c.title).weak()).truncate());
                let big = RichText::new(value).size(24.0);
                ui.label(if c.value.is_some() {
                    big.strong()
                } else {
                    big.weak()
                });
                let note = c.note.as_deref().unwrap_or(" ");
                ui.add(egui::Label::new(RichText::new(note).small().weak()).truncate());
            });
        });
    let label = format!("{}: {value}", c.title);
    let response = frame.response.interact(egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, &label));
    response.on_hover_text(&c.hint);
}

fn array<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn series(v: &Value) -> Vec<&Value> {
    array(v, "series")
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn dates(rows: &[&Value]) -> Vec<String> {
    rows.iter().map(|r| text(r, "date").to_owned()).collect()
}

/// Eje X con las fechas de la serie.
fn date_axis(
    dates: Vec<String>,
) -> impl Fn(egui_plot::GridMark, &std::ops::RangeInclusive<f64>) -> String {
    move |mark, _| {
        let i = mark.value.round();
        if (mark.value - i).abs() > 0.01 || i < 0.0 {
            return String::new();
        }
        dates
            .get(i as usize)
            .map(|d| d.chars().skip(5).collect())
            .unwrap_or_default()
    }
}

/// Gráfico con fechas en el eje X; al pasar el ratón, la fecha y el valor.
fn plot(id: &str, dates: Vec<String>) -> Plot<'static> {
    let hover = dates.clone();
    Plot::new(id)
        .height(CHART_HEIGHT)
        .include_y(0.0)
        .allow_scroll(false)
        .x_axis_formatter(date_axis(dates))
        .label_formatter(move |pos| {
            let p = match pos {
                egui_plot::HoverPosition::NearDataPoint { position, .. } => position,
                egui_plot::HoverPosition::Elsewhere { position } => position,
            };
            let i = p.x.round();
            let date = (i >= 0.0).then(|| hover.get(i as usize)).flatten()?;
            Some(format!("{date}\n{:.0}", p.y))
        })
}

/// Hechas por día; con más de un mes, por semanas contadas desde el día
/// más reciente (la última barra es una semana entera; la primera puede no
/// serlo). Cada barra lleva la fecha de su primer día.
fn buckets(rows: &[&Value]) -> (bool, Vec<(String, f64)>) {
    let weekly = rows.len() > DAILY_MAX;
    let step = if weekly { 7 } else { 1 };
    let mut out: Vec<(String, f64)> = rows
        .rchunks(step)
        .map(|c| {
            let first = c.first().map(|r| text(r, "date")).unwrap_or_default();
            (first.to_owned(), c.iter().map(|r| num(r, "done")).sum())
        })
        .collect();
    out.reverse();
    (weekly, out)
}

fn throughput(ui: &mut Ui, lang: Lang, flow: &Value) {
    let rows = array(flow, "throughput");
    let (weekly, buckets) = buckets(&rows);
    let title = if weekly {
        "ui-throughput-week"
    } else {
        "ui-throughput-day"
    };
    section(ui, &t(lang, title));
    if buckets.is_empty() {
        no_data(ui, lang);
        return;
    }
    let bars: Vec<Bar> = buckets
        .iter()
        .enumerate()
        .map(|(i, (date, n))| Bar::new(i as f64, *n).name(date).width(0.7))
        .collect();
    let name = t(lang, title);
    plot("throughput", buckets.into_iter().map(|(d, _)| d).collect()).show(ui, |plot| {
        plot.bar_chart(BarChart::new(name, bars).color(theme::GREEN));
    });
}

fn burndown(ui: &mut Ui, lang: Lang, v: &Value) {
    let rows = series(v);
    if rows.is_empty() {
        no_data(ui, lang);
        return;
    }
    let points = |key: &str| -> Vec<[f64; 2]> {
        rows.iter()
            .enumerate()
            .map(|(i, r)| [i as f64, num(r, key)])
            .collect()
    };
    plot("burndown", dates(&rows))
        .legend(Legend::default())
        .show(ui, |plot| {
            plot.line(
                Line::new(t(lang, "ui-remaining"), points("remaining"))
                    .color(theme::ORANGE)
                    .width(2.0),
            );
            plot.line(
                Line::new(t(lang, "status-done"), points("done"))
                    .color(theme::GREEN)
                    .width(2.0),
            );
        });
}

/// Flujo acumulado: cada estado apilado sobre los cerrados, de abajo arriba.
fn cfd(ui: &mut Ui, lang: Lang, v: &Value) {
    let rows = series(v);
    if rows.is_empty() {
        no_data(ui, lang);
        return;
    }
    let order = [
        model::Status::Done,
        model::Status::InReview,
        model::Status::InProgress,
        model::Status::Todo,
        model::Status::Backlog,
    ];
    // Se apilan de abajo arriba y se dibujan de arriba abajo: la leyenda
    // sigue el orden del gráfico.
    let mut base = vec![0.0; rows.len()];
    let mut layers = Vec::new();
    for s in order {
        let points: Vec<[f64; 2]> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let top = base.get(i).copied().unwrap_or(0.0) + num(r, s.as_str());
                if let Some(b) = base.get_mut(i) {
                    *b = top;
                }
                [i as f64, top]
            })
            .collect();
        layers.push((s, points));
    }
    plot("cfd", dates(&rows))
        .legend(Legend::default().follow_insertion_order(true))
        .show(ui, |plot| {
            for (s, points) in layers.into_iter().rev() {
                plot.line(
                    Line::new(t(lang, s.label_key()), points)
                        .color(theme::status(s))
                        .fill(0.0)
                        .fill_alpha(0.25),
                );
            }
        });
}

/// Minutos de `key` con `fmt`, o «—» si no hay dato.
fn minutes_cell(ui: &mut Ui, v: &Value, key: &str, fmt: fn(i64) -> String) {
    match opt_f64(v, key) {
        Some(m) => ui.label(fmt(m as i64)),
        None => ui.weak("—"),
    };
}

/// Qué mide cada cabecera que no se explica sola.
fn header_hint(key: &str) -> Option<&'static str> {
    Some(match key {
        "ui-flow-worked" => "ui-flow-worked-hint",
        "ui-flow-queued" => "ui-flow-queued-hint",
        "ui-flow-returns-received" => "ui-flow-returns-received-hint",
        "ui-flow-returns-sent" => "ui-flow-returns-sent-hint",
        "ui-flow-avg-time" => "ui-flow-avg-time-hint",
        _ => return None,
    })
}

fn table(ui: &mut Ui, id: &str, lang: Lang, headers: &[&'static str], rows: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::horizontal().id_salt(id).show(ui, |ui| {
        egui::Grid::new(id)
            .striped(true)
            .spacing([14.0, 4.0])
            .show(ui, |ui| {
                for h in headers {
                    let r = ui.strong(t(lang, h));
                    if let Some(hint) = header_hint(h) {
                        r.on_hover_text(t(lang, hint));
                    }
                }
                ui.end_row();
                rows(ui);
            });
    });
}

/// Por agente: hechas, tiempo trabajado y en su cola, y devoluciones.
fn agents(ui: &mut Ui, lang: Lang, flow: &Value) {
    let items = array(flow, "agents");
    if items.is_empty() {
        no_data(ui, lang);
        return;
    }
    let headers = [
        "ui-col-agent",
        "ui-flow-done",
        "ui-flow-worked",
        "ui-flow-queued",
        "ui-flow-returns-received",
        "ui-flow-returns-sent",
    ];
    table(ui, "flow_agents", lang, &headers, |ui| {
        for r in &items {
            let name = match text(r, "actor_name") {
                "" => text(r, "actor"),
                n => n,
            };
            ui.label(name);
            ui.label(format!("{:.0}", num(r, "done")));
            minutes_cell(ui, r, "worked_minutes", hours);
            minutes_cell(ui, r, "queued_minutes", duration);
            let received = num(r, "returns_received");
            let text = RichText::new(format!("{received:.0}"));
            ui.label(if received > 0.0 {
                text.color(theme::ORANGE)
            } else {
                text
            });
            ui.label(format!("{:.0}", num(r, "returns_sent")));
            ui.end_row();
        }
    });
}

/// Por columna del flujo: su dueño, cuántas tareas tiene ahora y el tiempo
/// medio que pasa una tarea en ella.
fn columns(ui: &mut Ui, lang: Lang, flow: &Value) {
    let items = array(flow, "columns");
    if items.is_empty() {
        no_data(ui, lang);
        return;
    }
    let headers = [
        "ui-flow-column",
        "ui-flow-owner",
        "ui-flow-tasks-now",
        "ui-flow-avg-time",
    ];
    table(ui, "flow_columns", lang, &headers, |ui| {
        for r in &items {
            let base = text(r, "base").parse::<model::Status>().ok();
            ui.horizontal(|ui| {
                icons::dot(ui, base.map(theme::status).unwrap_or(theme::GRAY));
                let name = match text(r, "name") {
                    "" => text(r, "key"),
                    n => n,
                };
                ui.label(common::column_name(lang, name));
            });
            match text(r, "owner") {
                "" => ui.weak("—"),
                o => ui.label(o),
            };
            ui.label(format!("{:.0}", num(r, "tasks_now")));
            minutes_cell(ui, r, "avg_minutes", duration);
            ui.end_row();
        }
    });
}

fn sheet(ui: &mut Ui, lang: Lang, v: &Value) {
    let rows = array(v, "rows");
    if rows.is_empty() {
        ui.weak(t(lang, "ui-no-time"));
        return;
    }
    let totals = array(v, "totals");
    // Fila de total de un periodo, bajo sus filas.
    let total_row = |ui: &mut Ui, period: &str| {
        let minutes = totals
            .iter()
            .find(|x| text(x, "period") == period)
            .map(|x| num(x, "minutes"))
            .unwrap_or(0.0);
        for _ in 0..3 {
            ui.weak("");
        }
        ui.strong(t(lang, "ui-total"));
        ui.strong(hours(minutes as i64));
        ui.end_row();
    };
    egui::Grid::new("timesheet")
        .striped(true)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for h in [
                "ui-period",
                "ui-col-project",
                "ui-col-task",
                "ui-col-agent",
                "ui-perf-time",
            ] {
                ui.strong(t(lang, h));
            }
            ui.end_row();
            let mut last = "";
            for r in &rows {
                let period = text(r, "period");
                if period != last {
                    if !last.is_empty() {
                        total_row(ui, last);
                    }
                    last = period;
                }
                ui.label(period);
                for k in ["project", "task", "actor"] {
                    ui.label(text(r, k));
                }
                ui.label(hours(num(r, "minutes") as i64));
                ui.end_row();
            }
            total_row(ui, last);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days(n: usize) -> Vec<Value> {
        (0..n)
            .map(|i| serde_json::json!({"date": format!("d{i:02}"), "done": 1}))
            .collect()
    }

    #[test]
    fn up_to_a_month_goes_by_day() {
        let rows = days(30);
        let refs: Vec<&Value> = rows.iter().collect();
        let (weekly, b) = buckets(&refs);
        assert!(!weekly);
        assert_eq!(b.len(), 30);
    }

    /// Las semanas se cuentan desde el día más reciente: la última barra es
    /// una semana entera y lo que sobra queda en la primera.
    #[test]
    fn weeks_count_back_from_the_last_day() {
        let rows = days(90);
        let refs: Vec<&Value> = rows.iter().collect();
        let (weekly, b) = buckets(&refs);
        assert!(weekly);
        assert_eq!(b.len(), 13);
        assert_eq!(b.last(), Some(&("d83".to_owned(), 7.0)));
        assert_eq!(b.first(), Some(&("d00".to_owned(), 6.0)));
        let total: f64 = b.iter().map(|(_, n)| n).sum();
        assert_eq!(total, 90.0);
    }

    #[test]
    fn base_keys_and_deleted_stages_read_well() {
        assert_eq!(common::column_name(Lang::Es, "in_progress"), "En progreso");
        assert_eq!(common::column_name(Lang::Es, "cola_de_dev"), "Cola de dev");
        assert_eq!(
            common::column_name(Lang::Es, "Mesa de revisor"),
            "Mesa de revisor"
        );
        assert_eq!(common::duration(90), "1:30");
        assert_eq!(common::duration(3 * 24 * 60 + 4 * 60 + 5), "3 d 4 h");
    }
}
