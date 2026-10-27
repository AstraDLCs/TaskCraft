//! Conversión fila SQL ↔ modelo, igual que en `store-sqlite`: los valores se
//! guardan como texto canónico (`Display`) y se leen con `FromStr`.

use std::fmt::Display;
use std::str::FromStr;

use crate::pg::Row;
use model::{CheckItem, LabelId, Project, Settings, Task, WorkspaceInfo};
use store::{StoreError, StoreResult};

pub(crate) fn parse<T>(field: &str, text: &str) -> StoreResult<T>
where
    T: FromStr,
    T::Err: Display,
{
    text.parse()
        .map_err(|e| StoreError::Corrupt(format!("{field} = `{text}`: {e}")))
}

fn parse_opt<T>(field: &str, text: Option<String>) -> StoreResult<Option<T>>
where
    T: FromStr,
    T::Err: Display,
{
    text.map(|t| parse(field, &t)).transpose()
}

/// Columna con JSON (listas y mapas pequeños del modelo).
pub(crate) fn json<T: serde::de::DeserializeOwned>(field: &str, text: &str) -> StoreResult<T> {
    serde_json::from_str(text).map_err(|e| StoreError::Corrupt(format!("{field}: {e}")))
}

pub(crate) fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

pub(crate) fn text<T: ToString>(value: &Option<T>) -> Option<String> {
    value.as_ref().map(ToString::to_string)
}

pub(crate) struct WorkspaceRow {
    id: String,
    name: String,
    created_at: String,
    max_depth: u32,
    trash_days: u32,
    lease_minutes: u32,
    agent_wip_limit: u32,
    auto_archive_days: u32,
    max_attachment_mb: u32,
    read_only: bool,
    backup_hours: u32,
    backup_keep: u32,
    stale_weeks: u32,
    urgent_days: u32,
}

impl WorkspaceRow {
    pub const SELECT: &str = "SELECT id, name, created_at, max_depth, trash_days, lease_minutes, \
                              agent_wip_limit, auto_archive_days, max_attachment_mb, \
                              read_only, backup_hours, backup_keep, stale_weeks, \
                              urgent_days FROM workspace";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            created_at: r.get("created_at")?,
            max_depth: r.get("max_depth")?,
            trash_days: r.get("trash_days")?,
            lease_minutes: r.get("lease_minutes")?,
            agent_wip_limit: r.get("agent_wip_limit")?,
            auto_archive_days: r.get("auto_archive_days")?,
            max_attachment_mb: r.get("max_attachment_mb")?,
            read_only: r.get("read_only")?,
            backup_hours: r.get("backup_hours")?,
            backup_keep: r.get("backup_keep")?,
            stale_weeks: r.get("stale_weeks")?,
            urgent_days: r.get("urgent_days")?,
        })
    }

    pub fn into_model(self) -> StoreResult<WorkspaceInfo> {
        Ok(WorkspaceInfo {
            id: parse("workspace.id", &self.id)?,
            name: self.name,
            created_at: parse("workspace.created_at", &self.created_at)?,
            settings: Settings {
                max_depth: self.max_depth,
                trash_days: self.trash_days,
                lease_minutes: self.lease_minutes,
                agent_wip_limit: self.agent_wip_limit,
                auto_archive_days: self.auto_archive_days,
                max_attachment_mb: self.max_attachment_mb,
                read_only: self.read_only,
                backup_hours: self.backup_hours,
                backup_keep: self.backup_keep,
                stale_weeks: self.stale_weeks,
                urgent_days: self.urgent_days,
            },
        })
    }
}

pub(crate) struct ProjectRow {
    id: String,
    key: String,
    name: String,
    description: String,
    status: String,
    next_number: u64,
    next_diagram: u64,
    created_at: String,
    updated_at: String,
    locked: bool,
    workflow: String,
    estimate_unit: String,
    fields: String,
}

