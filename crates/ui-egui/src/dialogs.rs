//! Ventanas de edición: configuración del workspace, proyecto, etiquetas,
//! agente, atajos de teclado y confirmaciones de acciones irreversibles.

use eframe::egui::{self, RichText};
use i18n::t;

use crate::data::Snapshot;
use crate::state::{Dialog, Intent, UiState};
use crate::theme;

/// Separación desde arriba y tamaño máximo de una ventana para que quepa
/// entera en la de la app, también en la mínima: si no, se desplaza dentro.
pub fn fit_window(ctx: &egui::Context, top: f32) -> (f32, egui::Vec2) {
    let screen = ctx.content_rect();
    let top = top.min(screen.height() * 0.08);
    let max = egui::vec2(
        (screen.width() - 32.0).max(200.0),
        (screen.height() - top - 72.0).max(160.0),
    );
    (top, max)
}

pub fn show(ctx: &egui::Context, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let Some(mut dialog) = st.dialog.take() else {
        st.dialog_error = None;
        return;
    };
    let mut open = true;
    let mut close = false;
    let title = t(
        lang,
        match &dialog {
            Dialog::Settings(_) => "ui-settings",
            Dialog::Project(f) if f.editing.is_none() => "ui-new-project",
            Dialog::Project(_) => "ui-edit-project",
            Dialog::Labels(_) => "ui-labels",
            Dialog::Agent(f) if f.editing.is_none() => "ui-agent-add",
            Dialog::Agent(_) => "ui-agent-edit",
            Dialog::Interface(f) if f.editing.is_none() => "ui-interface-register",
            Dialog::Interface(_) => "ui-interface-edit",
            Dialog::Shortcuts => "ui-shortcuts",
            Dialog::SaveView(_) => "ui-view-save",
            Dialog::Workspaces(_) => "ui-workspaces",
            Dialog::Token { .. } => "ui-interface-token",
            Dialog::Summary(_) => "ui-summary",
            Dialog::Export(_) => "ui-export",
            Dialog::Workflow(_) => "ui-workflow-fields",
            Dialog::Templates(_) => "ui-templates",
            Dialog::Import(_) => "ui-import",
            Dialog::Cycle(f) if f.id.is_none() => "ui-cycle-new",
            Dialog::Cycle(_) => "ui-cycle-edit",
            Dialog::ConfirmRevoke(_)
            | Dialog::ConfirmInterfaceRevoke(_)
            | Dialog::ConfirmNewToken(_)
            | Dialog::ConfirmCycleDelete(..) => "ui-confirm",
            Dialog::DeleteData(_) => "ui-delete-data",
        },
    );
    let (top, max) = fit_window(ctx, 80.0);
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(380.0_f32.min(max.x))
        .max_size(max)
        .vscroll(true)
        .anchor(egui::Align2::CENTER_TOP, [0.0, top])
        .show(ctx, |ui| {
            // Con desplazamiento, el contenido no ensancha la ventana por sí solo.
            ui.set_min_width(360.0_f32.min(max.x - 24.0));
            match &mut dialog {
                Dialog::Settings(f) => {
                    egui::Grid::new("settings")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            for (key, value, min, max) in [
                                ("param-max_depth", &mut f.max_depth, 1, 20),
                                ("param-trash_days", &mut f.trash_days, 1, 3650),
                                ("param-lease_minutes", &mut f.lease_minutes, 1, 1440),
                                ("param-agent_wip_limit", &mut f.agent_wip_limit, 1, 50),
                                ("param-auto_archive_days", &mut f.auto_archive_days, 1, 3650),
                                // RF-ALM-06: 0 horas = sin copias automáticas.
                                ("param-backup_hours", &mut f.backup_hours, 0, 720),
                                ("param-backup_keep", &mut f.backup_keep, 1, 365),
                            ] {
                                ui.label(t(lang, key));
                                ui.add(egui::DragValue::new(value).range(min..=max));
                                ui.end_row();
                            }
                        });
                    if ui.button(t(lang, "ui-save")).clicked() {
                        out.push(Intent::run(
                            "workspace.configure",
                            &[
                                ("max_depth", &f.max_depth.to_string()),
                                ("trash_days", &f.trash_days.to_string()),
                                ("lease_minutes", &f.lease_minutes.to_string()),
                                ("agent_wip_limit", &f.agent_wip_limit.to_string()),
                                ("auto_archive_days", &f.auto_archive_days.to_string()),
                                ("backup_hours", &f.backup_hours.to_string()),
                                ("backup_keep", &f.backup_keep.to_string()),
                            ],
                        ));
                        close = true;
                    }
                }
                Dialog::Project(f) => {
                    egui::Grid::new("project")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            if f.editing.is_none() {
                                ui.label(t(lang, "ui-key-hint"));
                                ui.add(egui::TextEdit::singleline(&mut f.key).hint_text("WEB"));
                                ui.end_row();
                            }
                            ui.label(t(lang, "ui-name-hint"));
                            ui.text_edit_singleline(&mut f.name);
                            ui.end_row();
                        });
                    if f.editing.is_none() {
                        // Fuera de la rejilla, para que se parta en líneas.
                        ui.add(
                            egui::Label::new(RichText::new(t(lang, "ui-key-explain")).weak())
                                .wrap(),
                        );
                    }
                    ui.label(t(lang, "ui-description"));
                    ui.add(
                        egui::TextEdit::multiline(&mut f.description)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );
                    let unit = f.estimate_unit.unwrap_or(model::EstimateUnit::Hours);
                    let mut chosen = unit;
                    ui.horizontal(|ui| {
                        ui.label(t(lang, "ui-estimate-unit"));
                        egui::ComboBox::from_id_salt("project_unit")
                            .selected_text(t(lang, unit.label_key()))
                            .show_ui(ui, |ui| {
                                for u in model::EstimateUnit::ALL {
                                    ui.selectable_value(&mut chosen, *u, t(lang, u.label_key()));
                                }
                            });
                    });
                    f.estimate_unit = Some(chosen);
                    if ui.button(t(lang, "ui-save")).clicked() {
                        match f.editing {
                            None => out.push(Intent::run(
                                "project.create",
                                &[
                                    ("key", f.key.trim()),
                                    ("name", f.name.trim()),
                                    ("description", &f.description),
                                    ("estimate_unit", chosen.as_str()),
                                ],
                            )),
                            Some(id) => {
                                if let Some(p) = snap.project(id) {
                                    out.push(Intent::run(
                                        "project.update",
                                        &[
                                            ("project", &p.key),
                                            ("name", f.name.trim()),
                                            ("description", &f.description),
                                            ("estimate_unit", chosen.as_str()),
                                        ],
                                    ));
                                }
                            }
                        }
                        close = true;
                    }
                }
                Dialog::Labels(f) => {
                    egui::Grid::new("labels")
                        .num_columns(3)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            for (original, name, color) in &mut f.rows {
                                ui.color_edit_button_srgb(color);
                                ui.text_edit_singleline(name);
                                if ui.button(t(lang, "ui-save")).clicked() {
                                    out.push(Intent::run(
                                        "label.update",
                                        &[
                                            ("label", original),
                                            ("name", name.trim()),
                                            ("color", &theme::to_hex(*color)),
                                        ],
                                    ));
                                    *original = name.trim().to_owned();
                                }
                                ui.end_row();
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut f.new_color);
                        ui.add(
                            egui::TextEdit::singleline(&mut f.new_name)
                                .hint_text(t(lang, "ui-new-label-hint")),
                        );
                        if ui
                            .add_enabled(
                                !f.new_name.trim().is_empty(),
                                egui::Button::new(t(lang, "ui-add")),
                            )
                            .clicked()
                        {
                            out.push(Intent::run(
                                "label.create",
                                &[
                                    ("name", f.new_name.trim()),
                                    ("color", &theme::to_hex(f.new_color)),
                                ],
                            ));
                            close = true;
                        }
                    });
                }
                Dialog::Agent(f) => agent(ui, snap, lang, f, out),
                Dialog::Interface(f) => interface(ui, snap, lang, f, out),
                Dialog::Shortcuts => shortcuts(ui, st),
                Dialog::Workspaces(f) => {
                    for name in &f.names {
                        let current = *name == f.current;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!current, egui::Button::selectable(current, name))
                                .clicked()
                            {
                                out.push(Intent::OpenWorkspace(name.clone()));
                            }
                        });
                    }
                    ui.separator();
                    ui.label(t(lang, "ui-workspace-new"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut f.new_name).hint_text("cliente-acme"),
                        );
                        let name = f.new_name.trim().to_lowercase();
                        let valid = !name.is_empty()
                            && name.chars().all(|c| {
                                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                            });
                        if ui
                            .add_enabled(valid, egui::Button::new(t(lang, "ui-workspace-open")))
                            .clicked()
                        {
                            out.push(Intent::OpenWorkspace(name));
                        }
                    });
                    ui.weak(t(lang, "ui-workspace-hint"));
                }
                Dialog::SaveView(name) => {
                    ui.label(t(lang, "ui-view-name"));
                    let edit =
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    if name.is_empty() {
                        edit.request_focus();
                    }
                    ui.label(RichText::new(&st.filter.text).monospace().weak());
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button(t(lang, "ui-save")).clicked() || enter) && !name.trim().is_empty()
                    {
                        out.push(save_view_intent(snap, st, name.trim()));
                        st.active_view = Some(name.trim().to_owned());
                        close = true;
                    }
                }
                Dialog::Token { interface, token } => {
                    ui.label(i18n::tr(
                        lang,
                        &i18n::Msg::new("ui-interface-token-of")
                            .arg("interface", interface.as_str()),
                    ));
                    ui.add(
                        egui::TextEdit::singleline(&mut token.clone()).desired_width(f32::INFINITY),
                    );
                    if ui.button(t(lang, "ui-copy")).clicked() {
                        ui.ctx().copy_text(token.clone());
                    }
                    ui.weak(t(lang, "ui-agent-token-hint"));
                }
                Dialog::Summary(v) => crate::dialogs_project::summary(ui, snap, lang, v, out),
                Dialog::Export(f) => export(ui, snap, st, f, out),
                Dialog::Workflow(f) => crate::dialogs_project::workflow(ui, snap, lang, f, out),
                Dialog::Templates(f) => crate::dialogs_project::templates(ui, snap, st, f, out),
                Dialog::Import(f) => import(ui, lang, f, out),
                Dialog::ConfirmRevoke(name) => {
                    ui.label(i18n::tr(
                        lang,
                        &i18n::Msg::new("ui-confirm-revoke").arg("agent", name.as_str()),
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new(t(lang, "ui-revoke")).color(theme::RED))
                            .clicked()
                        {
                            out.push(Intent::run("agent.revoke", &[("agent", name)]));
                            close = true;
                        }
                        if ui.button(t(lang, "ui-cancel")).clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::ConfirmInterfaceRevoke(name) => {
                    ui.label(i18n::tr(
                        lang,
                        &i18n::Msg::new("ui-confirm-interface-revoke")
                            .arg("interface", name.as_str()),
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new(t(lang, "ui-revoke")).color(theme::RED))
                            .clicked()
                        {
                            out.push(Intent::run("interface.revoke", &[("interface", name)]));
                            close = true;
                        }
                        if ui.button(t(lang, "ui-cancel")).clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::ConfirmNewToken(name) => {
                    ui.label(i18n::tr(
                        lang,
                        &i18n::Msg::new("ui-confirm-new-token").arg("interface", name.as_str()),
                    ));
                    ui.horizontal(|ui| {
                        if ui.button(t(lang, "ui-new-token")).clicked() {
                            // Si sale bien, este diálogo pasa a ser el del token.
                            out.push(Intent::IssueToken {
                                id: "interface.rotate_token",
                                args: engine::Args::none().with("interface", name.as_str()),
                            });
                        }
                        if ui.button(t(lang, "ui-cancel")).clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Cycle(f) => {
                    close = crate::dialogs_project::cycle(ui, snap, lang, f, out);
                }
                Dialog::ConfirmCycleDelete(id, name) => {
                    ui.label(i18n::tr(
                        lang,
                        &i18n::Msg::new("ui-cycle-delete-confirm").arg("cycle", name.as_str()),
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new(t(lang, "ui-cycle-delete")).color(theme::RED))
                            .clicked()
                        {
                            out.push(Intent::run("cycle.delete", &[("cycle", &id.to_string())]));
                            if st.filter.cycle == Some(*id) {
                                st.filter.cycle = None;
                            }
                            close = true;
                        }
                        if ui.button(t(lang, "ui-cancel")).clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::DeleteData(f) => delete_data(ui, snap, lang, f, out),
            }
            if let Some(e) = &st.dialog_error {
                ui.add_space(4.0);
                ui.colored_label(theme::RED, e);
            }
        });
    if open && !close {
        st.dialog = Some(dialog);
    } else {
        st.dialog_error = None;
    }
}

/// Casillas con los proyectos activos; vacío = todos.
fn scope_picker(ui: &mut egui::Ui, snap: &Snapshot, lang: i18n::Lang, scope: &mut Vec<String>) {
    ui.vertical(|ui| {
        ui.horizontal_wrapped(|ui| {
            for p in snap
                .projects
                .iter()
                .filter(|p| p.status == model::ProjectStatus::Active)
            {
                let mut on = scope.contains(&p.key);
                if ui.checkbox(&mut on, &p.key).changed() {
                    if on {
                        scope.push(p.key.clone());
                    } else {
                        scope.retain(|k| k != &p.key);
                    }
                }
            }
        });
        if scope.is_empty() {
            ui.weak(t(lang, "ui-agent-scope-all"));
        }
    });
}

fn level_picker(ui: &mut egui::Ui, lang: i18n::Lang, id: &str, level: &mut model::AgentLevel) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(t(lang, level.label_key()))
        .show_ui(ui, |ui| {
            for l in model::AgentLevel::ALL {
                ui.selectable_value(level, *l, t(lang, l.label_key()));
            }
        });
}

/// Un agente en su interfaz (D15, D22): sin token propio. Al editarlo se
/// puede mover a otra interfaz.
fn agent(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    lang: i18n::Lang,
    f: &mut crate::state::AgentForm,
    out: &mut Vec<Intent>,
) {
    let name_ok = f.editing.is_some() || rules::agent_name(&f.name).is_ok();
    egui::Grid::new("agent")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(t(lang, "ui-agent-interface"));
            if f.editing.is_none() {
                ui.strong(&f.interface);
            } else {
                let current = if f.interface.is_empty() {
                    t(lang, "ui-no-interface")
                } else {
                    f.interface.clone()
                };
                egui::ComboBox::from_id_salt("agent_interface")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for i in snap
                            .interfaces
                            .iter()
                            .filter(|i| i.status == model::AgentStatus::Active)
                        {
                            ui.selectable_value(&mut f.interface, i.name.clone(), &i.name);
                        }
                    });
            }
            ui.end_row();
            if f.editing.is_none() {
                ui.label(t(lang, "ui-name-hint"));
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut f.name).hint_text("cc-dev"));
                    if !f.name.trim().is_empty() && !name_ok {
                        ui.colored_label(theme::ORANGE, t(lang, "ui-agent-name-rule"));
                    }
                });
                ui.end_row();
            }
            ui.label(t(lang, "ui-agent-model"));
            ui.add(egui::TextEdit::singleline(&mut f.model).hint_text("claude-opus-5-5"));
            ui.end_row();
            // Su función: lo único que distingue a un agente de otro (D20).
            ui.label(t(lang, "ui-agent-function"));
            ui.add(
                egui::TextEdit::multiline(&mut f.description)
                    .hint_text(t(lang, "ui-agent-function-hint"))
                    .desired_rows(2),
            );
            ui.end_row();
            ui.label(t(lang, "param-color"));
            ui.color_edit_button_srgb(&mut f.color);
            ui.end_row();
            ui.label(t(lang, "ui-agent-level"));
            let mut level = f.level.unwrap_or(model::AgentLevel::Plan);
            level_picker(ui, lang, "agent_level", &mut level);
            f.level = Some(level);
            ui.end_row();
            ui.label(t(lang, "ui-agent-scope"));
            scope_picker(ui, snap, lang, &mut f.scope);
            ui.end_row();
            ui.label(t(lang, "ui-agent-capabilities"));
            ui.add(
                egui::TextEdit::singleline(&mut f.capabilities).hint_text("rust, frontend, docs"),
            );
            ui.end_row();
        });
    ui.weak(t(lang, "ui-agent-no-token-hint"));
    let ready = name_ok && !f.name.trim().is_empty() && !f.interface.is_empty();
    if ui
        .add_enabled(ready, egui::Button::new(t(lang, "ui-save")))
        .clicked()
    {
        let (id, args) = f.command();
        out.push(Intent::Submit { id, args });
    }
}

