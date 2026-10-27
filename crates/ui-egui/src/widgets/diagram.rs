//! Diagramas Mermaid dibujados (RF-MOD-19, D21): la fuente pasa a píxeles con
//! el crate `diagram` y queda como textura, en caché por fuente, tema y escala.
//! Si no se puede dibujar, se ve el aviso con la fuente.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use diagram::{DiagramError, Theme};
use eframe::egui::{self, ColorImage, RichText, TextureHandle, TextureOptions, Ui, Vec2};
use i18n::{Lang, Localize, tr};

use crate::theme;
use crate::widgets::icons::{self, Icon};

/// Más entradas que esto y la caché empieza de cero.
const MAX_ENTRIES: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    source: u64,
    dark: bool,
    /// Escala del raster en cuartos (4 = 1×).
    quarters: u32,
}

type Entry = Result<(TextureHandle, Vec2), DiagramError>;

#[derive(Default)]
pub struct DiagramCache {
    entries: HashMap<Key, Entry>,
}

impl DiagramCache {
    /// Texturas (o errores) guardados.
    #[cfg(test)]
    pub fn cached(&self) -> usize {
        self.entries.len()
    }

    /// Textura de `source` para dibujarla a `zoom` (1 = tamaño natural) y su
    /// tamaño natural en puntos. Se rasteriza a la densidad de la pantalla.
    pub fn get(&mut self, ui: &Ui, source: &str, zoom: f32) -> Entry {
        let dark = ui.visuals().dark_mode;
        let scale = (ui.ctx().pixels_per_point() * zoom).clamp(0.25, 8.0);
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        let key = Key {
            source: hasher.finish(),
            dark,
            quarters: (scale * 4.0).round() as u32,
        };
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries
            .entry(key)
            .or_insert_with(|| {
                let quantized = key.quarters as f32 / 4.0;
                let theme = if dark { Theme::Dark } else { Theme::Light };
                let image = diagram::render(source, theme, quantized)?;
                let size = [image.width as usize, image.height as usize];
                let texture = ui.ctx().load_texture(
                    format!("diagram-{:x}-{}", key.source, key.quarters),
                    ColorImage::from_rgba_premultiplied(size, &image.rgba),
                    TextureOptions::LINEAR,
                );
                let natural = Vec2::new(image.width as f32, image.height as f32) / quantized;
                Ok((texture, natural))
            })
            .clone()
    }
}

/// El diagrama a su tamaño natural, reducido si no cabe a lo ancho. Devuelve
/// la respuesta de la imagen (clic) o `None` si se mostró el error.
pub fn fitted(
    ui: &mut Ui,
    cache: &mut DiagramCache,
    lang: Lang,
    source: &str,
) -> Option<egui::Response> {
    match cache.get(ui, source, 1.0) {
        Ok((texture, natural)) => {
            let width = natural.x.min(ui.available_width()).max(1.0);
            let size = natural * (width / natural.x.max(1.0));
            Some(ui.add(egui::Image::new((texture.id(), size)).sense(egui::Sense::click())))
        }
        Err(e) => {
            error(ui, lang, &e, source);
            None
        }
    }
}

/// El diagrama a `zoom`, sin ajustar al ancho (la vista lo pone en un
/// desplazable).
pub fn zoomed(ui: &mut Ui, cache: &mut DiagramCache, lang: Lang, source: &str, zoom: f32) {
    match cache.get(ui, source, zoom) {
        Ok((texture, natural)) => {
            ui.add(egui::Image::new((texture.id(), natural * zoom)));
        }
        Err(e) => error(ui, lang, &e, source),
    }
}

/// Aviso de que no se puede dibujar, con el motivo y la fuente.
pub fn error(ui: &mut Ui, lang: Lang, e: &DiagramError, source: &str) {
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, theme::ORANGE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                icons::show(ui, Icon::Warning, theme::ORANGE);
                ui.label(RichText::new(tr(lang, &e.msg())).color(theme::ORANGE));
            });
            if !source.trim().is_empty() {
                ui.add(egui::Label::new(RichText::new(source).monospace().weak()).wrap());
            }
        });
}
