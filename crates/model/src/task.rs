use std::fmt;

use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use std::collections::BTreeMap;

use crate::{
    Actor, AgentId, CommentId, EpicId, Estimate, ItemId, LabelId, ProjectId, TaskId, Timestamp,
    When,
};

/// Tarea (RF-MOD-03). Una subtarea es una tarea completa con `parent_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub project_id: ProjectId,
    /// Número dentro del proyecto: `API-<number>`.
    pub number: u64,
    pub parent_id: Option<TaskId>,
    pub title: String,
    /// Markdown (fuente).
    pub description: String,
    pub status: Status,
    pub priority: Priority,
    pub start: Option<When>,
    pub due: Option<When>,
    pub labels: Vec<LabelId>,
    /// Agentes (o el supervisor) a cargo, sin papel fijo: lo que hace cada
    /// uno lo dice su función (D20).
    #[serde(default)]
    pub assignees: Vec<Actor>,
    pub creator: Actor,
    pub checklist: Vec<CheckItem>,
    /// Criterios de aceptación (RF-MOD-11).
    pub criteria: Vec<CheckItem>,
    /// Se incrementa con cada cambio; base de la concurrencia optimista (RF-COO-06).
    pub revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub archived_at: Option<Timestamp>,
    /// Borrado lógico: la tarea está en la papelera (RF-HIS-08).
    pub deleted_at: Option<Timestamp>,
    /// Agente que la tiene tomada y hasta cuándo (RF-COO-01).
    pub claim: Option<Claim>,
    /// Pregunta pendiente para el supervisor; bloquea a los agentes (RF-COO-10).
    pub needs_human: Option<String>,
    /// Bloqueo humano: los agentes pueden leerla pero no modificarla (RF-SUP-05).
    pub locked: bool,
    /// Etapa propia del proyecto dentro del estado base (D10).
    #[serde(default)]
    pub stage: Option<String>,
    /// Esfuerzo estimado (RF-MOD-14).
    #[serde(default)]
    pub estimate: Option<Estimate>,
    #[serde(default)]
    pub epic_id: Option<EpicId>,
    #[serde(default)]
    pub milestone_id: Option<EpicId>,
    /// Valores de los campos personalizados, como texto canónico (RF-MOD-15).
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    /// Al terminarla nace la siguiente (RF-MOD-18, D17).
    #[serde(default)]
    pub recurrence: Option<crate::Recurrence>,
    /// Ciclos o sprints en los que está planificada (RF-INT-05), en el orden en
    /// que se añadieron. Al cerrar uno con la tarea abierta, gana el siguiente y
    /// conserva el cerrado.
    #[serde(default)]
    pub cycles: Vec<crate::CycleId>,
}

/// Claim con lease: la tarea es de `agent` hasta `until`, salvo que lo renueve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub agent: AgentId,
    pub since: Timestamp,
    pub until: Timestamp,
    /// Revisión de la tarea tras la última escritura del agente. Si alguien más
    /// la cambió después, la próxima escritura del agente es un conflicto.
    pub revision: u64,
}

impl Claim {
    pub fn is_active(&self, now: Timestamp) -> bool {
        self.until > now
    }
}

impl Task {
    /// El primer asignado: el que muestran las vistas cuando solo cabe uno.
    pub fn assignee(&self) -> Option<&Actor> {
        self.assignees.first()
    }

    pub fn is_assigned(&self, actor: &Actor) -> bool {
        self.assignees.contains(actor)
    }

    /// Añade `actor` al final si no estaba; `true` si lo añadió.
    pub fn assign(&mut self, actor: Actor) -> bool {
        if self.is_assigned(&actor) {
            return false;
        }
        self.assignees.push(actor);
        true
    }

    pub fn in_cycle(&self, cycle: crate::CycleId) -> bool {
        self.cycles.contains(&cycle)
    }

    /// Añade el ciclo al final si no estaba; `true` si lo añadió.
    pub fn add_cycle(&mut self, cycle: crate::CycleId) -> bool {
        if self.in_cycle(cycle) {
            return false;
        }
        self.cycles.push(cycle);
        true
    }

    /// Quita el ciclo; `true` si estaba.
    pub fn remove_cycle(&mut self, cycle: crate::CycleId) -> bool {
        let before = self.cycles.len();
        self.cycles.retain(|c| *c != cycle);
        self.cycles.len() != before
    }

    /// Lo deja como único asignado (o ninguno).
    pub fn set_assignee(&mut self, actor: Option<Actor>) {
        self.assignees = actor.into_iter().collect();
    }

    pub fn is_live(&self) -> bool {
        self.archived_at.is_none() && self.deleted_at.is_none()
    }

    /// Claim vigente en `now`; uno vencido cuenta como libre aunque nadie lo
    /// haya barrido todavía.
    pub fn active_claim(&self, now: Timestamp) -> Option<&Claim> {
        self.claim.as_ref().filter(|c| c.is_active(now))
    }
}

/// Paso de checklist o criterio de aceptación, marcable de forma individual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckItem {
    pub id: ItemId,
    pub text: String,
    pub done: bool,
}

/// `blocker` bloquea a `blocked` (RF-REL-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dependency {
    pub blocker: TaskId,
    pub blocked: TaskId,
}

/// Clave legible de una tarea: `API-42`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskKey {
    pub project: String,
    pub number: u64,
}