/// Alta o edición de una interfaz, la que tiene el token (D15, D22).
fn interface(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    lang: i18n::Lang,
    f: &mut crate::state::InterfaceForm,
    out: &mut Vec<Intent>,
) {
    let name_ok = rules::interface_name(&f.name).is_ok();
    egui::Grid::new("interface")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(t(lang, "ui-interface-name"));
            ui.vertical(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.name).hint_text("Claude Code"));
                if !f.name.trim().is_empty() && !name_ok {
                    ui.colored_label(theme::ORANGE, t(lang, "ui-interface-name-rule"));
                }
            });
            ui.end_row();
            ui.label(t(lang, "ui-interface-kind"));
            ui.add(
                egui::TextEdit::singleline(&mut f.kind)
                    .hint_text("claude-code, cursor, antigravity"),
            );
            ui.end_row();
            ui.label(t(lang, "ui-description"));
            ui.text_edit_singleline(&mut f.description);
            ui.end_row();
            ui.label(t(lang, "param-color"));
            ui.color_edit_button_srgb(&mut f.color);
            ui.end_row();
            ui.label(t(lang, "ui-interface-level"));
            level_picker(ui, lang, "interface_level", &mut f.level);
            ui.end_row();
            ui.label(t(lang, "ui-agent-scope"));
            scope_picker(ui, snap, lang, &mut f.scope);
            ui.end_row();
            ui.label(t(lang, "param-max_agents"));
            ui.add(egui::DragValue::new(&mut f.max_agents).range(1..=1000));
            ui.end_row();
        });
    ui.weak(t(lang, "ui-interface-hint"));
    if ui
        .add_enabled(name_ok, egui::Button::new(t(lang, "ui-save")))
        .clicked()
    {
        out.push(f.intent());
    }
}

