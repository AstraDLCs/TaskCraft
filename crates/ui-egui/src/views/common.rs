//! Piezas compartidas por las vistas: selectores, avisos y tiempos relativos.

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, Msg, t, tr};
use model::{Actor, Priority, Task, Timestamp};

use crate::data::{Snapshot, TaskRow, Who};
use crate::state::Intent;
use crate::theme;
use crate::widgets::avatar;
use crate::widgets::icons::{self, Icon};

pub fn priority_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    current: Priority,
    lang: Lang,
) -> Option<Priority> {
    let mut chosen = current;
    egui::ComboBox::from_id_salt(id)
        .selected_text(RichText::new(t(lang, current.label_key())).color(theme::priority(current)))
        .width(100.0)
        .show_ui(ui, |ui| {
            for &p in Priority::ALL {
                ui.selectable_value(
                    &mut chosen,
                    p,
                    RichText::new(t(lang, p.label_key())).color(theme::priority(p)),
                );
            }
        });
    (chosen != current).then_some(chosen)
}

/// Asignado: nadie, el supervisor o un agente activo. `Some(nuevo)` si cambió.
/// Asignados de una tarea en poco sitio: los nombres y, al abrirlo, una
/// casilla por agente para sumarlo o quitarlo; puede tener varios (D20).
pub fn assignees_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    task: &Task,
    key: &str,
    snap: &Snapshot,
    lang: Lang,
    out: &mut Vec<Intent>,
) {
    let names: Vec<String> = task.assignees.iter().map(|a| actor_name(snap, a)).collect();
    let text = match names.as_slice() {
        [] => t(lang, "ui-unassigned"),
        [one] => one.clone(),
        [first, rest @ ..] => format!("{first} +{}", rest.len()),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(120.0)
        .show_ui(ui, |ui| {
            assignee_checks(ui, task, key, snap, out);
        })
        .response
        .on_hover_text(names.join(", "));
}

/// Una casilla por agente activo (y el supervisor) para sumarlo o quitarlo.
pub fn assignee_checks(
    ui: &mut Ui,
    task: &Task,
    key: &str,
    snap: &Snapshot,
    out: &mut Vec<Intent>,
) {
    let agents = snap
        .agents
        .iter()
        .filter(|a| a.agent.status == model::AgentStatus::Active)
        .map(|a| {
            (
                Actor::Agent(a.agent.id),
                a.agent.name.clone(),
                a.agent.description.clone(),
            )
        });
    let options =
        std::iter::once((Actor::Supervisor, "supervisor".to_owned(), String::new())).chain(agents);
    for (actor, name, function) in options {
        let mut on = task.is_assigned(&actor);
        let check = ui.checkbox(&mut on, &name);
        let check = if function.trim().is_empty() {
            check
        } else {
            check.on_hover_text(function.trim())
        };
        if check.changed() {
            let command = if on {
                "task.add_assignee"
            } else {
                "task.remove_assignee"
            };
            out.push(Intent::run(
                command,
                &[("task", key), ("agent", &actor.to_string())],
            ));
        }
    }
}

/// Ciclos a los que se puede sumar la tarea (los abiertos de su proyecto) y
/// los cerrados en los que ya está, por fecha de inicio.
pub fn cycles_for<'s>(snap: &'s Snapshot, task: &Task) -> Vec<&'s model::Cycle> {
    let mut cycles: Vec<_> = snap
        .cycles
        .iter()
        .filter(|c| {
            c.project_id == task.project_id && (c.closed_at.is_none() || task.in_cycle(c.id))
        })
        .collect();
    cycles.sort_by_key(|c| c.start);
    cycles
}

/// Una casilla por ciclo para sumar la tarea o sacarla; puede estar en varios.
pub fn cycle_checks(ui: &mut Ui, task: &Task, key: &str, snap: &Snapshot, out: &mut Vec<Intent>) {
    for c in cycles_for(snap, task) {
        let mut on = task.in_cycle(c.id);
        let text = if c.closed_at.is_some() {
            RichText::new(&c.name).weak()
        } else {
            RichText::new(&c.name)
        };
        let check = ui
            .checkbox(&mut on, text)
            .on_hover_text(format!("{} – {}", c.start, c.end));
        if check.changed() {
            let command = if on {
                "task.add_cycle"
            } else {
                "task.remove_cycle"
            };
            out.push(Intent::run(
                command,
                &[("task", key), ("cycle", &c.id.to_string())],
            ));
        }
    }
}

/// Nombre visible de un actor.
pub fn actor_name(snap: &Snapshot, actor: &Actor) -> String {
    match actor {
        Actor::Agent(id) => snap
            .agents
            .iter()
            .find(|r| r.agent.id == *id)
            .map_or_else(|| id.to_string(), |r| r.agent.name.clone()),
        other => other.to_string(),
    }
}

pub fn who(ui: &mut Ui, who: &Who, size: f32) {
    ui.horizontal(|ui| {
        avatar::avatar(ui, &who.seed, who.color, size);
        ui.label(&who.name);
    });
}

/// Avatar de un agente por su ID, con su nombre y su función al pasar por encima.
pub fn agent_avatar(ui: &mut Ui, snap: &Snapshot, id: model::AgentId, size: f32) -> egui::Response {
    match snap.agents.iter().find(|r| r.agent.id == id) {
        Some(r) => {
            let a = &r.agent;
            let hover = if a.description.trim().is_empty() {
                a.name.clone()
            } else {
                format!("{}\n{}", a.name, a.description.trim())
            };
            avatar::avatar(ui, &a.id.to_string(), theme::parse_hex(&a.color), size)
                .on_hover_text(hover)
        }
        None => avatar::avatar(ui, &id.to_string(), theme::GRAY, size),
    }
}

