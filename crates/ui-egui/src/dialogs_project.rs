//! Diálogos de lo que define cada proyecto: su flujo con etapas (D10) y sus
//! dueños (D20), sus campos personalizados (RF-MOD-15) y las plantillas
//! (RF-MOD-16).

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, t};
use model::{FieldKind, Status, TemplateKind};

use crate::data::Snapshot;
use crate::state::{CycleForm, Intent, TemplateForm, UiState, WorkflowForm};
use crate::theme;
use crate::widgets::date_picker;
use crate::widgets::icons::{self, Icon};

/// Dueño de una columna: un agente activo o nadie. `Some` si cambió.
fn owner_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    current: Option<model::AgentId>,
    snap: &Snapshot,
    lang: Lang,
) -> Option<Option<model::AgentId>> {
    let name = |id: Option<model::AgentId>| {
        id.and_then(|id| snap.agents.iter().find(|r| r.agent.id == id))
            .map_or_else(|| t(lang, "ui-column-no-owner"), |r| r.agent.name.clone())
    };
    let mut chosen = current;
    egui::ComboBox::from_id_salt(id)
        .selected_text(name(current))
        .width(110.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, t(lang, "ui-column-no-owner"));
            for r in snap
                .agents
                .iter()
                .filter(|r| r.agent.status == model::AgentStatus::Active)
            {
                ui.selectable_value(&mut chosen, Some(r.agent.id), &r.agent.name);
            }
        })
        .response
        .on_hover_text(t(lang, "ui-column-owner-hint"));
    (chosen != current).then_some(chosen)
}