impl ProjectRow {
    pub const SELECT: &str = "SELECT id, key, name, description, status, \
                              next_number, next_diagram, created_at, updated_at, locked, \
                              workflow, estimate_unit, fields FROM project";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            key: r.get("key")?,
            name: r.get("name")?,
            description: r.get("description")?,
            status: r.get("status")?,
            next_number: r.get("next_number")?,
            next_diagram: r.get("next_diagram")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            locked: r.get("locked")?,
            workflow: r.get("workflow")?,
            estimate_unit: r.get("estimate_unit")?,
            fields: r.get("fields")?,
        })
    }

    pub fn into_model(self) -> StoreResult<Project> {
        Ok(Project {
            id: parse("project.id", &self.id)?,
            key: self.key,
            name: self.name,
            description: self.description,
            status: parse("project.status", &self.status)?,
            next_number: self.next_number,
            next_diagram: self.next_diagram,
            created_at: parse("project.created_at", &self.created_at)?,
            updated_at: parse("project.updated_at", &self.updated_at)?,
            locked: self.locked,
            workflow: json("project.workflow", &self.workflow)?,
            estimate_unit: parse("project.estimate_unit", &self.estimate_unit)?,
            fields: json("project.fields", &self.fields)?,
        })
    }
}

pub(crate) struct TaskRow {
    pub id: String,
    project_id: String,
    number: u64,
    parent_id: Option<String>,
    title: String,
    description: String,
    status: String,
    priority: String,
    start_at: Option<String>,
    due_at: Option<String>,
    creator: String,
    revision: u64,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
    deleted_at: Option<String>,
    claimed_by: Option<String>,
    claimed_at: Option<String>,
    lease_until: Option<String>,
    claim_revision: Option<u64>,
    needs_human: Option<String>,
    locked: bool,
    stage: Option<String>,
    estimate: Option<u32>,
    epic_id: Option<String>,
    milestone_id: Option<String>,
    fields: String,
    assignees: String,
    recurrence: Option<String>,
    cycles: String,
}

/// Partes de la tarea guardadas en tablas hijas.
#[derive(Default)]
pub(crate) struct TaskParts {
    pub checklist: Vec<CheckItem>,
    pub criteria: Vec<CheckItem>,
    pub labels: Vec<LabelId>,
}

impl TaskRow {
    pub const SELECT: &str = "SELECT id, project_id, number, parent_id, title, description, \
                              status, priority, start_at, due_at, creator, revision, \
                              created_at, updated_at, archived_at, deleted_at, claimed_by, \
                              claimed_at, lease_until, claim_revision, needs_human, locked, \
                              stage, estimate, epic_id, milestone_id, fields, assignees, \
                              recurrence, cycles FROM task";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            project_id: r.get("project_id")?,
            number: r.get("number")?,
            parent_id: r.get("parent_id")?,
            title: r.get("title")?,
            description: r.get("description")?,
            status: r.get("status")?,
            priority: r.get("priority")?,
            start_at: r.get("start_at")?,
            due_at: r.get("due_at")?,
            creator: r.get("creator")?,
            revision: r.get("revision")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            archived_at: r.get("archived_at")?,
            deleted_at: r.get("deleted_at")?,
            claimed_by: r.get("claimed_by")?,
            claimed_at: r.get("claimed_at")?,
            lease_until: r.get("lease_until")?,
            claim_revision: r.get("claim_revision")?,
            needs_human: r.get("needs_human")?,
            locked: r.get("locked")?,
            stage: r.get("stage")?,
            estimate: r.get("estimate")?,
            epic_id: r.get("epic_id")?,
            milestone_id: r.get("milestone_id")?,
            fields: r.get("fields")?,
            assignees: r.get("assignees")?,
            recurrence: r.get("recurrence")?,
            cycles: r.get("cycles")?,
        })
    }

    pub fn into_model(self, parts: TaskParts) -> StoreResult<Task> {
        let claim = match (
            self.claimed_by,
            self.claimed_at,
            self.lease_until,
            self.claim_revision,
        ) {
            (Some(agent), Some(since), Some(until), Some(revision)) => Some(model::Claim {
                agent: parse("task.claimed_by", &agent)?,
                since: parse("task.claimed_at", &since)?,
                until: parse("task.lease_until", &until)?,
                revision,
            }),
            (None, None, None, None) => None,
            _ => {
                return Err(StoreError::Corrupt(format!(
                    "task {}: partial claim",
                    self.id
                )));
            }
        };
        Ok(Task {
            id: parse("task.id", &self.id)?,
            project_id: parse("task.project_id", &self.project_id)?,
            number: self.number,
            parent_id: parse_opt("task.parent_id", self.parent_id)?,
            title: self.title,
            description: self.description,
            status: parse("task.status", &self.status)?,
            priority: parse("task.priority", &self.priority)?,
            start: parse_opt("task.start_at", self.start_at)?,
            due: parse_opt("task.due_at", self.due_at)?,
            labels: parts.labels,
            assignees: json("task.assignees", &self.assignees)?,
            creator: parse("task.creator", &self.creator)?,
            checklist: parts.checklist,
            criteria: parts.criteria,
            revision: self.revision,
            created_at: parse("task.created_at", &self.created_at)?,
            updated_at: parse("task.updated_at", &self.updated_at)?,
            archived_at: parse_opt("task.archived_at", self.archived_at)?,
            deleted_at: parse_opt("task.deleted_at", self.deleted_at)?,
            claim,
            needs_human: self.needs_human,
            locked: self.locked,
            stage: self.stage,
            estimate: self.estimate.map(model::Estimate),
            epic_id: parse_opt("task.epic_id", self.epic_id)?,
            milestone_id: parse_opt("task.milestone_id", self.milestone_id)?,
            fields: json("task.fields", &self.fields)?,
            recurrence: self
                .recurrence
                .map(|r| json("task.recurrence", &r))
                .transpose()?,
            cycles: json("task.cycles", &self.cycles)?,
        })
    }
}

