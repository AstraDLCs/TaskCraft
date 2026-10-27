//! Secciones del detalle añadidas con el modelo ampliado (M5): planificación
//! (estimación, épica, hito y campos propios), vínculos (relacionadas,
//! duplicadas y menciones) y adjuntos.

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, Msg, t, tr};
use model::{EpicKind, FieldKind};

use crate::data::{Detail, Snapshot, TaskRef};
use crate::state::{Intent, UiState};
use crate::theme;
use crate::views::common;
use crate::widgets::icons::{self, Icon};

/// Estimación, épica, hito y campos personalizados (RF-MOD-13/14/15).
pub fn planning(
    ui: &mut Ui,
    detail: &Detail,
    snap: &Snapshot,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let task = &detail.row.task;
    let key = detail.row.key.as_str();
    let Some(project) = snap.project(task.project_id) else {
        return;
    };
    egui::CollapsingHeader::new(t(lang, "ui-planning"))
        .id_salt(("planning", key))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("detail-planning")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.weak(format!(
                        "{} ({})",
                        t(lang, "ui-estimate"),
                        t(lang, project.estimate_unit.label_key())
                    ));
                    let current = task
                        .estimate
                        .map(|e| rules::format_estimate(project.estimate_unit, e))
                        .unwrap_or_default();
                    if st.edit.estimate.is_none() {
                        st.edit.estimate = Some(current.clone());
                    }
                    if let Some(text) = st.edit.estimate.as_mut() {
                        let edit = ui.add(egui::TextEdit::singleline(text).desired_width(80.0));
                        if edit.lost_focus() && text.trim() != current {
                            let value = if text.trim().is_empty() {
                                "none"
                            } else {
                                text.trim()
                            };
                            out.push(Intent::run(
                                "task.update",
                                &[("task", key), ("estimate", value)],
                            ));
                        }
                    }
                    ui.end_row();

                    for (kind, label, command, param, current) in [
                        (
                            EpicKind::Epic,
                            "ui-epic",
                            "task.set_epic",
                            "epic",
                            task.epic_id,
                        ),
                        (
                            EpicKind::Milestone,
                            "ui-milestone",
                            "task.set_milestone",
                            "milestone",
                            task.milestone_id,
                        ),
                    ] {
                        ui.weak(t(lang, label));
                        let groups = snap.groups_of(task.project_id, kind);
                        let name = current
                            .and_then(|id| snap.epic(id))
                            .map(|e| e.epic.name.clone())
                            .unwrap_or_else(|| t(lang, "ui-none"));
                        let mut chosen = current;
                        egui::ComboBox::from_id_salt((param, task.id))
                            .selected_text(name)
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut chosen, None, t(lang, "ui-none"));
                                for g in groups {
                                    ui.selectable_value(&mut chosen, Some(g.epic.id), &g.epic.name);
                                }
                            });
                        if chosen != current {
                            let value =
                                chosen.map_or_else(|| "none".to_owned(), |id| id.to_string());
                            out.push(Intent::run(command, &[("task", key), (param, &value)]));
                        }
                        ui.end_row();
                    }

                    ui.weak(t(lang, "ui-cycles"));
                    ui.vertical(|ui| cycles(ui, task, key, snap, lang, out));
                    ui.end_row();

                    ui.weak(t(lang, "ui-recurrence"));
                    recurrence(ui, task, key, st, out);
                    ui.end_row();

                    for f in &project.fields {
                        ui.weak(&f.name);
                        let value = task.fields.get(&f.key).cloned().unwrap_or_default();
                        field_editor(ui, st, f, &value, key, out);
                        ui.end_row();
                    }
                });
        });
}

/// Ciclos o sprints de la tarea (puede estar en varios), cada uno con su «×»,
/// y un menú con una casilla por ciclo del proyecto.
fn cycles(
    ui: &mut Ui,
    task: &model::Task,
    key: &str,
    snap: &Snapshot,
    lang: Lang,
    out: &mut Vec<Intent>,
) {
    let today = crate::widgets::date_picker::today();
    ui.horizontal_wrapped(|ui| {
        for c in task
            .cycles
            .iter()
            .filter_map(|id| snap.cycles.iter().find(|c| c.id == *id))
        {
            ui.horizontal(|ui| {
                let state = c.state(today);
                icons::show(ui, Icon::Cycle, crate::views::cycles::state_color(state));
                let name = if c.closed_at.is_some() {
                    RichText::new(&c.name).weak()
                } else {
                    RichText::new(&c.name)
                };
                ui.label(name)
                    .on_hover_text(format!("{} – {}", c.start, c.end));
                if icons::button_sized(ui, Icon::Close, true, &t(lang, "ui-remove"), 14.0).clicked()
                {
                    out.push(Intent::run(
                        "task.remove_cycle",
                        &[("task", key), ("cycle", &c.id.to_string())],
                    ));
                }
            });
            ui.add_space(6.0);
        }
        if task.cycles.is_empty() {
            ui.weak(t(lang, "ui-none"));
        }
    });
    if common::cycles_for(snap, task).is_empty() {
        ui.weak(t(lang, "ui-sidebar-no-cycles"));
    } else {
        ui.menu_button(t(lang, "ui-cycles-plan"), |ui| {
            common::cycle_checks(ui, task, key, snap, out);
        });
    }
}

