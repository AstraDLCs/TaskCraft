//! Métricas (RF-INT-06) y hojas de horas (RF-HIS-05): burndown y flujo
//! acumulado del proyecto, rendimiento por agente y tiempo por periodo.
//! Se calculan del historial al entrar en la vista o al pedirlo.

use eframe::egui::{self, RichText, Ui};
use egui_plot::{Legend, Line, Plot};
use i18n::{Msg, t, tr};
use serde_json::Value;

use crate::data::{Scope, Snapshot};
use crate::state::{Intent, MetricsTab, UiState};
use crate::theme;

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
    egui::ScrollArea::vertical()
        .id_salt("metrics")
        .show(ui, |ui| match st.metrics.tab {
            MetricsTab::Charts => {
                match st.scope {
                    Scope::Project(id) => {
                        let key = snap.project(id).map(|p| p.key.clone()).unwrap_or_default();
                        ui.heading(tr(
                            lang,
                            &Msg::new("ui-burndown").arg("project", key.as_str()),
                        ));
                        burndown(ui, lang, &data.burndown);
                        ui.add_space(8.0);
                        ui.heading(t(lang, "ui-cfd"));
                        cfd(ui, lang, &data.cfd);
                    }
                    Scope::All => {
                        ui.weak(t(lang, "ui-metrics-pick-project"));
                    }
                }
                ui.add_space(8.0);
                ui.heading(t(lang, "ui-agent-performance"));
                agents(ui, lang, &data.agents);
                ui.add_space(8.0);
                ui.heading(t(lang, "ui-cost-by-agent"));
                cost(ui, lang, &data.cost);
            }
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

fn series(v: &Value) -> Vec<&Value> {
    v.get("series")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
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

fn burndown(ui: &mut Ui, lang: i18n::Lang, v: &Value) {
    let rows = series(v);
    let dates: Vec<String> = rows
        .iter()
        .map(|r| {
            r.get("date")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let points = |key: &str| -> Vec<[f64; 2]> {
        rows.iter()
            .enumerate()
            .map(|(i, r)| [i as f64, num(r, key)])
            .collect()
    };
    Plot::new("burndown")
        .height(220.0)
        .legend(Legend::default())
        .include_y(0.0)
        .allow_scroll(false)
        .x_axis_formatter(date_axis(dates))
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
fn cfd(ui: &mut Ui, lang: i18n::Lang, v: &Value) {
    let rows = series(v);
    let dates: Vec<String> = rows
        .iter()
        .map(|r| {
            r.get("date")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    let order = [
        model::Status::Done,
        model::Status::InReview,
        model::Status::InProgress,
        model::Status::Todo,
        model::Status::Backlog,
    ];
    Plot::new("cfd")
        .height(220.0)
        .legend(Legend::default())
        .include_y(0.0)
        .allow_scroll(false)
        .x_axis_formatter(date_axis(dates))
        .show(ui, |plot| {
            let mut base = vec![0.0; rows.len()];
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
                plot.line(
                    Line::new(t(lang, s.label_key()), points)
                        .color(theme::status(s))
                        .fill(0.0)
                        .fill_alpha(0.25),
                );
            }
        });
}

fn hours(minutes: f64) -> String {
    let m = minutes.max(0.0) as i64;
    format!("{}:{:02}", m / 60, m % 60)
}

fn agents(ui: &mut Ui, lang: i18n::Lang, v: &Value) {
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        ui.weak(t(lang, "ui-agents-empty"));
        return;
    }
    egui::Grid::new("agent_perf")
        .striped(true)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for h in [
                "ui-col-agent",
                "ui-perf-submitted",
                "ui-perf-approved",
                "ui-perf-rejected",
                "ui-perf-rejection-rate",
                "ui-perf-cycle",
                "ui-perf-time",
            ] {
                ui.strong(t(lang, h));
            }
            ui.end_row();
            for r in &items {
                ui.label(r.get("agent").and_then(Value::as_str).unwrap_or_default());
                for k in ["submitted", "approved", "rejected"] {
                    ui.label(num(r, k).to_string());
                }
                match r.get("rejection_rate").and_then(Value::as_f64) {
                    Some(rate) => {
                        let text = RichText::new(format!("{:.0} %", rate * 100.0));
                        ui.label(if rate > 0.3 {
                            text.color(theme::RED)
                        } else {
                            text
                        })
                    }
                    None => ui.weak("—"),
                };
                match r.get("avg_cycle_minutes").and_then(Value::as_f64) {
                    Some(m) => ui.label(hours(m)),
                    None => ui.weak("—"),
                };
                ui.label(hours(num(r, "minutes")));
                ui.end_row();
            }
        });
}

/// Coste reportado por agente (RF-HIS-06): tokens e importe por moneda.
fn cost(ui: &mut Ui, lang: i18n::Lang, v: &Value) {
    let groups = v
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if groups.is_empty() {
        ui.weak(t(lang, "ui-cost-empty"));
        return;
    }
    let amounts = |g: &Value| -> String {
        g.get("amounts")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|a| {
                        let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
                        format!("{} {}", s("amount"), s("currency"))
                    })
                    .collect::<Vec<_>>()
                    .join(" + ")
            })
            .unwrap_or_default()
    };
    egui::Grid::new("agent_cost")
        .striped(true)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for h in [
                "ui-col-agent",
                "ui-cost-reports",
                "param-tokens_in",
                "param-tokens_out",
                "ui-cost",
            ] {
                ui.strong(t(lang, h));
            }
            ui.end_row();
            let total = v.get("total").cloned().unwrap_or(Value::Null);
            for g in groups.iter().chain([&total]) {
                let is_total = std::ptr::eq(g, &total);
                let name = match g.get("key").and_then(Value::as_str) {
                    Some(k) => k.to_owned(),
                    None => t(lang, "ui-total"),
                };
                let cells = [
                    name,
                    num(g, "entries").to_string(),
                    num(g, "tokens_in").to_string(),
                    num(g, "tokens_out").to_string(),
                    amounts(g),
                ];
                for c in cells {
                    if is_total {
                        ui.strong(c);
                    } else {
                        ui.label(c);
                    }
                }
                ui.end_row();
            }
        });
}

