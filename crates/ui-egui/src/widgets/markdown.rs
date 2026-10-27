//! Descripciones y comentarios en Markdown (RF-MOD-10, RF-HIS-02), con
//! enlaces a tareas `[[API-42]]` (RF-REL-04) y diagramas `[[API-D3]]` o
//! incrustados con `![[API-D3]]` en su propia línea (RF-MOD-19).

use std::collections::HashMap;

use eframe::egui::{OpenUrl, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use i18n::Lang;
use model::{DiagramKey, TaskKey};

use crate::data::Snapshot;
use crate::widgets::diagram::{self, DiagramCache};

/// Prefijo de los enlaces internos a tareas: `[[API-42]]` se dibuja como un
/// enlace a `taskcraft://task/API-42`, que la app abre en vez del navegador.
pub const TASK_LINK: &str = "taskcraft://task/";
/// Igual para los diagramas: `[[API-D3]]` → `taskcraft://diagram/API-D3`.
pub const DIAGRAM_LINK: &str = "taskcraft://diagram/";

pub struct Markdown {
    cache: CommonMarkCache,
    /// Texturas de los diagramas, también para la vista Diagramas.
    pub diagrams: DiagramCache,
    /// Fuente de cada diagrama por clave, para incrustarlos.
    sources: HashMap<String, String>,
    /// (ID, última modificación) de lo que hay en `sources`.
    seen: Vec<(model::DiagramId, model::Timestamp)>,
    lang: Lang,
}

impl Default for Markdown {
    fn default() -> Self {
        Self {
            cache: CommonMarkCache::default(),
            diagrams: DiagramCache::default(),
            sources: HashMap::new(),
            seen: Vec::new(),
            lang: Lang::Es,
        }
    }
}

/// Un trozo del texto: Markdown o un diagrama incrustado (su clave canónica).
#[derive(Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Embed(String),
}

impl Markdown {
    /// Pone al día los diagramas que se pueden incrustar y el idioma.
    pub fn prepare(&mut self, snap: &Snapshot, lang: Lang) {
        self.lang = lang;
        let same = self.seen.len() == snap.diagrams.len()
            && self
                .seen
                .iter()
                .zip(&snap.diagrams)
                .all(|((id, at), d)| *id == d.diagram.id && *at == d.diagram.updated_at);
        if same {
            return;
        }
        self.seen = snap
            .diagrams
            .iter()
            .map(|d| (d.diagram.id, d.diagram.updated_at))
            .collect();
        self.sources = snap
            .diagrams
            .iter()
            .map(|d| (d.key.clone(), d.diagram.source.clone()))
            .collect();
    }

    /// Diagramas ya dibujados en caché (tests).
    #[cfg(test)]
    pub fn diagrams_cached(&self) -> usize {
        self.diagrams.cached()
    }

    pub fn show(&mut self, ui: &mut Ui, text: &str) {
        let parts = split(text, |key| self.sources.contains_key(key));
        for (i, part) in parts.into_iter().enumerate() {
            ui.push_id(i, |ui| match part {
                Part::Text(md) => {
                    let md = link_mentions(&md);
                    CommonMarkViewer::new().show(ui, &mut self.cache, &md);
                }
                Part::Embed(key) => {
                    let source = self.sources.get(&key).cloned().unwrap_or_default();
                    let shown = diagram::fitted(ui, &mut self.diagrams, self.lang, &source);
                    if let Some(r) = shown {
                        let r = r
                            .on_hover_text(&key)
                            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
                        if r.clicked() {
                            ui.ctx()
                                .open_url(OpenUrl::same_tab(format!("{DIAGRAM_LINK}{key}")));
                        }
                    }
                }
            });
        }
    }
}

/// Parte el texto en las líneas que son solo `![[CLAVE]]` de un diagrama
/// conocido (`known`), fuera de los bloques de código.
fn split(text: &str, known: impl Fn(&str) -> bool) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut buffer = String::new();
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        let embed = (!fenced)
            .then(|| trimmed.strip_prefix("![[")?.strip_suffix("]]"))
            .flatten()
            .and_then(|key| DiagramKey::parse(key.trim()))
            .map(|key| key.to_string())
            .filter(|key| known(key));
        match embed {
            Some(key) => {
                if !buffer.trim().is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut buffer)));
                }
                buffer.clear();
                parts.push(Part::Embed(key));
            }
            None => buffer.push_str(line),
        }
    }
    if !buffer.trim().is_empty() || parts.is_empty() {
        parts.push(Part::Text(buffer));
    }
    parts
}

/// `[[API-42]]` → `[API-42](taskcraft://task/API-42)` (RF-REL-04) y
/// `[[API-D3]]` → `[API-D3](taskcraft://diagram/API-D3)` (RF-MOD-19). Un
/// `![[API-D3]]` en mitad de una línea queda como enlace (no como imagen).
pub fn link_mentions(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("[[") {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 32);
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let inner = after.find("]]").map(|end| (end, after[..end].trim()));
        match inner {
            Some((end, raw)) if TaskKey::parse(raw).is_some() => {
                let key = raw.to_uppercase();
                out.push_str(&format!("[{key}]({TASK_LINK}{key})"));
                rest = &after[end + 2..];
            }
            Some((end, raw)) if DiagramKey::parse(raw).is_some() => {
                let key = DiagramKey::parse(raw)
                    .map(|k| k.to_string())
                    .unwrap_or_default();
                if out.ends_with('!') {
                    out.pop();
                }
                out.push_str(&format!("[{key}]({DIAGRAM_LINK}{key})"));
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("[[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_become_task_links() {
        assert_eq!(
            link_mentions("Ver [[api-42]] y [[no es]]"),
            "Ver [API-42](taskcraft://task/API-42) y [[no es]]"
        );
        assert!(matches!(
            link_mentions("nada"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn diagram_citations_become_links_never_images() {
        assert_eq!(
            link_mentions("Ver [[api-d3]] o ![[API-D3]]."),
            "Ver [API-D3](taskcraft://diagram/API-D3) o [API-D3](taskcraft://diagram/API-D3)."
        );
    }

    #[test]
    fn embeds_split_the_text_outside_code_blocks() {
        let known = |k: &str| k == "API-D1";
        let text = "Antes\n![[api-d1]]\nDespués\n```\n![[API-D1]]\n```\n![[API-D9]]\n";
        let parts = split(text, known);
        assert_eq!(parts.len(), 3, "{parts:?}");
        assert_eq!(parts[0], Part::Text("Antes\n".into()));
        assert_eq!(parts[1], Part::Embed("API-D1".into()));
        let Part::Text(rest) = &parts[2] else {
            panic!("{parts:?}");
        };
        assert!(rest.contains("```\n![[API-D1]]\n```") && rest.contains("![[API-D9]]"));
        assert_eq!(split("", known), vec![Part::Text(String::new())]);
    }
}