/// Editor de un campo según su tipo; cada cambio es un `task.set_field`.
fn field_editor(
    ui: &mut Ui,
    st: &mut UiState,
    f: &model::FieldDef,
    value: &str,
    key: &str,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let set = |out: &mut Vec<Intent>, v: &str| {
        out.push(Intent::run(
            "task.set_field",
            &[("task", key), ("field", &f.key), ("value", v)],
        ));
    };
    match f.kind {
        FieldKind::Bool => {
            let mut on = value == "true";
            if ui.checkbox(&mut on, "").changed() {
                set(out, if on { "true" } else { "false" });
            }
        }
        FieldKind::Select => {
            let mut chosen = value.to_owned();
            egui::ComboBox::from_id_salt(("field", &f.key, key))
                .selected_text(if value.is_empty() {
                    t(lang, "ui-none")
                } else {
                    value.to_owned()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, String::new(), t(lang, "ui-none"));
                    for o in &f.options {
                        ui.selectable_value(&mut chosen, o.clone(), o);
                    }
                });
            if chosen != value {
                set(out, &chosen);
            }
        }
        FieldKind::Multiselect => {
            let selected: Vec<&str> = value.split(',').filter(|s| !s.is_empty()).collect();
            ui.horizontal_wrapped(|ui| {
                for o in &f.options {
                    let mut on = selected.contains(&o.as_str());
                    if ui.checkbox(&mut on, o).changed() {
                        let next: Vec<&str> = f
                            .options
                            .iter()
                            .map(String::as_str)
                            .filter(|x| {
                                if *x == o.as_str() {
                                    on
                                } else {
                                    selected.contains(x)
                                }
                            })
                            .collect();
                        set(out, &next.join(","));
                    }
                }
            });
        }
        FieldKind::Text | FieldKind::Number | FieldKind::Date | FieldKind::Url => {
            let text = st
                .edit
                .field_values
                .entry(f.key.clone())
                .or_insert_with(|| value.to_owned());
            let edit = ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text(t(lang, f.kind.label_key()))
                    .desired_width(180.0),
            );
            if edit.lost_focus() && text.trim() != value {
                let v = text.trim().to_owned();
                set(out, &v);
            }
            if f.kind == FieldKind::Url
                && !value.is_empty()
                && icons::button_sized(ui, Icon::External, true, &t(lang, "ui-open-link"), 16.0)
                    .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(value));
            }
        }
    }
}

fn link_ref(ui: &mut Ui, r: &TaskRef, lang: Lang, out: &mut Vec<Intent>) {
    icons::dot(ui, theme::status(r.status)).on_hover_text(t(lang, r.status.label_key()));
    ui.label(RichText::new(&r.key).monospace().weak());
    let mut text = RichText::new(&r.title);
    if r.status.category().is_closed() {
        text = text.strikethrough().weak();
    }
    if ui.link(text).clicked() {
        out.push(Intent::Select(Some(r.id)));
    }
}