pub(crate) struct AgentRow {
    id: String,
    name: String,
    model: String,
    description: String,
    color: String,
    status: String,
    created_at: String,
    updated_at: String,
    interface_id: Option<String>,
    level: String,
    scope: String,
    capabilities: String,
    paused: bool,
}

impl AgentRow {
    pub const SELECT: &str = "SELECT id, name, model, description, color, status, created_at, \
                              updated_at, interface_id, level, scope, capabilities, paused FROM agent";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            model: r.get("model")?,
            description: r.get("description")?,
            color: r.get("color")?,
            status: r.get("status")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            interface_id: r.get("interface_id")?,
            level: r.get("level")?,
            scope: r.get("scope")?,
            capabilities: r.get("capabilities")?,
            paused: r.get("paused")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Agent> {
        Ok(model::Agent {
            id: parse("agent.id", &self.id)?,
            name: self.name,
            model: self.model,
            description: self.description,
            color: self.color,
            status: parse("agent.status", &self.status)?,
            created_at: parse("agent.created_at", &self.created_at)?,
            updated_at: parse("agent.updated_at", &self.updated_at)?,
            interface_id: parse_opt("agent.interface_id", self.interface_id)?,
            level: parse("agent.level", &self.level)?,
            scope: json("agent.scope", &self.scope)?,
            capabilities: json("agent.capabilities", &self.capabilities)?,
            paused: self.paused,
        })
    }
}

pub(crate) struct SessionRow {
    id: String,
    agent_id: String,
    transport: String,
    started_at: String,
    last_seen: String,
    ended_at: Option<String>,
}

impl SessionRow {
    pub const SELECT: &str =
        "SELECT id, agent_id, transport, started_at, last_seen, ended_at FROM agent_session";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            agent_id: r.get("agent_id")?,
            transport: r.get("transport")?,
            started_at: r.get("started_at")?,
            last_seen: r.get("last_seen")?,
            ended_at: r.get("ended_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::AgentSession> {
        Ok(model::AgentSession {
            id: parse("agent_session.id", &self.id)?,
            agent_id: parse("agent_session.agent_id", &self.agent_id)?,
            transport: self.transport,
            started_at: parse("agent_session.started_at", &self.started_at)?,
            last_seen: parse("agent_session.last_seen", &self.last_seen)?,
            ended_at: parse_opt("agent_session.ended_at", self.ended_at)?,
        })
    }
}

pub(crate) struct CommentRow {
    id: String,
    task_id: String,
    author: String,
    body: String,
    created_at: String,
}

impl CommentRow {
    pub const SELECT: &str = "SELECT id, task_id, author, body, created_at FROM comment";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            task_id: r.get("task_id")?,
            author: r.get("author")?,
            body: r.get("body")?,
            created_at: r.get("created_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Comment> {
        Ok(model::Comment {
            id: parse("comment.id", &self.id)?,
            task_id: parse("comment.task_id", &self.task_id)?,
            author: parse("comment.author", &self.author)?,
            body: self.body,
            created_at: parse("comment.created_at", &self.created_at)?,
        })
    }
}

