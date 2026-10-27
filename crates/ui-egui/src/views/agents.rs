//! Panel de agentes (RF-SUP-01): quién está conectado, en qué trabaja y desde
//! cuándo. Primero se registra la interfaz (D15, D22): el cliente LLM, con el
//! único token, que se puede copiar o renovar; dentro de ella, sus agentes,
//! que el supervisor añade, edita, desactiva o revoca.

use eframe::egui::{self, RichText, Ui};
use i18n::t;
use model::AgentStatus;

use super::common;
use crate::data::{AgentRow, Snapshot};
use crate::state::{AgentForm, Dialog, Intent, InterfaceForm, UiState};
use crate::theme;
use crate::widgets::{avatar, chip};

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    ui.horizontal(|ui| {
        if ui.button(t(lang, "ui-interface-register")).clicked() {
            st.dialog = Some(Dialog::Interface(InterfaceForm::default()));
        }
        ui.weak(i18n::tr(
            lang,
            &i18n::Msg::new("ui-agents-connected")
                .arg("n", snap.agents_connected())
                .arg("total", snap.agents.len()),
        ));
    });
    ui.add_space(6.0);
    if snap.interfaces.is_empty() && snap.agents.is_empty() {
        // Primero la interfaz (el cliente LLM, con el token); dentro, sus agentes.
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong(t(lang, "ui-agents-empty"));
            ui.add(egui::Label::new(t(lang, "ui-interfaces-first")).wrap());
            if ui.button(t(lang, "ui-interface-register")).clicked() {
                st.dialog = Some(Dialog::Interface(InterfaceForm::default()));
            }
        });
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("agents")
        .show(ui, |ui| {
            for i in &snap.interfaces {
                let agents: Vec<&AgentRow> = snap
                    .agents
                    .iter()
                    .filter(|r| r.agent.interface_id == Some(i.id))
                    .collect();
                interface_header(ui, snap, i, agents.len(), st, out);
                ui.indent(("interface", i.id), |ui| {
                    if agents.is_empty() && i.status != AgentStatus::Revoked {
                        ui.weak(t(lang, "ui-interface-no-agents"));
                    }
                    for row in agents {
                        agent_card(ui, snap, row, st, out);
                    }
                });
                ui.add_space(10.0);
            }
            let loose: Vec<&AgentRow> = snap
                .agents
                .iter()
                .filter(|r| r.agent.interface_id.is_none())
                .collect();
            if !loose.is_empty() {
                ui.strong(t(lang, "ui-no-interface"));
                ui.weak(t(lang, "ui-no-interface-hint"));
                for row in loose {
                    agent_card(ui, snap, row, st, out);
                }
            }
        });
}

/// La interfaz (D15, D22): tipo, estado, nivel, agentes y alcance, con sus
/// acciones: copiar o renovar el token, editarla, añadir agentes…
fn interface_header(
    ui: &mut Ui,
    snap: &Snapshot,
    i: &model::Interface,
    agents: usize,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    ui.horizontal_wrapped(|ui| {
        let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(dot.center(), 5.0, theme::parse_hex(&i.color));
        ui.heading(&i.name);
        if !i.kind.is_empty() {
            ui.label(RichText::new(&i.kind).monospace().small());
        }
        let color = match i.status {
            AgentStatus::Active => theme::GREEN,
            AgentStatus::Disabled => theme::ORANGE,
            AgentStatus::Revoked => theme::RED,
        };
        chip::chip(ui, &t(lang, i.status.label_key()), color);
        if i.paused {
            chip::chip(ui, &t(lang, "ui-paused"), theme::ORANGE);
        }
        chip::chip(ui, &t(lang, i.level.label_key()), theme::GRAY);
        chip::chip(
            ui,
            &i18n::tr(
                lang,
                &i18n::Msg::new("ui-interface-agents")
                    .arg("n", agents)
                    .arg("max", i.max_agents),
            ),
            theme::GRAY,
        );
        for p in &i.scope {
            if let Some(p) = snap.project(*p) {
                chip::chip(ui, &p.key, theme::BLUE);
            }
        }
    });
    if !i.description.is_empty() {
        ui.weak(&i.description);
    }
    if i.status == AgentStatus::Revoked {
        return;
    }
    let name = i.name.as_str();
    ui.horizontal_wrapped(|ui| {
        let room = agents < usize::try_from(i.max_agents).unwrap_or(usize::MAX);
        if ui
            .add_enabled(room, egui::Button::new(t(lang, "ui-agent-add")).small())
            .on_disabled_hover_text(t(lang, "ui-interface-full"))
            .clicked()
        {
            st.dialog = Some(Dialog::Agent(AgentForm::new(name)));
        }
        if ui
            .small_button(t(lang, "ui-copy-token"))
            .on_hover_text(t(lang, "ui-copy-token-hint"))
            .clicked()
        {
            out.push(Intent::CopyToken(i.name.clone()));
        }
        if ui.small_button(t(lang, "ui-new-token")).clicked() {
            st.dialog = Some(Dialog::ConfirmNewToken(i.name.clone()));
        }
        if ui.small_button(t(lang, "ui-edit")).clicked() {
            st.dialog = Some(Dialog::Interface(InterfaceForm::edit(i, snap)));
        }
        let (label, cmd) = if i.paused {
            ("ui-resume", "interface.resume")
        } else {
            ("ui-pause", "interface.pause")
        };
        if ui.small_button(t(lang, label)).clicked() {
            out.push(Intent::run(cmd, &[("interface", name)]));
        }
        if i.status == AgentStatus::Active {
            if ui.small_button(t(lang, "ui-disable")).clicked() {
                out.push(Intent::run("interface.disable", &[("interface", name)]));
            }
        } else if ui.small_button(t(lang, "ui-enable")).clicked() {
            out.push(Intent::run("interface.enable", &[("interface", name)]));
        }
        if ui
            .small_button(RichText::new(t(lang, "ui-revoke")).color(theme::RED))
            .clicked()
        {
            st.dialog = Some(Dialog::ConfirmInterfaceRevoke(i.name.clone()));
        }
    });
}