/// Relacionadas, duplicadas y menciones entrantes (RF-REL-03/04).
pub fn links(
    ui: &mut Ui,
    detail: &Detail,
    snap: &Snapshot,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let key = detail.row.key.clone();
    let count = detail.related.len()
        + detail.duplicates.len()
        + detail.mentioned_by.len()
        + usize::from(detail.duplicate_of.is_some());
    egui::CollapsingHeader::new(format!("{} ({count})", t(lang, "ui-links")))
        .id_salt(("links", &key))
        .default_open(count > 0)
        .show(ui, |ui| {
            if let Some(original) = &detail.duplicate_of {
                ui.horizontal(|ui| {
                    ui.weak(t(lang, "relation-duplicates"));
                    link_ref(ui, original, lang, out);
                });
            }
            for (title, list) in [
                ("ui-related", &detail.related),
                ("ui-duplicated-by", &detail.duplicates),
                ("ui-mentioned-by", &detail.mentioned_by),
            ] {
                if list.is_empty() {
                    continue;
                }
                ui.weak(t(lang, title));
                for r in list {
                    ui.horizontal(|ui| {
                        link_ref(ui, r, lang, out);
                        if title == "ui-related"
                            && icons::button_sized(
                                ui,
                                Icon::Close,
                                true,
                                &t(lang, "ui-remove"),
                                12.0,
                            )
                            .clicked()
                        {
                            out.push(Intent::run(
                                "task.unrelate",
                                &[("task", &key), ("other", &r.key)],
                            ));
                        }
                    });
                }
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("relate-kind", &key))
                    .selected_text(t(lang, st.edit.relate_kind.label_key()))
                    .show_ui(ui, |ui| {
                        for k in model::RelationKind::ALL {
                            ui.selectable_value(
                                &mut st.edit.relate_kind,
                                *k,
                                t(lang, k.label_key()),
                            );
                        }
                    });
                let candidates =
                    crate::hierarchy::parent_candidates(snap, detail.row.id(), &st.parent_query);
                ui.menu_button(t(lang, "ui-relate-with"), |ui| {
                    if let Some(other) =
                        crate::hierarchy::task_picker(ui, &candidates, &mut st.parent_query, lang)
                    {
                        out.push(Intent::run(
                            "task.relate",
                            &[
                                ("task", &key),
                                ("other", &other),
                                ("kind", st.edit.relate_kind.as_str()),
                            ],
                        ));
                        st.parent_query.clear();
                        ui.close();
                    }
                });
            });
            ui.weak(t(lang, "ui-mention-hint"));
        });
}

/// Diagramas que cita la tarea (RF-MOD-19) y un menú para incrustar otro
/// del proyecto al final de la descripción. Sin diagramas en el proyecto ni
/// citas, no se muestra.
pub fn diagrams(ui: &mut Ui, detail: &Detail, snap: &Snapshot, lang: Lang, out: &mut Vec<Intent>) {
    let task = &detail.row.task;
    let cited: Vec<_> = snap.diagrams_cited_by(task.id).collect();
    let available: Vec<_> = snap.diagrams_of(task.project_id).collect();
    if cited.is_empty() && available.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("{} ({})", t(lang, "ui-view-diagrams"), cited.len()))
        .id_salt(("diagrams", &detail.row.key))
        .default_open(!cited.is_empty())
        .show(ui, |ui| {
            for d in &cited {
                ui.horizontal(|ui| {
                    icons::show(ui, Icon::Diagram, theme::GRAY);
                    if ui
                        .link(format!("{} · {}", d.key, d.diagram.title))
                        .clicked()
                    {
                        out.push(Intent::OpenDiagram(d.key.clone()));
                    }
                });
            }
            if !available.is_empty() {
                ui.menu_button(t(lang, "ui-diagram-embed"), |ui| {
                    for d in &available {
                        if ui
                            .button(format!("{} · {}", d.key, d.diagram.title))
                            .clicked()
                        {
                            let mut text = task.description.trim_end().to_owned();
                            if !text.is_empty() {
                                text.push_str("\n\n");
                            }
                            text.push_str(&format!("![[{}]]", d.key));
                            out.push(Intent::run(
                                "task.update",
                                &[("task", &detail.row.key), ("description", &text)],
                            ));
                            ui.close();
                        }
                    }
                });
            }
            ui.weak(t(lang, "ui-diagram-cite-hint"));
        });
}

