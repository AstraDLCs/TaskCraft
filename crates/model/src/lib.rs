//! L1 — Modelo: entidades del dominio. Solo datos: sin IO, sin reglas, sin UI.

mod activity;
mod actor;
mod agent;
mod attachment;
mod automation;
mod cost;
mod cycle;
mod diagram;
mod epic;
mod git;
mod ids;
mod inbox;
mod label;
mod project;
mod recurrence;
mod serde_str;
mod task;
mod template;
mod text_enum;
mod time;
mod transition;
mod view;
mod when;
mod workflow;
mod workspace;

pub use activity::{Action, ActivityEntry, Comment, EntityKind, FieldChange};
pub use actor::{Actor, AgentId, InterfaceId, ParseActorError};
pub use agent::{Agent, AgentLevel, AgentSession, AgentStatus, Interface};
pub use attachment::Attachment;
pub use automation::{Automation, Delivery, DeliveryStatus, Trigger, Webhook};
pub use cost::CostEntry;
pub use cycle::{Cycle, CycleState};
pub use diagram::{Diagram, DiagramKey, DiagramMention};
pub use epic::{Epic, EpicKind, EpicStatus};
pub use foundation::CorrelationId;
pub use git::{GitKind, GitLink};
pub use ids::{
    ActivityId, AttachmentId, AutomationId, CommentId, CostEntryId, CycleId, DeliveryId, DiagramId,
    EpicId, GitLinkId, InboxId, ItemId, LabelId, ProjectId, SessionId, TaskId, TemplateId,
    TimeEntryId, TransitionId, ViewId, WebhookId, WorkspaceId,
};
pub use inbox::{InboxItem, InboxKind, InboxStatus};
pub use jiff::Timestamp;
pub use jiff::civil::Date;
pub use label::Label;
pub use project::{Project, ProjectStatus};
pub use recurrence::{Recurrence, RepeatUnit};
pub use task::{
    Category, CheckItem, Claim, Dependency, Mention, Priority, Relation, RelationKind, Status,
    Task, TaskKey,
};
pub use template::{Template, TemplateKind};
pub use time::{TimeEntry, TimeSource};
pub use transition::Transition;
pub use view::SavedView;
pub use when::When;
pub use workflow::{Estimate, EstimateUnit, FieldDef, FieldKind, StatusDef};
pub use workspace::{Settings, WorkspaceInfo};

/// Error al interpretar el texto de un enum del modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    /// Clave i18n del mensaje (`error-parse-status`).
    pub key: &'static str,
    pub value: String,
    pub expected: &'static [&'static str],
}

impl i18n::Localize for ParseEnumError {
    fn msg(&self) -> i18n::Msg {
        i18n::Msg::new(self.key)
            .arg("value", &self.value)
            .arg("expected", self.expected.join(", "))
    }
}

impl std::fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use i18n::Localize;
        self.msg().fmt(f)
    }
}

impl std::error::Error for ParseEnumError {}
