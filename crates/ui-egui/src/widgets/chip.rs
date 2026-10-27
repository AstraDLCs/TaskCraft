//! Pastillas de color: etiquetas, estados y avisos breves.

use eframe::egui::{self, Color32, RichText, Ui};

use crate::theme;

pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(color)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(theme::on(color)));
        })
        .response
}