/// Exportar a `.taskcraft`: todo el workspace o el proyecto actual (RF-EXP-09).
fn export(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    st: &UiState,
    f: &mut crate::state::ExchangeForm,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    ui.label(t(lang, "param-path"));
    ui.add(egui::TextEdit::singleline(&mut f.path).desired_width(f32::INFINITY));
    let current = match st.scope {
        crate::data::Scope::Project(id) => snap.project(id).map(|p| p.key.clone()),
        crate::data::Scope::All => None,
    };
    if let Some(key) = &current {
        ui.checkbox(
            &mut f.only_current,
            i18n::tr(
                lang,
                &i18n::Msg::new("ui-export-only").arg("project", key.as_str()),
            ),
        );
    }
    if ui.button(t(lang, "ui-export")).clicked() && !f.path.trim().is_empty() {
        let mut args = engine::Args::none().with("path", f.path.trim());
        if let (true, Some(key)) = (f.only_current, &current) {
            args = args.with("projects", key.as_str());
        }
        out.push(Intent::Exchange {
            id: "workspace.export",
            args,
        });
    }
    match &f.result {
        Some(Ok(v)) => {
            ui.label(i18n::tr(
                lang,
                &i18n::Msg::new("ui-export-done")
                    .arg("tasks", v["tasks"].as_u64().unwrap_or(0))
                    .arg("path", v["path"].as_str().unwrap_or_default()),
            ));
        }
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        None => {}
    }
}

