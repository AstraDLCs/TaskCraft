//! Diagramas Mermaid (RF-MOD-19, D21): la lista de cada proyecto, el diagrama
//! elegido con zoom, las tareas que lo citan y el editor con vista previa.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout, RichText, Ui, vec2};
use i18n::{Msg, t, tr};
use model::ProjectStatus;

use crate::data::{DiagramLine, Scope, Snapshot};
use crate::state::{DiagramEditor, Intent, UiState};
use crate::theme;
use crate::widgets::diagram;
use crate::widgets::icons::{self, Icon};
use crate::widgets::markdown::Markdown;

/// Pausa al escribir antes de volver a dibujar la vista previa.
const PREVIEW_DELAY: Duration = Duration::from_millis(350);
/// Con menos ancho, la lista va encima del diagrama.
const SIDE_BY_SIDE: f32 = 760.0;
const LIST_W: f32 = 250.0;
const ZOOMS: [f32; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0];

pub fn show(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    md: &mut Markdown,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    if st.diagrams.editor.is_some() {
        editor(ui, snap, st, md, out);
        return;
    }
    ui.add(egui::Label::new(RichText::new(t(lang, "ui-diagrams-hint")).weak()).wrap());
    ui.add_space(4.0);
    let projects: Vec<_> = snap
        .projects
        .iter()
        .filter(|p| match st.scope {
            Scope::Project(id) => p.id == id,
            Scope::All => p.status == ProjectStatus::Active,
        })
        .collect();
    if projects.is_empty() {
        ui.weak(t(lang, "ui-no-projects"));
        return;
    }
    let visible: Vec<&DiagramLine> = projects
        .iter()
        .flat_map(|p| snap.diagrams_of(p.id))
        .collect();
    if !visible
        .iter()
        .any(|d| Some(d.diagram.id) == st.diagrams.selected)
    {
        st.diagrams.selected = visible.first().map(|d| d.diagram.id);
    }
    let selected = st.diagrams.selected.and_then(|id| snap.diagram(id));

    let list = |ui: &mut Ui, st: &mut UiState| {
        egui::ScrollArea::vertical()
            .id_salt("diagram_list")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for p in &projects {
                    ui.horizontal(|ui| {
                        ui.strong(format!("{} · {}", p.key, p.name));
                        if icons::button(ui, Icon::Plus, true, &t(lang, "ui-diagram-new")).clicked()
                        {
                            st.diagrams.editor = Some(DiagramEditor::new(p.id));
                        }
                    });
                    let lines: Vec<_> = snap.diagrams_of(p.id).collect();
                    if lines.is_empty() {
                        ui.weak(t(lang, "ui-diagrams-empty"));
                    }
                    for d in lines {
                        let chosen = st.diagrams.selected == Some(d.diagram.id);
                        let text = format!("{} · {}", d.key, d.diagram.title);
                        let r = ui
                            .horizontal(|ui| {
                                icons::show(ui, Icon::Diagram, theme::GRAY);
                                let r = ui.selectable_label(chosen, text);
                                if !d.cited_by.is_empty() {
                                    ui.weak(d.cited_by.len().to_string());
                                }
                                r
                            })
                            .inner;
                        if r.clicked() {
                            st.diagrams.selected = Some(d.diagram.id);
                            st.diagrams.confirm_delete = None;
                        }
                    }
                    ui.add_space(6.0);
                }
            });
    };

    if ui.available_width() >= SIDE_BY_SIDE {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(LIST_W, ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| list(ui, st),
            );
            ui.separator();
            ui.vertical(|ui| match selected {
                Some(d) => detail(ui, st, md, d, out),
                None => {
                    ui.weak(t(lang, "ui-diagrams-none"));
                }
            });
        });
    } else {
        egui::CollapsingHeader::new(t(lang, "ui-view-diagrams"))
            .id_salt("diagram_list_narrow")
            .default_open(selected.is_none())
            .show(ui, |ui| list(ui, st));
        ui.separator();
        if let Some(d) = selected {
            detail(ui, st, md, d, out);
        }
    }
}

