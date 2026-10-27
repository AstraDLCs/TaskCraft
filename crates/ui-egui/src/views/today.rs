//! Vista «Hoy» (RF-VIS-08): atrasadas, las que vencen hoy y las marcadas a
//! mano. Mismo criterio que `today.list` en el engine.

use eframe::egui::{self, RichText, Ui};
use i18n::t;

use super::common;
use crate::data::{Snapshot, TaskRow};
use crate::state::{Intent, UiState, flat_rows};
use crate::theme;
use crate::widgets::icons::{self, Icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Overdue,
    Due,
    Marked,
}

/// Por qué está una tarea en «Hoy», si lo está.
pub fn why(snap: &Snapshot, row: &TaskRow, today: model::Date) -> Option<Why> {
    if row.is_closed() {
        return None;
    }
    match row.task.due.as_ref().map(model::When::date) {
        Some(d) if d < today => Some(Why::Overdue),
        Some(d) if d == today => Some(Why::Due),
        _ if snap.today.contains(&row.id()) => Some(Why::Marked),
        _ => None,
    }
}

/// Marcar o desmarcar «para hoy».
pub fn toggle(snap: &Snapshot, row: &TaskRow) -> Intent {
    let id = if snap.today.contains(&row.id()) {
        "today.unmark"
    } else {
        "today.mark"
    };
    Intent::run(id, &[("task", &row.key)])
}

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let today = rules::filter::today();
    let rows = flat_rows(snap, st, false);
    let mut sections: [(Why, &str, Vec<&TaskRow>); 3] = [
        (Why::Overdue, "ui-today-overdue", Vec::new()),
        (Why::Due, "ui-today-due", Vec::new()),
        (Why::Marked, "ui-today-marked", Vec::new()),
    ];
    for row in rows {
        if let Some(w) = why(snap, row, today)
            && let Some((_, _, list)) = sections.iter_mut().find(|(s, _, _)| *s == w)
        {
            list.push(row);
        }
    }
    if sections.iter().all(|(_, _, l)| l.is_empty()) {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-today-empty")));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("today")
        .show(ui, |ui| {
            for (kind, title, mut list) in sections {
                if list.is_empty() {
                    continue;
                }
                list.sort_by_key(|r| {
                    (
                        r.task.priority.rank(),
                        r.task.due.as_ref().map(model::When::date),
                    )
                });
                let color = match kind {
                    Why::Overdue => theme::RED,
                    Why::Due => theme::ORANGE,
                    Why::Marked => theme::BLUE,
                };
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("{} · {}", t(lang, title), list.len()))
                        .strong()
                        .color(color),
                );
                ui.separator();
                for row in list {
                    ui.horizontal(|ui| {
                        let marked = snap.today.contains(&row.id());
                        let tip = t(
                            lang,
                            if marked {
                                "ui-today-unmark"
                            } else {
                                "ui-today-mark"
                            },
                        );
                        if icons::toggle(ui, Icon::Sun, marked, theme::ORANGE, &tip).clicked() {
                            out.push(toggle(snap, row));
                        }
                        ui.label(RichText::new(&row.key).monospace().weak());
                        common::title_link(ui, row, st.selected == Some(row.id()), out);
                        if let Some(due) = common::due_text(&row.task) {
                            ui.label(due);
                        }
                        ui.weak(t(lang, row.task.priority.label_key()));
                        common::badges(ui, row, lang);
                    });
                }
            }
        });
}