/// Importar con vista previa: qué se crea, qué colisiona y qué falla (RF-EXP-05).
fn import(
    ui: &mut egui::Ui,
    lang: i18n::Lang,
    f: &mut crate::state::ExchangeForm,
    out: &mut Vec<Intent>,
) {
    ui.label(t(lang, "param-path"));
    let path_changed = ui
        .add(egui::TextEdit::singleline(&mut f.path).desired_width(f32::INFINITY))
        .changed();
    let mode_changed = ui
        .horizontal_wrapped(|ui| {
            let mut changed = false;
            for (mode, key) in [
                ("workspace", "ui-import-workspace"),
                ("project", "ui-import-projects"),
                ("merge", "ui-import-merge"),
            ] {
                changed |= ui.radio_value(&mut f.mode, mode, t(lang, key)).changed();
            }
            changed
        })
        .inner;
    if path_changed || mode_changed {
        f.result = None;
        f.imported = false;
    }
    let args = || {
        engine::Args::none()
            .with("path", f.path.trim())
            .with("mode", f.mode)
    };
    let can_import = matches!(&f.result, Some(Ok(v)) if v["can_import"] == true) && !f.imported;
    ui.horizontal(|ui| {
        if ui.button(t(lang, "ui-import-preview")).clicked() {
            out.push(Intent::Exchange {
                id: "workspace.import_preview",
                args: args(),
            });
        }
        if ui
            .add_enabled(can_import, egui::Button::new(t(lang, "ui-import")))
            .clicked()
        {
            out.push(Intent::Exchange {
                id: "workspace.import",
                args: args(),
            });
        }
    });
    ui.separator();
    match &f.result {
        Some(Ok(v)) if f.imported => {
            ui.label(t(lang, "ui-import-done"));
            counts(ui, lang, &v["creates"]);
            updates(ui, lang, &v["updates"]);
        }
        Some(Ok(v)) => {
            counts(ui, lang, &v["creates"]);
            updates(ui, lang, &v["updates"]);
            for c in v["collisions"].as_array().into_iter().flatten() {
                ui.label(format!(
                    "• {} `{}` › {}",
                    c["kind"].as_str().unwrap_or_default(),
                    c["name"].as_str().unwrap_or_default(),
                    c["resolution"].as_str().unwrap_or_default()
                ));
            }
            for e in v["errors"].as_array().into_iter().flatten() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    e["detail"].as_str().unwrap_or_default(),
                );
            }
        }
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        None => {
            ui.weak(t(lang, "ui-import-hint"));
        }
    }
}