/// Adjuntos (RF-MOD-17): guardar, quitar y añadir desde una ruta. Soltar
/// archivos sobre la ventana con una tarea abierta también los adjunta.
pub fn attachments(ui: &mut Ui, detail: &Detail, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let key = detail.row.key.clone();
    egui::CollapsingHeader::new(format!(
        "{} ({})",
        t(lang, "ui-attachments"),
        detail.attachments.len()
    ))
    .id_salt(("attachments", &key))
    .default_open(!detail.attachments.is_empty())
    .show(ui, |ui| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        for a in &detail.attachments {
            ui.horizontal(|ui| {
                icons::show(ui, Icon::Attachment, theme::GRAY);
                ui.label(&a.name);
                ui.weak(size_text(a.size));
                if ui.small_button(t(lang, "ui-save-as")).clicked() {
                    let path = format!("{home}/{}", a.name);
                    out.push(Intent::run(
                        "attachment.get",
                        &[("attachment", &a.id.to_string()), ("path", &path)],
                    ));
                }
                if icons::button_sized(ui, Icon::Trash, true, &t(lang, "ui-remove"), 14.0).clicked()
                {
                    out.push(Intent::run(
                        "attachment.remove",
                        &[("attachment", &a.id.to_string())],
                    ));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut st.edit.attach_path)
                    .hint_text(t(lang, "ui-attach-hint"))
                    .desired_width(220.0),
            );
            if ui
                .add_enabled(
                    !st.edit.attach_path.trim().is_empty(),
                    egui::Button::new(t(lang, "ui-attach")),
                )
                .clicked()
            {
                out.push(Intent::run(
                    "attachment.add",
                    &[("task", &key), ("path", st.edit.attach_path.trim())],
                ));
                st.edit.attach_path.clear();
            }
        });
        ui.weak(tr(
            lang,
            &Msg::new("ui-attach-drop").arg("home", home.as_str()),
        ));
    });
}

fn size_text(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

fn hours(minutes: i64) -> String {
    format!("{}:{:02}", minutes.max(0) / 60, minutes.max(0) % 60)
}

/// Tiempo de la tarea (RF-HIS-03/04): por actor, anotado y el cronómetro.
pub fn time(ui: &mut Ui, detail: &Detail, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let key = detail.row.key.clone();
    let tt = &detail.time;
    let total: i64 = tt.auto.iter().map(|(_, m)| m).sum::<i64>() + tt.manual;
    egui::CollapsingHeader::new(format!("{} ({})", t(lang, "ui-time"), hours(total)))
        .id_salt(("time", &key))
        .default_open(tt.running_here)
        .show(ui, |ui| {
            for (who, minutes) in &tt.auto {
                ui.horizontal(|ui| {
                    ui.label(who);
                    ui.weak(hours(*minutes));
                });
            }
            if tt.manual > 0 {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "ui-time-logged"));
                    ui.weak(hours(tt.manual));
                });
            }
            ui.horizontal(|ui| {
                if tt.running_here {
                    if icons::button(ui, Icon::Stop, true, &t(lang, "ui-timer-stop")).clicked() {
                        out.push(Intent::run("timer.stop", &[]));
                    }
                    ui.colored_label(theme::GREEN, t(lang, "ui-timer-running"));
                } else {
                    if icons::button(ui, Icon::Play, true, &t(lang, "ui-timer-start")).clicked() {
                        out.push(Intent::run("timer.start", &[("task", &key)]));
                    }
                    if let Some(other) = &tt.running_elsewhere {
                        ui.weak(tr(
                            lang,
                            &Msg::new("ui-timer-elsewhere").arg("task", other.as_str()),
                        ));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut st.edit.log_minutes)
                        .range(0..=1440)
                        .suffix(" min"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut st.edit.log_note)
                        .hint_text(t(lang, "param-note"))
                        .desired_width(140.0),
                );
                if ui
                    .add_enabled(
                        st.edit.log_minutes > 0,
                        egui::Button::new(t(lang, "ui-time-log")),
                    )
                    .clicked()
                {
                    out.push(Intent::run(
                        "time.log",
                        &[
                            ("task", &key),
                            ("minutes", &st.edit.log_minutes.to_string()),
                            ("note", st.edit.log_note.trim()),
                        ],
                    ));
                    st.edit.log_minutes = 0;
                    st.edit.log_note.clear();
                }
            });
        });
}

/// Coste que reportaron los agentes (RF-HIS-06), con el de las subtareas.
/// Solo aparece si hay algo reportado: lo escriben los agentes, no la app.
pub fn cost(ui: &mut Ui, detail: &Detail, lang: Lang) {
    let cost = &detail.cost;
    if cost.by_actor.is_empty() {
        return;
    }
    let amount = cost.total.amount_text();
    let tokens = tr(
        lang,
        &Msg::new("ui-cost-tokens").arg("tokens", tokens_text(cost.total.tokens)),
    );
    let title = if amount.is_empty() {
        format!("{} ({tokens})", t(lang, "ui-cost"))
    } else {
        format!("{} ({amount} · {tokens})", t(lang, "ui-cost"))
    };
    egui::CollapsingHeader::new(title)
        .id_salt(("cost", &detail.row.key))
        .show(ui, |ui| {
            for line in &cost.by_actor {
                ui.horizontal(|ui| {
                    ui.label(&line.who);
                    ui.weak(tr(
                        lang,
                        &Msg::new("ui-cost-tokens").arg("tokens", tokens_text(line.tokens)),
                    ));
                    ui.weak(line.amount_text());
                });
            }
            ui.weak(t(lang, "ui-cost-hint"));
        });
}

