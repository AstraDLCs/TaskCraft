use serde::{Deserialize, Serialize};

use crate::{Actor, AttachmentId, TaskId, Timestamp};

/// Archivo adjunto a una tarea (RF-MOD-17). El contenido se guarda aparte,
/// una vez por `sha256`, aunque lo adjunten varias tareas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: AttachmentId,
    pub task_id: TaskId,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: String,
    pub author: Actor,
    pub created_at: Timestamp,
}