fn sheet(ui: &mut Ui, lang: i18n::Lang, v: &Value) {
    let rows = v
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        ui.weak(t(lang, "ui-no-time"));
        return;
    }
    let totals = v
        .get("totals")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
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
            let mut last = String::new();
            for r in &rows {
                let period = r.get("period").and_then(Value::as_str).unwrap_or_default();
                if period != last {
                    if !last.is_empty() {
                        let total = totals
                            .iter()
                            .find(|x| {
                                x.get("period").and_then(Value::as_str) == Some(last.as_str())
                            })
                            .map(|x| num(x, "minutes"))
                            .unwrap_or(0.0);
                        ui.weak("");
                        ui.weak("");
                        ui.weak("");
                        ui.strong(t(lang, "ui-total"));
                        ui.strong(hours(total));
                        ui.end_row();
                    }
                    last = period.to_owned();
                }
                ui.label(period);
                for k in ["project", "task", "actor"] {
                    ui.label(r.get(k).and_then(Value::as_str).unwrap_or_default());
                }
                ui.label(hours(num(r, "minutes")));
                ui.end_row();
            }
            let total = totals
                .iter()
                .find(|x| x.get("period").and_then(Value::as_str) == Some(last.as_str()))
                .map(|x| num(x, "minutes"))
                .unwrap_or(0.0);
            ui.weak("");
            ui.weak("");
            ui.weak("");
            ui.strong(t(lang, "ui-total"));
            ui.strong(hours(total));
            ui.end_row();
        });
}
