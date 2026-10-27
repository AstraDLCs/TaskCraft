//! El flujo de cada proyecto tal como se ve (D10): columnas con su nombre
//! visible, estado base y etapa, y el selector de estado que las ofrece.

use eframe::egui::{self, RichText, Ui};
use i18n::{Lang, t};
use model::{ProjectId, Status, Task};

use crate::data::Snapshot;
use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Clave para `task.move status=`.
    pub key: String,
    pub base: Status,
    /// Etapa propia (no un estado base).
    pub stage: bool,
    pub name: String,
    /// Límite WIP (RF-COO-08).
    pub wip_limit: Option<u32>,
    /// Agente al que pertenece (D20).
    pub owner: Option<model::AgentId>,
}

impl Column {
    /// ¿Está `task` en esta columna? En una base solo las que no tienen etapa
    /// (si `stages` = el flujo muestra etapas); si no, todas las de esa base.
    pub fn holds(&self, task: &Task, stages: bool) -> bool {
        if self.stage {
            task.stage.as_deref() == Some(self.key.as_str())
        } else {
            task.status == self.base && (!stages || task.stage.is_none())
        }
    }
}

/// Nombre visible de una columna: el suyo, «Algún día» (RF-VIS-16) o el del
/// estado base traducido.
pub fn def_name(d: &model::StatusDef, lang: Lang) -> String {
    if !d.name.is_empty() {
        d.name.clone()
    } else if d.key == rules::SOMEDAY {
        t(lang, "stage-someday")
    } else {
        t(lang, d.base.label_key())
    }
}

/// Columnas de un proyecto, o los 6 estados base para varios proyectos.
pub fn columns(snap: &Snapshot, project: Option<ProjectId>, lang: Lang) -> Vec<Column> {
    match project.and_then(|id| snap.project(id)) {
        Some(p) => rules::workflow(p)
            .into_iter()
            .map(|d| Column {
                stage: !d.is_base(),
                wip_limit: d.wip_limit,
                owner: d.owner,
                name: def_name(&d, lang),
                key: d.key,
                base: d.base,
            })
            .collect(),
        None => Status::ALL
            .iter()
            .map(|s| Column {
                key: s.as_str().to_owned(),
                base: *s,
                stage: false,
                name: t(lang, s.label_key()),
                wip_limit: None,
                owner: None,
            })
            .collect(),
    }
}

/// Nombre visible del estado de una tarea: su etapa o su estado base (con
/// el nombre que le dé su proyecto).
pub fn status_name(snap: &Snapshot, task: &Task, lang: Lang) -> String {
    let key = task.stage.as_deref().unwrap_or(task.status.as_str());
    columns(snap, Some(task.project_id), lang)
        .into_iter()
        .find(|c| c.key == key)
        .map(|c| c.name)
        .unwrap_or_else(|| t(lang, task.status.label_key()))
}

/// Estado actual y a qué columnas se puede pasar: las del mismo estado base
/// y las de los estados base permitidos. Devuelve la clave elegida si cambió.
pub fn status_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    task: &Task,
    snap: &Snapshot,
    lang: Lang,
) -> Option<String> {
    let current = task
        .stage
        .clone()
        .unwrap_or_else(|| task.status.as_str().to_owned());
    let allowed = rules::allowed_targets(task.status);
    let mut chosen = current.clone();
    let cols = columns(snap, Some(task.project_id), lang);
    egui::ComboBox::from_id_salt(id)
        .selected_text(
            RichText::new(status_name(snap, task, lang)).color(theme::status(task.status)),
        )
        .width(120.0)
        .show_ui(ui, |ui| {
            for c in cols
                .iter()
                .filter(|c| c.base == task.status || allowed.contains(&c.base))
            {
                ui.selectable_value(
                    &mut chosen,
                    c.key.clone(),
                    RichText::new(&c.name).color(theme::status(c.base)),
                );
            }
        });
    (chosen != current).then_some(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::StatusDef;

    #[test]
    fn project_columns_include_renamed_bases_and_stages() {
        let mut snap = Snapshot::default();
        let mut p = crate::data::test_project("API");
        p.workflow = vec![StatusDef::new("todo", "Listo", Status::Todo)];
        p.workflow
            .push(StatusDef::new("qa", "QA", Status::InReview));
        let id = p.id;
        snap.projects.push(p);
        let cols = columns(&snap, Some(id), Lang::Es);
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Backlog",
                "Listo",
                "En progreso",
                "En revisión",
                "QA",
                "Hecho",
                "Cancelado"
            ]
        );
        assert_eq!(columns(&snap, None, Lang::Es).len(), 6);
    }
}