/// En fusión: lo que ya existía y se actualiza (RF-EXP-03b).
fn updates(ui: &mut egui::Ui, lang: i18n::Lang, updates: &serde_json::Value) {
    let n = |k: &str| updates[k].as_u64().unwrap_or(0);
    let total = n("projects") + n("tasks") + n("epics") + n("templates");
    if total > 0 {
        ui.label(i18n::tr(
            lang,
            &i18n::Msg::new("ui-import-updates")
                .arg("projects", n("projects"))
                .arg("tasks", n("tasks")),
        ));
    }
}

fn counts(ui: &mut egui::Ui, lang: i18n::Lang, creates: &serde_json::Value) {
    if !creates.is_object() {
        return;
    }
    let n = |k: &str| creates[k].as_u64().unwrap_or(0);
    ui.label(i18n::tr(
        lang,
        &i18n::Msg::new("ui-import-counts")
            .arg("projects", n("projects"))
            .arg("tasks", n("tasks"))
            .arg("comments", n("comments"))
            .arg("agents", n("agents"))
            .arg("activity", n("activity")),
    ));
}

/// Guarda el filtro, la forma y las columnas actuales como vista (RF-VIS-12).
pub(crate) fn save_view_intent(snap: &Snapshot, st: &UiState, name: &str) -> Intent {
    let layout = if crate::state::ViewKind::TASK_VIEWS.contains(&st.view) {
        st.view.code()
    } else {
        "list"
    };
    let project = match st.scope {
        crate::data::Scope::Project(id) => snap.project(id).map(|p| p.key.clone()),
        crate::data::Scope::All => None,
    }
    .unwrap_or_default();
    let columns = st.table.columns.join(",");
    Intent::run(
        "view.save",
        &[
            ("name", name),
            ("filter", st.filter.text.trim()),
            ("layout", layout),
            ("columns", &columns),
            ("project", &project),
        ],
    )
}