pub(crate) struct ActivityRow {
    id: String,
    at: String,
    actor: String,
    correlation: String,
    command: String,
    action: String,
    entity_kind: String,
    entity_id: String,
    project_id: Option<String>,
    task_id: Option<String>,
    changes: String,
}

impl ActivityRow {
    pub const SELECT: &str = "SELECT id, at, actor, correlation, command, action, entity_kind, \
                              entity_id, project_id, task_id, changes FROM activity";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            at: r.get("at")?,
            actor: r.get("actor")?,
            correlation: r.get("correlation")?,
            command: r.get("command")?,
            action: r.get("action")?,
            entity_kind: r.get("entity_kind")?,
            entity_id: r.get("entity_id")?,
            project_id: r.get("project_id")?,
            task_id: r.get("task_id")?,
            changes: r.get("changes")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::ActivityEntry> {
        Ok(model::ActivityEntry {
            id: parse("activity.id", &self.id)?,
            at: parse("activity.at", &self.at)?,
            actor: parse("activity.actor", &self.actor)?,
            correlation: parse("activity.correlation", &self.correlation)?,
            command: self.command,
            action: parse("activity.action", &self.action)?,
            entity: parse("activity.entity_kind", &self.entity_kind)?,
            entity_id: self.entity_id,
            project_id: parse_opt("activity.project_id", self.project_id)?,
            task_id: parse_opt("activity.task_id", self.task_id)?,
            changes: serde_json::from_str(&self.changes)
                .map_err(|e| StoreError::Corrupt(format!("activity.changes: {e}")))?,
        })
    }
}