pub fn workflow(
    ui: &mut Ui,
    snap: &Snapshot,
    lang: Lang,
    f: &mut WorkflowForm,
    out: &mut Vec<Intent>,
) {
    let Some(project) = snap.project(f.project) else {
        return;
    };
    let key = project.key.clone();
    ui.heading(t(lang, "ui-workflow"));
    ui.weak(t(lang, "ui-workflow-hint"));
    let flow = rules::workflow(project);
    let last = flow.len().saturating_sub(1);
    egui::Grid::new("workflow")
        .num_columns(5)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for (i, d) in flow.iter().enumerate() {
                crate::widgets::icons::dot(ui, theme::status(d.base));
                let shown = crate::flow::def_name(d, lang);
                let text = f
                    .renames
                    .entry(d.key.clone())
                    .or_insert_with(|| shown.clone());
                // En una rejilla, `desired_width` no basta: la columna nace estrecha.
                let edit = ui.add(
                    egui::TextEdit::singleline(text)
                        .desired_width(160.0)
                        .min_size(egui::vec2(140.0, 0.0)),
                );
                if edit.lost_focus() && text.trim() != shown && !text.trim().is_empty() {
                    out.push(Intent::run(
                        "status.update",
                        &[("project", &key), ("status", &d.key), ("name", text.trim())],
                    ));
                }
                ui.weak(if d.is_base() {
                    t(lang, "ui-base-status")
                } else {
                    format!("› {}", t(lang, d.base.label_key()))
                });
                // Límite WIP (RF-COO-08): 0 = sin límite. Dueño de la columna (D20).
                ui.horizontal(|ui| {
                    if let Some(owner) = owner_combo(ui, ("owner", &d.key), d.owner, snap, lang) {
                        let name = owner
                            .and_then(|id| snap.agents.iter().find(|r| r.agent.id == id))
                            .map(|r| r.agent.name.clone())
                            .unwrap_or_else(|| "none".to_owned());
                        out.push(Intent::run(
                            "status.update",
                            &[("project", &key), ("status", &d.key), ("owner", &name)],
                        ));
                    }
                    let mut limit = d.wip_limit.unwrap_or(0);
                    let drag = ui
                        .add(
                            egui::DragValue::new(&mut limit)
                                .range(0..=99)
                                .prefix("WIP "),
                        )
                        .on_hover_text(t(lang, "param-wip"));
                    if (drag.drag_stopped() || drag.lost_focus())
                        && Some(limit).filter(|l| *l > 0) != d.wip_limit
                    {
                        out.push(Intent::run(
                            "status.update",
                            &[
                                ("project", &key),
                                ("status", &d.key),
                                ("wip", &limit.to_string()),
                            ],
                        ));
                    }
                    let mut reject = d.wip_reject;
                    if d.wip_limit.is_some()
                        && ui.checkbox(&mut reject, t(lang, "ui-wip-reject")).changed()
                    {
                        out.push(Intent::run(
                            "status.update",
                            &[
                                ("project", &key),
                                ("status", &d.key),
                                ("wip_reject", if reject { "true" } else { "false" }),
                            ],
                        ));
                    }
                });
                ui.horizontal(|ui| {
                    let pos = |p: usize| (p + 1).to_string();
                    if i > 0
                        && icons::button_sized(
                            ui,
                            Icon::ChevronUp,
                            true,
                            &t(lang, "ui-move-up"),
                            16.0,
                        )
                        .clicked()
                    {
                        out.push(Intent::run(
                            "status.update",
                            &[
                                ("project", &key),
                                ("status", &d.key),
                                ("position", &pos(i - 1)),
                            ],
                        ));
                    }
                    if i < last
                        && icons::button_sized(
                            ui,
                            Icon::ChevronDown,
                            true,
                            &t(lang, "ui-move-down"),
                            16.0,
                        )
                        .clicked()
                    {
                        out.push(Intent::run(
                            "status.update",
                            &[
                                ("project", &key),
                                ("status", &d.key),
                                ("position", &pos(i + 1)),
                            ],
                        ));
                    }
                    if !d.is_base()
                        && icons::button_sized(ui, Icon::Trash, true, &t(lang, "ui-delete"), 14.0)
                            .clicked()
                    {
                        out.push(Intent::run(
                            "status.delete",
                            &[("project", &key), ("status", &d.key)],
                        ));
                    }
                });
                ui.end_row();
            }
        });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.new_stage)
                .hint_text(t(lang, "ui-new-stage-hint"))
                .desired_width(160.0),
        );
        egui::ComboBox::from_id_salt("stage_base")
            .selected_text(t(lang, f.new_base.label_key()))
            .show_ui(ui, |ui| {
                for s in Status::ALL {
                    ui.selectable_value(&mut f.new_base, *s, t(lang, s.label_key()));
                }
            });
        if let Some(owner) = owner_combo(ui, "new_stage_owner", f.new_owner, snap, lang) {
            f.new_owner = owner;
        }
        if ui
            .add_enabled(
                !f.new_stage.trim().is_empty(),
                egui::Button::new(t(lang, "ui-add")),
            )
            .clicked()
        {
            let owner = f
                .new_owner
                .and_then(|id| snap.agents.iter().find(|r| r.agent.id == id))
                .map(|r| r.agent.name.clone())
                .unwrap_or_default();
            out.push(Intent::run(
                "status.create",
                &[
                    ("project", &key),
                    ("name", f.new_stage.trim()),
                    ("base", f.new_base.as_str()),
                    ("owner", &owner),
                ],
            ));
            f.new_stage.clear();
        }
    });

    ui.separator();
    ui.heading(t(lang, "ui-fields"));
    for field in &project.fields {
        ui.horizontal(|ui| {
            ui.strong(&field.name);
            ui.weak(t(lang, field.kind.label_key()));
            if !field.options.is_empty() {
                ui.weak(field.options.join(", "));
            }
            if icons::button_sized(ui, Icon::Trash, true, &t(lang, "ui-delete"), 14.0).clicked() {
                out.push(Intent::run(
                    "field.delete",
                    &[("project", &key), ("field", &field.key)],
                ));
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.new_field)
                .hint_text(t(lang, "ui-new-field-hint"))
                .desired_width(140.0),
        );
        egui::ComboBox::from_id_salt("field_kind")
            .selected_text(t(lang, f.new_kind.label_key()))
            .show_ui(ui, |ui| {
                for k in FieldKind::ALL {
                    ui.selectable_value(&mut f.new_kind, *k, t(lang, k.label_key()));
                }
            });
        let needs_options = matches!(f.new_kind, FieldKind::Select | FieldKind::Multiselect);
        if needs_options {
            ui.add(
                egui::TextEdit::singleline(&mut f.new_options)
                    .hint_text(t(lang, "param-options"))
                    .desired_width(160.0),
            );
        }
        let ready =
            !f.new_field.trim().is_empty() && (!needs_options || !f.new_options.trim().is_empty());
        if ui
            .add_enabled(ready, egui::Button::new(t(lang, "ui-add")))
            .clicked()
        {
            let mut pairs = vec![
                ("project", key.clone()),
                ("name", f.new_field.trim().to_owned()),
                ("kind", f.new_kind.as_str().to_owned()),
            ];
            if needs_options {
                pairs.push(("options", f.new_options.trim().to_owned()));
            }
            out.push(Intent::Run {
                id: "field.create",
                args: pairs
                    .iter()
                    .fold(engine::Args::none(), |a, (k, v)| a.with(*k, v.as_str())),
            });
            f.new_field.clear();
            f.new_options.clear();
        }
    });
}