/// Editor de atajos (RF-VIS-14): clic en una combinación y pulsar la nueva.
fn shortcuts(ui: &mut egui::Ui, st: &mut UiState) {
    use crate::keymap::{Action, Keymap};
    let lang = st.lang;
    let conflicts = st.keymap.conflicts();
    egui::Grid::new("shortcuts")
        .num_columns(3)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for action in Action::ALL {
                ui.label(t(lang, action.label_key()));
                let text = if st.capturing == Some(action) {
                    t(lang, "ui-shortcut-press")
                } else {
                    st.keymap
                        .get(action)
                        .map_or_else(|| "—".to_owned(), |s| s.text())
                };
                let mut rich = RichText::new(text).monospace();
                if conflicts.contains(&action) {
                    rich = rich.color(theme::RED);
                }
                if ui.button(rich).clicked() {
                    st.capturing = Some(action);
                }
                if st.keymap.get(action).is_some()
                    && ui
                        .small_button("×")
                        .on_hover_text(t(lang, "ui-shortcut-clear"))
                        .clicked()
                {
                    st.keymap.set(action, None);
                }
                ui.end_row();
            }
            for (keys, what) in [
                ("Supr", "ui-shortcut-delete"),
                ("Tab", "ui-shortcut-indent"),
                ("Shift+Tab", "ui-shortcut-outdent"),
                ("Esc", "ui-shortcut-escape"),
            ] {
                ui.label(t(lang, what));
                ui.label(RichText::new(keys).monospace().weak());
                ui.end_row();
            }
        });
    if !conflicts.is_empty() {
        ui.colored_label(theme::RED, t(lang, "ui-shortcut-conflict"));
    }
    if ui.button(t(lang, "ui-shortcut-reset")).clicked() {
        st.keymap = Keymap::default();
        st.capturing = None;
    }
}