impl TaskKey {
    /// `API-42` → (`API`, 42). Devuelve `None` si no tiene esa forma.
    pub fn parse(text: &str) -> Option<Self> {
        let (project, number) = text.rsplit_once('-')?;
        Some(Self {
            project: project.to_uppercase(),
            number: number.parse().ok()?,
        })
    }
}

impl fmt::Display for TaskKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.project, self.number)
    }
}

/// Relación no bloqueante entre dos tareas (RF-REL-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Relation {
    pub a: TaskId,
    pub b: TaskId,
    pub kind: RelationKind,
}

text_enum! {
    /// `relates`: simétrica. `duplicates`: `a` es duplicada de `b`.
    pub enum RelationKind ("error-parse-relation", "relation") {
        Relates = "relates" | "relacionada",
        Duplicates = "duplicates" | "duplicada",
    }
}

/// `[[API-42]]` en la descripción de `source` o en uno de sus comentarios
/// apunta a `target` (RF-REL-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Mention {
    pub source: TaskId,
    pub comment: Option<CommentId>,
    pub target: TaskId,
}

text_enum! {
    /// Estado del flujo (RF-MOD-06).
    pub enum Status ("error-parse-status", "status") {
        Backlog = "backlog",
        Todo = "todo" | "por_hacer",
        InProgress = "in_progress" | "en_progreso",
        InReview = "in_review" | "en_revision",
        Done = "done" | "hecho",
        Cancelled = "cancelled" | "cancelado",
    }
}

impl Status {
    pub fn category(self) -> Category {
        match self {
            Self::Backlog | Self::Todo => Category::Pending,
            Self::InProgress => Category::Active,
            Self::InReview => Category::Review,
            Self::Done => Category::Done,
            Self::Cancelled => Category::Cancelled,
        }
    }
}

text_enum! {
    /// Categoría base de un estado. Los estados personalizados (RF-MOD-07) se
    /// mapean a una, para que agentes y métricas los entiendan.
    pub enum Category ("error-parse-category", "category") {
        Pending = "pending",
        Active = "active",
        Review = "review",
        Done = "done",
        Cancelled = "cancelled",
    }
}

impl Category {
    /// Terminada o cancelada: ya no bloquea a nadie.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

text_enum! {
    /// Prioridad (RF-MOD-08), de mayor a menor.
    pub enum Priority ("error-parse-priority", "priority") {
        Urgent = "urgent" | "urgente",
        High = "high" | "alta",
        Medium = "medium" | "media",
        Low = "low" | "baja",
        None = "none" | "ninguna",
    }
}

impl Priority {
    /// 0 = más urgente; para ordenar.
    pub fn rank(self) -> u8 {
        match self {
            Self::Urgent => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
            Self::None => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_accept_canonical_and_spanish() {
        assert_eq!("in_progress".parse::<Status>().unwrap(), Status::InProgress);
        assert_eq!("En progreso".parse::<Status>().unwrap(), Status::InProgress);
        assert_eq!("alta".parse::<Priority>().unwrap(), Priority::High);
        assert!("rapidísimo".parse::<Priority>().is_err());
    }

    #[test]
    fn task_key_roundtrip() {
        let key = TaskKey::parse("api-42").unwrap();
        assert_eq!(key.to_string(), "API-42");
        assert!(TaskKey::parse("API").is_none());
        assert!(TaskKey::parse("API-x").is_none());
    }
}

#[cfg(test)]
mod i18n_tests {
    use i18n::{Lang, Localize, has};

    use crate::{
        AgentStatus, Category, EpicKind, EpicStatus, EstimateUnit, FieldKind, InboxKind,
        InboxStatus, Priority, ProjectStatus, RelationKind, Status, TemplateKind, TimeSource,
    };

    #[test]
    fn every_enum_value_has_a_visible_name_in_both_languages() {
        let keys = Status::ALL
            .iter()
            .map(|v| v.label_key())
            .chain(Category::ALL.iter().map(|v| v.label_key()))
            .chain(Priority::ALL.iter().map(|v| v.label_key()))
            .chain(ProjectStatus::ALL.iter().map(|v| v.label_key()))
            .chain(AgentStatus::ALL.iter().map(|v| v.label_key()))
            .chain(InboxKind::ALL.iter().map(|v| v.label_key()))
            .chain(InboxStatus::ALL.iter().map(|v| v.label_key()))
            .chain(EstimateUnit::ALL.iter().map(|v| v.label_key()))
            .chain(FieldKind::ALL.iter().map(|v| v.label_key()))
            .chain(EpicKind::ALL.iter().map(|v| v.label_key()))
            .chain(EpicStatus::ALL.iter().map(|v| v.label_key()))
            .chain(TemplateKind::ALL.iter().map(|v| v.label_key()))
            .chain(RelationKind::ALL.iter().map(|v| v.label_key()))
            .chain(TimeSource::ALL.iter().map(|v| v.label_key()));
        for key in keys {
            for lang in Lang::ALL {
                assert!(has(lang, key), "{lang}: falta {key}");
            }
        }
    }

    #[test]
    fn parse_errors_are_localized() {
        let err = "rapidísimo".parse::<Priority>().unwrap_err();
        assert!(
            err.localize(Lang::Es)
                .starts_with("Prioridad inválida `rapidísimo`")
        );
        assert!(
            err.localize(Lang::En)
                .starts_with("Invalid priority `rapidísimo`")
        );
    }
}