pub fn templates(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &UiState,
    f: &mut TemplateForm,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    if snap.templates.is_empty() {
        ui.weak(t(lang, "ui-no-templates"));
    }
    for tpl in &snap.templates {
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut f.template,
                Some(tpl.name.clone()),
                RichText::new(&tpl.name).strong(),
            );
            ui.weak(t(lang, tpl.kind.label_key()));
            if !tpl.description.is_empty() {
                ui.weak(&tpl.description);
            }
            if icons::button_sized(ui, Icon::Trash, true, &t(lang, "ui-delete"), 14.0).clicked() {
                out.push(Intent::run("template.delete", &[("template", &tpl.name)]));
            }
        });
    }
    let chosen = f
        .template
        .as_ref()
        .and_then(|n| snap.templates.iter().find(|t| &t.name == n));
    if let Some(tpl) = chosen {
        ui.separator();
        match tpl.kind {
            TemplateKind::Task => {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "ui-col-project"));
                    ui.add(egui::TextEdit::singleline(&mut f.project_key).desired_width(80.0));
                    ui.label(t(lang, "ui-parent"));
                    ui.add(
                        egui::TextEdit::singleline(&mut f.parent)
                            .hint_text("API-1")
                            .desired_width(80.0),
                    );
                });
            }
            TemplateKind::Project => {
                ui.horizontal(|ui| {
                    ui.label(t(lang, "ui-key-hint"));
                    ui.add(egui::TextEdit::singleline(&mut f.new_key).desired_width(80.0));
                });
            }
        }
        if ui.button(t(lang, "ui-template-apply")).clicked() {
            let mut pairs = vec![("template", tpl.name.clone())];
            match tpl.kind {
                TemplateKind::Task if !f.parent.trim().is_empty() => {
                    pairs.push(("parent", f.parent.trim().to_owned()));
                }
                TemplateKind::Task => pairs.push(("project", f.project_key.trim().to_owned())),
                TemplateKind::Project => pairs.push(("key", f.new_key.trim().to_owned())),
            }
            out.push(Intent::Run {
                id: "template.instantiate",
                args: pairs
                    .iter()
                    .fold(engine::Args::none(), |a, (k, v)| a.with(*k, v.as_str())),
            });
        }
    }
    if let Some(task) = st
        .selected
        .and_then(|id| snap.rows.iter().find(|r| r.id() == id))
    {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(i18n::tr(
                lang,
                &i18n::Msg::new("ui-template-save").arg("task", task.key.as_str()),
            ));
            ui.add(egui::TextEdit::singleline(&mut f.save_name).desired_width(140.0));
            if ui
                .add_enabled(
                    !f.save_name.trim().is_empty(),
                    egui::Button::new(t(lang, "ui-save")),
                )
                .clicked()
            {
                out.push(Intent::run(
                    "template.from_task",
                    &[("task", &task.key), ("name", f.save_name.trim())],
                ));
                f.save_name.clear();
            }
        });
    }
}