/// Eliminar información (D23): qué se borra, la confirmación escrita cuando
/// hace falta y el aviso de la copia que se guarda antes.
fn delete_data(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    lang: i18n::Lang,
    f: &mut crate::state::DeleteForm,
    out: &mut Vec<Intent>,
) {
    use crate::state::DeleteWhat;
    ui.horizontal_wrapped(|ui| {
        for what in DeleteWhat::ALL {
            if ui
                .selectable_label(f.what == what, t(lang, what.label_key()))
                .clicked()
                && f.what != what
            {
                f.what = what;
                f.confirm.clear();
            }
        }
    });
    ui.separator();
    ui.add(egui::Label::new(t(lang, f.what.hint_key())).wrap());
    ui.add_space(4.0);
    match f.what {
        DeleteWhat::Trash => {
            ui.label(i18n::tr(
                lang,
                &i18n::Msg::new("ui-delete-trash-count").arg("count", snap.trash.len()),
            ));
        }
        DeleteWhat::History => {
            ui.horizontal(|ui| {
                ui.label(t(lang, "ui-delete-history-before"));
                if let Some(Some(d)) = crate::widgets::date_picker::date_button(
                    ui,
                    "delete-before",
                    Some(f.before),
                    lang,
                ) {
                    f.before = d;
                }
            });
        }
        DeleteWhat::Project => {
            let current = f
                .project
                .clone()
                .unwrap_or_else(|| t(lang, "ui-delete-pick-project"));
            egui::ComboBox::from_id_salt("delete_project")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for p in &snap.projects {
                        if ui
                            .selectable_label(
                                f.project.as_deref() == Some(p.key.as_str()),
                                format!("{} · {}", p.key, p.name),
                            )
                            .clicked()
                        {
                            f.project = Some(p.key.clone());
                            f.confirm.clear();
                        }
                    }
                });
        }
        DeleteWhat::Workspace => {}
    }
    if let Some(expected) = f.expected(snap).filter(|e| !e.is_empty()) {
        ui.label(i18n::tr(
            lang,
            &i18n::Msg::new("ui-delete-type-to-confirm").arg("text", expected.as_str()),
        ));
        ui.add(egui::TextEdit::singleline(&mut f.confirm).hint_text(expected.as_str()));
    }
    ui.add_space(4.0);
    ui.colored_label(theme::ORANGE, t(lang, "ui-delete-backup-first"));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                f.ready(snap),
                egui::Button::new(RichText::new(t(lang, "ui-delete")).color(theme::RED)),
            )
            .clicked()
        {
            out.push(f.intent());
        }
    });
}