/// `1234567` → `1.2 M`, `12300` → `12.3 k`.
fn tokens_text(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1} k", n as f64 / 1_000.0),
        _ => format!("{:.1} M", n as f64 / 1_000_000.0),
    }
}

/// Ramas, commits y PRs vinculados (RF-INT-04, D16): se abren en el navegador.
pub fn git(ui: &mut Ui, detail: &Detail, lang: Lang) {
    if detail.git.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("{} ({})", t(lang, "ui-git"), detail.git.len()))
        .id_salt(("git", &detail.row.key))
        .show(ui, |ui| {
            for l in &detail.git {
                ui.horizontal(|ui| {
                    ui.weak(t(lang, l.kind.label_key()));
                    let reference = match l.kind {
                        model::GitKind::Commit => l.reference.get(..7).unwrap_or(&l.reference),
                        _ => l.reference.as_str(),
                    };
                    let text = RichText::new(reference).monospace();
                    match &l.url {
                        Some(url) => {
                            ui.hyperlink_to(text, url);
                        }
                        None => {
                            ui.label(text);
                        }
                    }
                    if !l.title.is_empty() {
                        ui.label(&l.title);
                    }
                });
            }
            ui.weak(t(lang, "ui-git-hint"));
        });
}

/// Regla de repetición (RF-MOD-18): su texto y un menú para cambiarla.
fn recurrence(ui: &mut Ui, task: &model::Task, key: &str, st: &mut UiState, out: &mut Vec<Intent>) {
    use model::RepeatUnit;

    let lang = st.lang;
    let current = task.recurrence.as_ref().map_or_else(
        || t(lang, "ui-recurrence-none"),
        |r| rules::recurrence::describe(r, lang),
    );
    let mut saved = false;
    ui.menu_button(current, |ui| {
        let edit = st
            .edit
            .recurrence
            .get_or_insert_with(|| crate::state::RecurrenceEdit::of(task.recurrence.as_ref()));
        ui.horizontal(|ui| {
            for (mode, label) in [
                (0, "ui-recurrence-none"),
                (1, "ui-recurrence-every"),
                (2, "ui-recurrence-after"),
            ] {
                ui.selectable_value(&mut edit.mode, mode, t(lang, label));
            }
        });
        match edit.mode {
            1 => {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut edit.interval).range(1..=366));
                    egui::ComboBox::from_id_salt(("rec-unit", key))
                        .selected_text(t(lang, edit.unit.label_key()))
                        .show_ui(ui, |ui| {
                            for u in [
                                RepeatUnit::Day,
                                RepeatUnit::Week,
                                RepeatUnit::Month,
                                RepeatUnit::Year,
                            ] {
                                ui.selectable_value(&mut edit.unit, u, t(lang, u.label_key()));
                            }
                        });
                });
                if edit.unit == RepeatUnit::Week {
                    ui.horizontal(|ui| {
                        for (i, on) in edit.weekdays.iter_mut().enumerate() {
                            let day = u8::try_from(i + 1).unwrap_or(1);
                            ui.toggle_value(on, t(lang, rules::recurrence::weekday_key(day)));
                        }
                    });
                }
                if edit.unit == RepeatUnit::Month {
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "ui-recurrence-month-day"));
                        ui.add(egui::DragValue::new(&mut edit.month_day).range(0..=31));
                    });
                    ui.weak(t(lang, "ui-recurrence-month-day-hint"));
                }
            }
            2 => {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut edit.after_days).range(1..=366));
                    ui.label(t(lang, "ui-recurrence-days-after"));
                });
            }
            _ => {}
        }
        ui.weak(t(lang, "ui-recurrence-hint"));
        if ui.button(t(lang, "ui-save")).clicked() {
            match edit.args() {
                Some(pairs) => {
                    let mut all: Vec<(&str, &str)> = vec![("task", key)];
                    all.extend(pairs.iter().map(|(k, v)| (*k, v.as_str())));
                    out.push(Intent::run("task.set_recurrence", &all));
                }
                None => out.push(Intent::run("task.clear_recurrence", &[("task", key)])),
            }
            saved = true;
            ui.close();
        }
    });
    if saved {
        st.edit.recurrence = None;
    }
}
