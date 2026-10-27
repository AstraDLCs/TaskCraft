use foundation::CorrelationId;
use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{ActivityId, Actor, CommentId, ProjectId, TaskId, Timestamp};

/// Comentario o nota de avance en una tarea (RF-HIS-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    pub task_id: TaskId,
    pub author: Actor,
    /// Markdown (fuente).
    pub body: String,
    pub created_at: Timestamp,
}

/// Entrada inmutable del historial (RF-HIS-01): quién cambió qué, cuándo y
/// con qué comando.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: ActivityId,
    pub at: Timestamp,
    pub actor: Actor,
    pub correlation: CorrelationId,
    pub command: String,
    pub action: Action,
    pub entity: EntityKind,
    pub entity_id: String,
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
    pub changes: Vec<FieldChange>,
}

/// Valor de un campo antes y después del cambio (`null` = no existía).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

text_enum! {
    pub enum Action ("error-parse-action", "action") {
        Created = "created",
        Updated = "updated",
        Deleted = "deleted",
        Linked = "linked",
        Unlinked = "unlinked",
    }
}

text_enum! {
    pub enum EntityKind ("error-parse-entity", "entity") {
        Workspace = "workspace",
        Project = "project",
        Task = "task",
        Label = "label",
        Dependency = "dependency",
        Comment = "comment",
        Agent = "agent",
        Inbox = "inbox",
        Epic = "epic",
        Template = "template",
        Attachment = "attachment",
        Relation = "relation",
        Time = "time",
        View = "view",
        Automation = "automation",
        Webhook = "webhook",
        Interface = "interface",
        Cost = "cost",
        Git = "git",
        Cycle = "cycle",
        Diagram = "diagram",
    }
}
