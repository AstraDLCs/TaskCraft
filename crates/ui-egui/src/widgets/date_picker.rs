//! Selector de fecha propio (mes en cuadrícula), sin dependencias extra.

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, t};
use jiff::civil::Date;

/// Nombres cortos de los días, de lunes a domingo.
pub fn weekday_keys() -> [&'static str; 7] {
    [
        "ui-wd-mon",
        "ui-wd-tue",
        "ui-wd-wed",
        "ui-wd-thu",
        "ui-wd-fri",
        "ui-wd-sat",
        "ui-wd-sun",
    ]
}

pub fn month_key(month: i8) -> &'static str {
    match month {
        1 => "ui-month-1",
        2 => "ui-month-2",
        3 => "ui-month-3",
        4 => "ui-month-4",
        5 => "ui-month-5",
        6 => "ui-month-6",
        7 => "ui-month-7",
        8 => "ui-month-8",
        9 => "ui-month-9",
        10 => "ui-month-10",
        11 => "ui-month-11",
        _ => "ui-month-12",
    }
}

pub fn today() -> Date {
    jiff::Zoned::now().date()
}

pub fn add_days(d: Date, n: i64) -> Date {
    d.checked_add(jiff::Span::new().days(n)).unwrap_or(d)
}

pub fn add_months(d: Date, n: i64) -> Date {
    d.first_of_month()
        .checked_add(jiff::Span::new().months(n))
        .unwrap_or(d)
}

/// Lunes de la semana en la que empieza el mes (primera celda de la cuadrícula).
pub fn grid_start(month: Date) -> Date {
    let first = month.first_of_month();
    add_days(first, -i64::from(first.weekday().to_monday_zero_offset()))
}

/// Botón con la fecha; al pulsarlo, abre un calendario. Devuelve `Some(nuevo)`
/// si cambió (`Some(None)` = borrada).
pub fn date_button(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: Option<Date>,
    lang: Lang,
) -> Option<Option<Date>> {
    let label = value.map_or_else(|| t(lang, "ui-no-date"), |d| d.to_string());
    let response = ui.button(label);
    let state_id = ui.make_persistent_id(("date-picker-month", &id));
    let mut month: Date = ui
        .data(|d| d.get_temp(state_id))
        .unwrap_or_else(|| value.unwrap_or_else(today).first_of_month());
    let mut chosen = None;
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.horizontal(|ui| {
                if ui.small_button("<").clicked() {
                    month = add_months(month, -1);
                }
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        t(lang, month_key(month.month())),
                        month.year()
                    ))
                    .strong(),
                );
                if ui.small_button(">").clicked() {
                    month = add_months(month, 1);
                }
            });
            egui::Grid::new(("date-grid", ui.scope_id())).show(ui, |ui| {
                for key in weekday_keys() {
                    ui.label(RichText::new(t(lang, key)).small().weak());
                }
                ui.end_row();
                let start = grid_start(month);
                for week in 0..6 {
                    for day in 0..7 {
                        let d = add_days(start, week * 7 + day);
                        let mut text = RichText::new(d.day().to_string());
                        if d.month() != month.month() {
                            text = text.weak();
                        }
                        if d == today() {
                            text = text.underline();
                        }
                        if ui.selectable_label(Some(d) == value, text).clicked() {
                            chosen = Some(Some(d));
                        }
                    }
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if ui.button(t(lang, "ui-today")).clicked() {
                    chosen = Some(Some(today()));
                }
                if value.is_some() && ui.button(t(lang, "ui-clear-date")).clicked() {
                    chosen = Some(None);
                }
            });
        });
    ui.data_mut(|d| d.insert_temp(state_id, month));
    if chosen.is_some() {
        egui::Popup::close_all(ui.ctx());
    }
    chosen
}
