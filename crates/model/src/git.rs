use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{Actor, GitLinkId, TaskId, Timestamp};

/// Rama, commit o PR vinculado a una tarea (RF-INT-04, D16). La terna
/// (tarea, tipo, referencia) es única.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLink {
    pub id: GitLinkId,
    pub task_id: TaskId,
    pub kind: GitKind,
    /// Nombre de la rama, SHA completo del commit o URL del PR.
    pub reference: String,
    /// Asunto del commit o título del PR.
    pub title: String,
    pub url: Option<String>,
    pub actor: Actor,
    pub at: Timestamp,
}

text_enum! {
    pub enum GitKind ("error-parse-git-kind", "git-kind") {
        Branch = "branch" | "rama",
        Commit = "commit",
        Pr = "pr" | "pull" | "pull_request" | "mr" | "merge_request",
    }
}