pub fn who_avatar(ui: &mut Ui, who: &Who, size: f32) -> egui::Response {
    avatar::avatar(ui, &who.seed, who.color, size).on_hover_text(who.hover())
}

pub fn ago(lang: Lang, at: Timestamp) -> String {
    let secs = Timestamp::now().duration_since(at).as_secs().max(0);
    let (key, n) = match secs {
        0..60 => ("ui-ago-now", 0),
        60..3600 => ("ui-ago-min", secs / 60),
        3600..86_400 => ("ui-ago-hours", secs / 3600),
        _ => ("ui-ago-days", secs / 86_400),
    };
    tr(lang, &Msg::new(key).arg("n", n))
}

pub fn remaining(lang: Lang, until: Timestamp) -> String {
    let mins = until.duration_since(Timestamp::now()).as_secs().max(0) / 60;
    tr(lang, &Msg::new("ui-lease-left").arg("n", mins))
}

/// Avisos de la tarea: bloqueada, escalada, con bloqueo humano, tomada.
pub fn badges(ui: &mut Ui, row: &TaskRow, lang: Lang) {
    if row.blocked() {
        icons::show(ui, Icon::Warning, theme::RED).on_hover_text(t(lang, "ui-blocked-hint"));
    }
    if row.task.needs_human.is_some() {
        icons::show(ui, Icon::Warning, theme::VIOLET)
            .on_hover_text(row.task.needs_human.clone().unwrap_or_default());
    }
    if row.task.locked {
        icons::show(ui, Icon::Lock, theme::GRAY).on_hover_text(t(lang, "ui-locked-hint"));
    }
    if row.stale {
        let weeks = rules::idle_weeks(&row.task, Timestamp::now());
        icons::show(ui, Icon::Hourglass, theme::ORANGE).on_hover_text(tr(
            lang,
            &Msg::new("ui-stale-hint").arg("weeks", u64::try_from(weeks).unwrap_or(0)),
        ));
    }
    if let Some(rule) = &row.task.recurrence {
        icons::show(ui, Icon::Repeat, theme::GRAY)
            .on_hover_text(rules::recurrence::describe(rule, lang));
    }
    if let Some(claim) = &row.claim {
        let soon = claim.until.duration_since(Timestamp::now()).as_secs() < 180;
        let color = if soon { theme::ORANGE } else { theme::GRAY };
        icons::show(ui, Icon::Clock, color).on_hover_text(format!(
            "{} · {}",
            claim.agent.name,
            remaining(lang, claim.until)
        ));
    }
}

/// Fecha de vencimiento; en rojo si ya pasó y la tarea sigue abierta.
pub fn due_text(task: &Task) -> Option<RichText> {
    let due = task.due.as_ref()?;
    let date = due.date();
    let overdue =
        date < crate::widgets::date_picker::today() && !task.status.category().is_closed();
    let text = RichText::new(date.to_string()).small();
    Some(if overdue {
        text.color(theme::RED)
    } else {
        text
    })
}

pub fn progress(row: &TaskRow) -> Option<String> {
    let mut parts = Vec::new();
    if row.children_total > 0 {
        parts.push(format!("{}/{}", row.children_done, row.children_total));
    }
    let checklist = &row.task.checklist;
    if !checklist.is_empty() {
        let done = checklist.iter().filter(|i| i.done).count();
        parts.push(format!("[{done}/{}]", checklist.len()));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Título clicable de una tarea: selecciona al hacer clic.
pub fn title_link(ui: &mut Ui, row: &TaskRow, selected: bool, out: &mut Vec<Intent>) {
    let mut text = RichText::new(&row.task.title);
    if row.is_closed() {
        text = text.strikethrough().weak();
    }
    if ui.selectable_label(selected, text).clicked() {
        out.push(Intent::Select(Some(row.id())));
    }
}

/// Nombre de una columna del flujo: los estados base llegan con su código
/// (`in_progress`) y se traducen; una etapa ya trae su nombre. Una etapa
/// borrada llega con su clave (`cola_de_dev`) y se deja legible.
pub fn column_name(lang: Lang, name: &str) -> String {
    match name.parse::<model::Status>() {
        Ok(s) if s.as_str() == name => t(lang, s.label_key()),
        _ if !name.contains(' ') && name.contains('_') => {
            let spaced = name.replace('_', " ");
            let mut chars = spaced.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => spaced,
            }
        }
        _ => name.to_owned(),
    }
}

/// Tiempo trabajado: `h:mm` (las horas pueden pasar de 24).
pub fn hours(minutes: i64) -> String {
    let m = minutes.max(0);
    format!("{}:{:02}", m / 60, m % 60)
}

/// Tiempo transcurrido: `h:mm` hasta un día; desde ahí, `3 d 4 h`.
pub fn duration(minutes: i64) -> String {
    let m = minutes.max(0);
    if m < 24 * 60 {
        hours(m)
    } else {
        format!("{} d {} h", m / (24 * 60), m % (24 * 60) / 60)
    }
}

/// Fecha y hora locales, como el resto del detalle: `2026-10-27 14:05`.
pub fn date_time(at: Timestamp) -> String {
    at.to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}
