//! Papelera (RF-HIS-08): lo eliminado se puede restaurar hasta la purga.

use eframe::egui::{self, RichText, Ui};
use i18n::t;

use super::common;
use crate::data::Snapshot;
use crate::state::{Dialog, Intent, UiState};

pub fn show(ui: &mut Ui, snap: &Snapshot, st: &mut UiState, out: &mut Vec<Intent>) {
    let lang = st.lang;
    ui.horizontal(|ui| {
        if let Some(ws) = &snap.workspace {
            ui.weak(i18n::tr(
                lang,
                &i18n::Msg::new("ui-trash-hint").arg("days", ws.settings.trash_days),
            ));
        }
        if !snap.trash.is_empty() && ui.button(t(lang, "ui-trash-empty-now")).clicked() {
            st.dialog = Some(Dialog::DeleteData(crate::state::DeleteForm::new(
                crate::state::DeleteWhat::Trash,
                None,
            )));
        }
    });
    if snap.trash.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak(t(lang, "ui-trash-empty")));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("trash")
        .show(ui, |ui| {
            egui::Grid::new("trash-grid")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    for row in &snap.trash {
                        ui.label(RichText::new(&row.key).monospace().weak());
                        ui.label(&row.task.title);
                        ui.weak(
                            row.task
                                .deleted_at
                                .map(|d| common::ago(lang, d))
                                .unwrap_or_default(),
                        );
                        if ui.button(t(lang, "ui-restore")).clicked() {
                            out.push(Intent::run("task.restore", &[("task", &row.key)]));
                        }
                        ui.end_row();
                    }
                });
        });
}