fn detail(
    ui: &mut Ui,
    st: &mut UiState,
    md: &mut Markdown,
    d: &DiagramLine,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    ui.horizontal_wrapped(|ui| {
        icons::show(ui, Icon::Diagram, ui.visuals().text_color());
        ui.heading(&d.diagram.title);
        ui.label(RichText::new(&d.key).monospace().weak());
        if icons::button(ui, Icon::Edit, true, &t(lang, "ui-edit")).clicked() {
            st.diagrams.editor = Some(DiagramEditor::edit(&d.diagram));
        }
        if ui
            .small_button(t(lang, "ui-diagram-copy"))
            .on_hover_text(t(lang, "ui-diagram-copy-hint"))
            .clicked()
        {
            ui.ctx().copy_text(format!("![[{}]]", d.key));
        }
        if icons::button(ui, Icon::Trash, true, &t(lang, "ui-diagram-delete")).clicked() {
            st.diagrams.confirm_delete = Some(d.diagram.id);
        }
    });
    if st.diagrams.confirm_delete == Some(d.diagram.id) {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr(
                lang,
                &Msg::new("ui-diagram-delete-confirm").arg("diagram", d.key.as_str()),
            ));
            if ui
                .button(RichText::new(t(lang, "ui-delete")).color(theme::RED))
                .clicked()
            {
                out.push(Intent::run(
                    "diagram.delete",
                    &[("diagram", &d.diagram.id.to_string())],
                ));
                st.diagrams.selected = None;
                st.diagrams.confirm_delete = None;
            }
            if ui.button(t(lang, "ui-cancel")).clicked() {
                st.diagrams.confirm_delete = None;
            }
        });
    }
    if !d.diagram.description.trim().is_empty() {
        md.show(ui, &d.diagram.description);
    }
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(t(lang, "ui-zoom"));
        let at = ZOOMS
            .iter()
            .position(|z| (*z - st.diagrams.zoom).abs() < 0.01)
            .unwrap_or(2);
        if ui
            .add_enabled(at > 0, egui::Button::new("−").small())
            .clicked()
        {
            st.diagrams.zoom = ZOOMS.get(at.saturating_sub(1)).copied().unwrap_or(1.0);
        }
        ui.label(format!("{:.0} %", st.diagrams.zoom * 100.0));
        if ui
            .add_enabled(at + 1 < ZOOMS.len(), egui::Button::new("+").small())
            .clicked()
        {
            st.diagrams.zoom = ZOOMS.get(at + 1).copied().unwrap_or(1.0);
        }
    });
    let height = (ui.available_height() - 90.0).max(160.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::both()
            .id_salt(("diagram", d.diagram.id))
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                diagram::zoomed(
                    ui,
                    &mut md.diagrams,
                    lang,
                    &d.diagram.source,
                    st.diagrams.zoom,
                );
            });
    });

    ui.add_space(4.0);
    ui.strong(tr(
        lang,
        &Msg::new("ui-diagram-cited-by").arg("count", d.cited_by.len()),
    ));
    if d.cited_by.is_empty() {
        ui.weak(tr(
            lang,
            &Msg::new("ui-diagram-not-cited").arg("key", d.key.as_str()),
        ));
    }
    ui.horizontal_wrapped(|ui| {
        for task in &d.cited_by {
            if ui.link(format!("{} · {}", task.key, task.title)).clicked() {
                out.push(Intent::OpenKey(task.key.clone()));
            }
        }
    });
}

/// Alta o edición, con la vista previa de la fuente; todo desplazable para
/// que en ventanas bajas se llegue a los botones y a la vista previa.
fn editor(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    md: &mut Markdown,
    out: &mut Vec<Intent>,
) {
    egui::ScrollArea::vertical()
        .id_salt("diagram_editor")
        .auto_shrink([false, false])
        .show(ui, |ui| editor_body(ui, snap, st, md, out));
}

fn editor_body(
    ui: &mut Ui,
    snap: &Snapshot,
    st: &mut UiState,
    md: &mut Markdown,
    out: &mut Vec<Intent>,
) {
    let lang = st.lang;
    let Some(ed) = st.diagrams.editor.as_mut() else {
        return;
    };
    let project_key = snap
        .project(ed.project)
        .map(|p| p.key.clone())
        .unwrap_or_default();
    ui.heading(match ed.id {
        Some(_) => t(lang, "ui-diagram-edit"),
        None => tr(
            lang,
            &Msg::new("ui-diagram-new-in").arg("project", project_key.as_str()),
        ),
    });
    ui.add_space(4.0);
    ui.label(t(lang, "ui-diagram-title"));
    ui.add(egui::TextEdit::singleline(&mut ed.title).desired_width(f32::INFINITY));
    ui.label(t(lang, "ui-diagram-description"));
    ui.add(
        egui::TextEdit::multiline(&mut ed.description)
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    ui.label(t(lang, "ui-diagram-source"));
    let source = ui.add(
        egui::TextEdit::multiline(&mut ed.source)
            .code_editor()
            .desired_rows(10)
            .desired_width(f32::INFINITY),
    );
    if source.changed() {
        ed.edited_at = Some(Instant::now());
    }
    if let Some(at) = ed.edited_at {
        let waited = at.elapsed();
        if waited >= PREVIEW_DELAY {
            ed.preview = ed.source.clone();
            ed.edited_at = None;
        } else {
            ui.ctx().request_repaint_after(PREVIEW_DELAY - waited);
        }
    }
    let mut cancel = false;
    ui.horizontal(|ui| {
        let ready = !ed.title.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new(t(lang, "ui-save")))
            .clicked()
        {
            let (id, args) = ed.command(&project_key);
            out.push(Intent::SaveDiagram { id, args });
        }
        cancel = ui.button(t(lang, "ui-cancel")).clicked();
    });
    if cancel {
        st.diagrams.editor = None;
    }
    let Some(ed) = st.diagrams.editor.as_ref() else {
        return;
    };
    ui.add_space(4.0);
    ui.strong(t(lang, "ui-diagram-preview"));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        diagram::fitted(ui, &mut md.diagrams, lang, &ed.preview);
    });
}