fn agent_card(
    ui: &mut Ui,
    snap: &Snapshot,
    row: &AgentRow,
    st: &mut UiState,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let a = &row.agent;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            avatar::avatar(ui, &row.who.seed, row.who.color, 40.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(&a.name);
                    let (text, color) = match a.status {
                        AgentStatus::Active if row.connected => {
                            (t(lang, "ui-connected"), theme::GREEN)
                        }
                        AgentStatus::Active => (t(lang, "ui-idle"), theme::GRAY),
                        AgentStatus::Disabled => (t(lang, a.status.label_key()), theme::ORANGE),
                        AgentStatus::Revoked => (t(lang, a.status.label_key()), theme::RED),
                    };
                    chip::chip(ui, &text, color);
                    if a.paused {
                        chip::chip(ui, &t(lang, "ui-paused"), theme::ORANGE);
                    }
                    chip::chip(ui, &t(lang, a.level.label_key()), theme::GRAY);
                    for p in &a.scope {
                        if let Some(p) = snap.project(*p) {
                            chip::chip(ui, &p.key, theme::BLUE);
                        }
                    }
                    for c in &a.capabilities {
                        chip::chip(ui, c, theme::VIOLET);
                    }
                });
                ui.horizontal(|ui| {
                    if !a.model.is_empty() {
                        ui.label(RichText::new(&a.model).monospace().small());
                    }
                    if let Some(seen) = row.last_seen {
                        ui.weak(format!(
                            "{} {}",
                            t(lang, "ui-last-seen"),
                            common::ago(lang, seen)
                        ));
                    }
                });
                if !a.description.is_empty() {
                    ui.weak(&a.description);
                }
            });
        });
        if row.working_on.is_empty() {
            ui.weak(t(lang, "ui-agent-no-work"));
        } else {
            for (id, key, title, since) in &row.working_on {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(key).monospace());
                    if ui.link(title).clicked() {
                        out.push(Intent::Select(Some(*id)));
                    }
                    ui.weak(common::ago(lang, *since));
                });
            }
        }
        ui.horizontal(|ui| {
            if a.status == AgentStatus::Revoked {
                return;
            }
            if ui.button(t(lang, "ui-edit")).clicked() {
                st.dialog = Some(Dialog::Agent(AgentForm::edit(a, snap)));
            }
            let (label, cmd) = if a.paused {
                ("ui-resume", "agent.resume")
            } else {
                ("ui-pause", "agent.pause")
            };
            if ui.button(t(lang, label)).clicked() {
                out.push(Intent::run(cmd, &[("agent", &a.name)]));
            }
            if a.status == AgentStatus::Active {
                if ui.button(t(lang, "ui-disable")).clicked() {
                    out.push(Intent::run("agent.disable", &[("agent", &a.name)]));
                }
            } else if ui.button(t(lang, "ui-enable")).clicked() {
                out.push(Intent::run("agent.enable", &[("agent", &a.name)]));
            }
            if ui
                .button(RichText::new(t(lang, "ui-revoke")).color(theme::RED))
                .clicked()
            {
                st.dialog = Some(Dialog::ConfirmRevoke(a.name.clone()));
            }
        });
    });
    ui.add_space(6.0);
}
