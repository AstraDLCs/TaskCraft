//! Texto pintado a mano (Gantt, grafo) que no puede salirse de su sitio.

use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{Color32, FontId, Galley, Ui};

/// Una línea que cabe en `max_width`: si no, se corta con «…» por el ancho
/// real de las letras, no por cuántas son.
pub fn fitted(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping {
        max_width: max_width.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}