pub(crate) struct InboxRow {
    id: String,
    kind: String,
    title: String,
    body: String,
    author: String,
    status: String,
    task_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl InboxRow {
    pub const SELECT: &str = "SELECT id, kind, title, body, author, status, task_id, created_at, \
                              updated_at FROM inbox_item";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            kind: r.get("kind")?,
            title: r.get("title")?,
            body: r.get("body")?,
            author: r.get("author")?,
            status: r.get("status")?,
            task_id: r.get("task_id")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::InboxItem> {
        Ok(model::InboxItem {
            id: parse("inbox_item.id", &self.id)?,
            kind: parse("inbox_item.kind", &self.kind)?,
            title: self.title,
            body: self.body,
            author: parse("inbox_item.author", &self.author)?,
            status: parse("inbox_item.status", &self.status)?,
            task_id: parse_opt("inbox_item.task_id", self.task_id)?,
            created_at: parse("inbox_item.created_at", &self.created_at)?,
            updated_at: parse("inbox_item.updated_at", &self.updated_at)?,
        })
    }
}

pub(crate) struct EpicRow {
    id: String,
    project_id: String,
    kind: String,
    name: String,
    description: String,
    target: Option<String>,
    status: String,
    locked: bool,
    created_at: String,
    updated_at: String,
}

impl EpicRow {
    pub const SELECT: &str = "SELECT id, project_id, kind, name, description, target, status, \
                              locked, created_at, updated_at FROM epic";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            project_id: r.get("project_id")?,
            kind: r.get("kind")?,
            name: r.get("name")?,
            description: r.get("description")?,
            target: r.get("target")?,
            status: r.get("status")?,
            locked: r.get("locked")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Epic> {
        Ok(model::Epic {
            id: parse("epic.id", &self.id)?,
            project_id: parse("epic.project_id", &self.project_id)?,
            kind: parse("epic.kind", &self.kind)?,
            name: self.name,
            description: self.description,
            target: parse_opt("epic.target", self.target)?,
            status: parse("epic.status", &self.status)?,
            locked: self.locked,
            created_at: parse("epic.created_at", &self.created_at)?,
            updated_at: parse("epic.updated_at", &self.updated_at)?,
        })
    }
}

pub(crate) struct CycleRow {
    id: String,
    project_id: String,
    name: String,
    start_day: String,
    end_day: String,
    closed_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl CycleRow {
    pub const SELECT: &str = "SELECT id, project_id, name, start_day, end_day, closed_at, \
                              created_at, updated_at FROM cycle";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            project_id: r.get("project_id")?,
            name: r.get("name")?,
            start_day: r.get("start_day")?,
            end_day: r.get("end_day")?,
            closed_at: r.get("closed_at")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Cycle> {
        Ok(model::Cycle {
            id: parse("cycle.id", &self.id)?,
            project_id: parse("cycle.project_id", &self.project_id)?,
            name: self.name,
            start: parse("cycle.start_day", &self.start_day)?,
            end: parse("cycle.end_day", &self.end_day)?,
            closed_at: parse_opt("cycle.closed_at", self.closed_at)?,
            created_at: parse("cycle.created_at", &self.created_at)?,
            updated_at: parse("cycle.updated_at", &self.updated_at)?,
        })
    }
}

pub(crate) struct DiagramRow {
    id: String,
    project_id: String,
    number: u64,
    title: String,
    description: String,
    source: String,
    created_at: String,
    updated_at: String,
}

impl DiagramRow {
    pub const SELECT: &str = "SELECT id, project_id, number, title, description, source, \
                              created_at, updated_at FROM diagram";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            project_id: r.get("project_id")?,
            number: r.get("number")?,
            title: r.get("title")?,
            description: r.get("description")?,
            source: r.get("source")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Diagram> {
        Ok(model::Diagram {
            id: parse("diagram.id", &self.id)?,
            project_id: parse("diagram.project_id", &self.project_id)?,
            number: self.number,
            title: self.title,
            description: self.description,
            source: self.source,
            created_at: parse("diagram.created_at", &self.created_at)?,
            updated_at: parse("diagram.updated_at", &self.updated_at)?,
        })
    }
}

pub(crate) struct TemplateRow {
    id: String,
    name: String,
    kind: String,
    description: String,
    body: String,
    created_at: String,
    updated_at: String,
}

impl TemplateRow {
    pub const SELECT: &str =
        "SELECT id, name, kind, description, body, created_at, updated_at FROM template";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            kind: r.get("kind")?,
            description: r.get("description")?,
            body: r.get("body")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Template> {
        Ok(model::Template {
            id: parse("template.id", &self.id)?,
            name: self.name,
            kind: parse("template.kind", &self.kind)?,
            description: self.description,
            body: self.body,
            created_at: parse("template.created_at", &self.created_at)?,
            updated_at: parse("template.updated_at", &self.updated_at)?,
        })
    }
}

pub(crate) struct AttachmentRow {
    id: String,
    task_id: String,
    name: String,
    mime: String,
    size: u64,
    sha256: String,
    author: String,
    created_at: String,
}

impl AttachmentRow {
    pub const SELECT: &str =
        "SELECT id, task_id, name, mime, size, sha256, author, created_at FROM attachment";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            task_id: r.get("task_id")?,
            name: r.get("name")?,
            mime: r.get("mime")?,
            size: r.get("size")?,
            sha256: r.get("sha256")?,
            author: r.get("author")?,
            created_at: r.get("created_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Attachment> {
        Ok(model::Attachment {
            id: parse("attachment.id", &self.id)?,
            task_id: parse("attachment.task_id", &self.task_id)?,
            name: self.name,
            mime: self.mime,
            size: self.size,
            sha256: self.sha256,
            author: parse("attachment.author", &self.author)?,
            created_at: parse("attachment.created_at", &self.created_at)?,
        })
    }
}

pub(crate) struct TimeRow {
    id: String,
    task_id: String,
    actor: String,
    source: String,
    started_at: String,
    minutes: u32,
    running: bool,
    note: String,
    created_at: String,
}

impl TimeRow {
    pub const SELECT: &str = "SELECT id, task_id, actor, source, started_at, minutes, running, \
                              note, created_at FROM time_entry";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            task_id: r.get("task_id")?,
            actor: r.get("actor")?,
            source: r.get("source")?,
            started_at: r.get("started_at")?,
            minutes: r.get("minutes")?,
            running: r.get("running")?,
            note: r.get("note")?,
            created_at: r.get("created_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::TimeEntry> {
        Ok(model::TimeEntry {
            id: parse("time_entry.id", &self.id)?,
            task_id: parse("time_entry.task_id", &self.task_id)?,
            actor: parse("time_entry.actor", &self.actor)?,
            source: parse("time_entry.source", &self.source)?,
            started_at: parse("time_entry.started_at", &self.started_at)?,
            minutes: self.minutes,
            running: self.running,
            note: self.note,
            created_at: parse("time_entry.created_at", &self.created_at)?,
        })
    }
}

pub(crate) struct CostRow {
    id: String,
    task_id: String,
    actor: String,
    model: String,
    tokens_in: u64,
    tokens_out: u64,
    amount_micros: u64,
    currency: String,
    note: String,
    at: String,
}

impl CostRow {
    pub const SELECT: &str = "SELECT id, task_id, actor, model, tokens_in, tokens_out, \
                              amount_micros, currency, note, at FROM cost_entry";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            task_id: r.get("task_id")?,
            actor: r.get("actor")?,
            model: r.get("model")?,
            tokens_in: r.get("tokens_in")?,
            tokens_out: r.get("tokens_out")?,
            amount_micros: r.get("amount_micros")?,
            currency: r.get("currency")?,
            note: r.get("note")?,
            at: r.get("at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::CostEntry> {
        Ok(model::CostEntry {
            id: parse("cost_entry.id", &self.id)?,
            task_id: parse("cost_entry.task_id", &self.task_id)?,
            actor: parse("cost_entry.actor", &self.actor)?,
            model: self.model,
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            amount_micros: self.amount_micros,
            currency: self.currency,
            note: self.note,
            at: parse("cost_entry.at", &self.at)?,
        })
    }
}

pub(crate) struct GitRow {
    id: String,
    task_id: String,
    kind: String,
    reference: String,
    title: String,
    url: Option<String>,
    actor: String,
    at: String,
}

impl GitRow {
    pub const SELECT: &str =
        "SELECT id, task_id, kind, reference, title, url, actor, at FROM git_link";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            task_id: r.get("task_id")?,
            kind: r.get("kind")?,
            reference: r.get("reference")?,
            title: r.get("title")?,
            url: r.get("url")?,
            actor: r.get("actor")?,
            at: r.get("at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::GitLink> {
        Ok(model::GitLink {
            id: parse("git_link.id", &self.id)?,
            task_id: parse("git_link.task_id", &self.task_id)?,
            kind: parse("git_link.kind", &self.kind)?,
            reference: self.reference,
            title: self.title,
            url: self.url,
            actor: parse("git_link.actor", &self.actor)?,
            at: parse("git_link.at", &self.at)?,
        })
    }
}

pub(crate) struct InterfaceRow {
    id: String,
    name: String,
    kind: String,
    description: String,
    color: String,
    status: String,
    level: String,
    scope: String,
    max_agents: u32,
    paused: bool,
    token_hash: Option<String>,
    token: Option<String>,
    created_at: String,
    updated_at: String,
}

impl InterfaceRow {
    pub const SELECT: &str = "SELECT id, name, kind, description, color, status, level, scope, \
                              max_agents, paused, token_hash, token, created_at, updated_at \
                              FROM interface";

    pub fn read(r: &Row<'_>) -> crate::pg::Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            kind: r.get("kind")?,
            description: r.get("description")?,
            color: r.get("color")?,
            status: r.get("status")?,
            level: r.get("level")?,
            scope: r.get("scope")?,
            max_agents: r.get("max_agents")?,
            paused: r.get("paused")?,
            token_hash: r.get("token_hash")?,
            token: r.get("token")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
        })
    }

    pub fn into_model(self) -> StoreResult<model::Interface> {
        Ok(model::Interface {
            id: parse("interface.id", &self.id)?,
            name: self.name,
            kind: self.kind,
            description: self.description,
            color: self.color,
            status: parse("interface.status", &self.status)?,
            level: parse("interface.level", &self.level)?,
            scope: json("interface.scope", &self.scope)?,
            max_agents: self.max_agents,
            paused: self.paused,
            token_hash: self.token_hash,
            token: self.token,
            created_at: parse("interface.created_at", &self.created_at)?,
            updated_at: parse("interface.updated_at", &self.updated_at)?,
        })
    }
}
