use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{Actor, InboxId, TaskId, Timestamp};

/// Hallazgo que un agente deja sin proyecto para el triaje humano (RF-COO-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: InboxId,
    pub kind: InboxKind,
    pub title: String,
    /// Markdown (fuente).
    pub body: String,
    pub author: Actor,
    pub status: InboxStatus,
    /// Tarea creada al convertirlo.
    pub task_id: Option<TaskId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

text_enum! {
    pub enum InboxKind ("error-parse-inbox-kind", "inbox-kind") {
        Idea = "idea",
        Bug = "bug",
        Debt = "debt" | "deuda",
    }
}

text_enum! {
    pub enum InboxStatus ("error-parse-inbox-status", "inbox-status") {
        Open = "open" | "abierto",
        Converted = "converted" | "convertido",
        Discarded = "discarded" | "descartado",
    }
}
