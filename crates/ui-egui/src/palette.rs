//! Paleta de comandos (RF-VIS-13): todo comando del registro, con un formulario
//! generado a partir de sus parámetros declarados.

use eframe::egui::{self, RichText};
use engine::{Args, Kind};
use i18n::t;

use crate::state::{Intent, UiState};

pub fn show(ctx: &egui::Context, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    let Some(mut palette) = st.palette.take() else {
        return;
    };
    let mut open = true;
    let (top, max) = crate::dialogs::fit_window(ctx, 60.0);
    egui::Window::new(t(lang, "ui-palette"))
        .open(&mut open)
        .collapsible(false)
        .default_width(520.0_f32.min(max.x))
        .max_size(max)
        .anchor(egui::Align2::CENTER_TOP, [0.0, top])
        .show(ctx, |ui| match palette.chosen {
            None => {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut palette.query)
                        .hint_text(t(lang, "ui-palette-hint"))
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                let q = palette.query.to_lowercase();
                let matches: Vec<usize> = palette
                    .commands
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        q.is_empty()
                            || c.id.contains(&q)
                            || t(lang, c.label).to_lowercase().contains(&q)
                    })
                    .map(|(i, _)| i)
                    .collect();
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                egui::ScrollArea::vertical()
                    .max_height((max.y - 80.0).clamp(120.0, 360.0))
                    .show(ui, |ui| {
                        for (n, &i) in matches.iter().enumerate() {
                            let Some(c) = palette.commands.get(i) else {
                                continue;
                            };
                            let clicked = ui
                                .selectable_label(
                                    n == 0 && enter,
                                    format!("{}  —  {}", c.id, t(lang, c.label)),
                                )
                                .clicked();
                            if clicked || (n == 0 && enter) {
                                palette.chosen = Some(i);
                            }
                        }
                    });
            }
            Some(i) => {
                let Some(cmd) = palette.commands.get(i).cloned() else {
                    palette.chosen = None;
                    return;
                };
                ui.horizontal(|ui| {
                    if ui.small_button("<").clicked() {
                        palette.chosen = None;
                        palette.result = None;
                    }
                    ui.strong(cmd.id);
                });
                ui.weak(t(lang, cmd.label));
                egui::Grid::new("palette-form")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        for p in cmd.params {
                            let label = if p.required {
                                format!("{} *", p.name)
                            } else {
                                p.name.to_owned()
                            };
                            ui.label(RichText::new(label).monospace()).on_hover_text(
                                i18n::lookup(lang, &p.help_key()).unwrap_or_default(),
                            );
                            let value = palette.values.entry(p.name).or_default();
                            match p.kind {
                                Kind::Bool => {
                                    let mut b = value == "true";
                                    if ui.checkbox(&mut b, "").changed() {
                                        *value = if b { "true".into() } else { String::new() };
                                    }
                                }
                                Kind::Enum(options) => {
                                    egui::ComboBox::from_id_salt(("pal", p.name))
                                        .selected_text(if value.is_empty() {
                                            "—"
                                        } else {
                                            value.as_str()
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(value, String::new(), "—");
                                            for o in options {
                                                ui.selectable_value(value, (*o).to_owned(), *o);
                                            }
                                        });
                                }
                                Kind::Json => {
                                    ui.add(
                                        egui::TextEdit::multiline(value)
                                            .code_editor()
                                            .desired_rows(4),
                                    );
                                }
                                _ => {
                                    ui.text_edit_singleline(value);
                                }
                            }
                            ui.end_row();
                        }
                    });
                if ui.button(t(lang, "ui-run")).clicked() {
                    let args = palette
                        .values
                        .iter()
                        .filter(|(_, v)| !v.trim().is_empty())
                        .fold(Args::none(), |a, (k, v)| a.with(*k, v.trim()));
                    out.push(Intent::RunPalette { id: cmd.id, args });
                }
                if let Some(result) = &palette.result {
                    match result {
                        Ok(json) => {
                            egui::ScrollArea::vertical()
                                .max_height(240.0)
                                .show(ui, |ui| {
                                    ui.label(RichText::new(json).monospace().small());
                                });
                        }
                        Err(e) => {
                            ui.colored_label(ui.visuals().error_fg_color, e);
                        }
                    }
                }
            }
        });
    if open && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.palette = Some(palette);
    }
}
