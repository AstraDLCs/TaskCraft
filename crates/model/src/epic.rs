use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{EpicId, ProjectId, Timestamp, When};

/// Épica o hito (RF-MOD-13): agrupa tareas de un proyecto con una fecha
/// objetivo; su progreso se calcula a partir de sus tareas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epic {
    pub id: EpicId,
    pub project_id: ProjectId,
    pub kind: EpicKind,
    pub name: String,
    /// Markdown (fuente).
    pub description: String,
    pub target: Option<When>,
    pub status: EpicStatus,
    /// Bloqueo humano de todas sus tareas (RF-SUP-05).
    pub locked: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

text_enum! {
    pub enum EpicKind ("error-parse-epic-kind", "epic-kind") {
        Epic = "epic" | "epica" | "épica",
        Milestone = "milestone" | "hito",
    }
}

text_enum! {
    pub enum EpicStatus ("error-parse-epic-status", "epic-status") {
        Open = "open" | "abierta" | "abierto",
        Closed = "closed" | "cerrada" | "cerrado",
    }
}