/// Resumen desde la última visita (RF-SUP-09): cifras y claves clicables.
pub fn summary(
    ui: &mut Ui,
    _snap: &Snapshot,
    lang: Lang,
    v: &serde_json::Value,
    out: &mut Vec<Intent>,
) {
    let since = v["since"]
        .as_str()
        .and_then(|s| s.parse::<model::Timestamp>().ok())
        .map(|ts| crate::views::common::ago(lang, ts))
        .unwrap_or_default();
    ui.label(i18n::tr(
        lang,
        &i18n::Msg::new("ui-summary-since").arg("ago", since.as_str()),
    ));
    ui.separator();
    for (key, title) in [
        ("done", "ui-summary-done"),
        ("in_review", "ui-summary-review"),
        ("escalated", "ui-summary-escalated"),
    ] {
        let keys: Vec<&str> = v[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{} ({})", t(lang, title), keys.len()));
            for k in keys {
                if ui.link(k).clicked() {
                    out.push(Intent::OpenKey(k.to_owned()));
                }
            }
        });
    }
    ui.label(i18n::tr(
        lang,
        &i18n::Msg::new("ui-summary-counts")
            .arg("created", v["created"].as_u64().unwrap_or(0))
            .arg("comments", v["comments"].as_u64().unwrap_or(0))
            .arg("findings", v["findings"].as_u64().unwrap_or(0))
            .arg("expired", v["expired_leases"].as_u64().unwrap_or(0)),
    ));
    let agents: Vec<&str> = v["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect();
    if !agents.is_empty() {
        ui.weak(format!(
            "{}: {}",
            t(lang, "ui-view-agents"),
            agents.join(", ")
        ));
    }
}

/// ¿Hay algo que contar en el resumen?
pub fn summary_has_news(v: &serde_json::Value) -> bool {
    ["done", "in_review", "escalated"]
        .iter()
        .any(|k| v[*k].as_array().is_some_and(|a| !a.is_empty()))
        || ["created", "comments", "findings", "expired_leases"]
            .iter()
            .any(|k| v[*k].as_u64().unwrap_or(0) > 0)
}

/// Alta o edición de un ciclo o sprint (RF-INT-05). Devuelve `true` al guardar.
pub fn cycle(
    ui: &mut Ui,
    snap: &Snapshot,
    lang: Lang,
    f: &mut CycleForm,
    out: &mut Vec<Intent>,
) -> bool {
    let Some(key) = snap.project(f.project).map(|p| p.key.clone()) else {
        return true;
    };
    egui::Grid::new("cycle")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(t(lang, "ui-project"));
            ui.label(RichText::new(&key).strong());
            ui.end_row();
            ui.label(t(lang, "ui-cycle-name"));
            ui.add(egui::TextEdit::singleline(&mut f.name).min_size(egui::vec2(160.0, 0.0)));
            ui.end_row();
            ui.label(t(lang, "ui-cycle-start"));
            if let Some(Some(d)) =
                date_picker::date_button(ui, ("cycle_date", 0), Some(f.start), lang)
            {
                f.start = d;
                f.end = f.end.max(d);
            }
            ui.end_row();
            ui.label(t(lang, "ui-cycle-end"));
            if let Some(Some(d)) =
                date_picker::date_button(ui, ("cycle_date", 1), Some(f.end), lang)
            {
                f.end = d.max(f.start);
            }
            ui.end_row();
        });
    let days = i64::from((f.end - f.start).get_days()) + 1;
    ui.weak(i18n::tr(
        lang,
        &i18n::Msg::new("ui-cycle-days").arg("days", days),
    ));
    let name = f.name.trim();
    let (start, end) = (f.start.to_string(), f.end.to_string());
    let save = ui
        .add_enabled(!name.is_empty(), egui::Button::new(t(lang, "ui-save")))
        .clicked();
    if save {
        match f.id {
            None => out.push(Intent::run(
                "cycle.create",
                &[
                    ("project", &key),
                    ("name", name),
                    ("start", &start),
                    ("end", &end),
                ],
            )),
            Some(id) => out.push(Intent::run(
                "cycle.update",
                &[
                    ("cycle", &id.to_string()),
                    ("name", name),
                    ("start", &start),
                    ("end", &end),
                ],
            )),
        }
    }
    save
}
