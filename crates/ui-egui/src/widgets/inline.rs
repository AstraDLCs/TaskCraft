//! Cajas de texto que aparecen en su sitio (subtarea nueva, celda de la
//! tabla…): toman el foco al aparecer y dicen cómo terminó la edición.

use eframe::egui::{self, Response, Ui};

/// Cómo terminó una edición en el sitio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Sigue escribiendo.
    Editing,
    /// Enter: guardar.
    Submit,
    /// Escape o clic fuera: descartar.
    Cancel,
}

/// Llamar justo después de añadir la caja. Pide el foco solo mientras no lo
/// tiene y no acaba de perderlo: pedirlo en cada frame lo recupera en el
/// mismo frame en que Enter lo suelta, y la edición nunca termina.
pub fn outcome(ui: &Ui, edit: &Response) -> Outcome {
    if edit.lost_focus() {
        return if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            Outcome::Submit
        } else {
            Outcome::Cancel
        };
    }
    if !edit.has_focus() {
        edit.request_focus();
    }
    Outcome::Editing
}
