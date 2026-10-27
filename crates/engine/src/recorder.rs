//! Registro automático de actividad (RF-HIS-01).
//!
//! Los comandos escriben a través de un [`Recorder`], que delega en la
//! transacción real y anota qué cambió en cada escritura (valores antes y
//! después). Así ningún comando puede olvidarse de dejar rastro, y el engine
//! guarda esas notas en el historial dentro de la misma transacción.

use model::{
    Action, ActivityEntry, ActivityId, Agent, AgentId, AgentSession, Comment, Dependency,
    EntityKind, FieldChange, InboxId, InboxItem, InboxStatus, Label, LabelId, Project, ProjectId,
    SessionId, Status, Task, TaskId, Timestamp, Transition, WorkspaceInfo,
};
use serde::Serialize;
use serde_json::Value;
use store::{ActivityFilter, Reader, StoreResult, TaskFilter, Writer};

use crate::Context;

/// Campos que cambian en cada escritura y no aportan información.
const IGNORED: &[&str] = &["id", "revision", "updated_at"];

/// Un cambio pendiente de pasar al historial.
pub(crate) struct Change {
    action: Action,
    entity: EntityKind,
    entity_id: String,
    project_id: Option<ProjectId>,
    task_id: Option<TaskId>,
    changes: Vec<FieldChange>,
}

impl Change {
    pub(crate) fn into_entry(self, ctx: &Context, command: &str, at: Timestamp) -> ActivityEntry {
        ActivityEntry {
            id: ActivityId::generate(),
            at,
            actor: ctx.actor.clone(),
            correlation: ctx.correlation,
            command: command.to_owned(),
            action: self.action,
            entity: self.entity,
            entity_id: self.entity_id,
            project_id: self.project_id,
            task_id: self.task_id,
            changes: self.changes,
        }
    }
}

/// Un paso del recorrido de una tarea pendiente de guardar (D24): el engine
/// le pone quién y cuándo, igual que al historial.
pub(crate) struct Step {
    task: TaskId,
    project: ProjectId,
    from: Option<(Status, Option<String>)>,
    to: (Status, Option<String>),
}

impl Step {
    pub(crate) fn into_transition(self, ctx: &Context, at: Timestamp) -> Transition {
        Transition::new(
            self.task,
            self.project,
            self.from,
            self.to,
            ctx.actor.clone(),
            at,
        )
    }
}

pub(crate) struct Recorder<'a> {
    inner: &'a mut dyn Writer,
    changes: Vec<Change>,
    steps: Vec<Step>,
}

impl<'a> Recorder<'a> {
    pub(crate) fn new(inner: &'a mut dyn Writer) -> Self {
        Self {
            inner,
            changes: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Lo que va al historial y los pasos del recorrido de las tareas.
    pub(crate) fn into_parts(self) -> (Vec<Change>, Vec<Step>) {
        (self.changes, self.steps)
    }

    /// Tareas que lo registrado hasta ahora pasó a hecho (D17).
    pub(crate) fn completed_tasks(&self) -> Vec<TaskId> {
        let mut out = Vec::new();
        for c in &self.changes {
            if let Some(task) = c.task_id
                && c.entity == EntityKind::Task
                && c.changes
                    .iter()
                    .any(|f| f.field == "status" && f.after == serde_json::json!("done"))
                && !out.contains(&task)
            {
                out.push(task);
            }
        }
        out
    }

    fn push(&mut self, change: Change) {
        // Una escritura que no cambia nada visible no deja rastro.
        if change.action != Action::Updated || !change.changes.is_empty() {
            self.changes.push(change);
        }
    }
}

fn fields<T: Serialize>(value: &T) -> serde_json::Map<String, Value> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}

/// Campos distintos entre `before` y `after` (`None` = no existía).
fn diff<T: Serialize>(before: Option<&T>, after: &T) -> Vec<FieldChange> {
    let old = before.map(fields).unwrap_or_default();
    let new = fields(after);
    new.into_iter()
        .filter(|(field, _)| !IGNORED.contains(&field.as_str()))
        .filter_map(|(field, after)| {
            let before = old.get(&field).cloned().unwrap_or(Value::Null);
            (before != after).then_some(FieldChange {
                field,
                before,
                after,
            })
        })
        .collect()
}

fn task_change(action: Action, task: &Task, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Task,
        entity_id: task.id.to_string(),
        project_id: Some(task.project_id),
        task_id: Some(task.id),
        changes,
    }
}

fn dependency_change(action: Action, dep: Dependency, project: Option<ProjectId>) -> Change {
    Change {
        action,
        entity: EntityKind::Dependency,
        entity_id: format!("{}>{}", dep.blocker, dep.blocked),
        project_id: project,
        task_id: Some(dep.blocked),
        changes: vec![FieldChange {
            field: "blocker".into(),
            before: Value::Null,
            after: Value::String(dep.blocker.to_string()),
        }],
    }
}

impl Reader for Recorder<'_> {
    fn workspace(&self) -> StoreResult<WorkspaceInfo> {
        self.inner.workspace()
    }
    fn project(&self, id: ProjectId) -> StoreResult<Option<Project>> {
        self.inner.project(id)
    }
    fn project_by_key(&self, key: &str) -> StoreResult<Option<Project>> {
        self.inner.project_by_key(key)
    }
    fn projects(&self, include_archived: bool) -> StoreResult<Vec<Project>> {
        self.inner.projects(include_archived)
    }
    fn task(&self, id: TaskId) -> StoreResult<Option<Task>> {
        self.inner.task(id)
    }
    fn task_by_number(&self, project: ProjectId, number: u64) -> StoreResult<Option<Task>> {
        self.inner.task_by_number(project, number)
    }
    fn tasks(&self, filter: &TaskFilter) -> StoreResult<Vec<Task>> {
        self.inner.tasks(filter)
    }
    fn children(&self, parent: TaskId) -> StoreResult<Vec<Task>> {
        self.inner.children(parent)
    }
    fn label(&self, id: LabelId) -> StoreResult<Option<Label>> {
        self.inner.label(id)
    }
    fn label_by_name(&self, name: &str) -> StoreResult<Option<Label>> {
        self.inner.label_by_name(name)
    }
    fn labels(&self) -> StoreResult<Vec<Label>> {
        self.inner.labels()
    }
    fn dependencies(&self) -> StoreResult<Vec<Dependency>> {
        self.inner.dependencies()
    }
    fn blockers_of(&self, task: TaskId) -> StoreResult<Vec<TaskId>> {
        self.inner.blockers_of(task)
    }
    fn blocks(&self, task: TaskId) -> StoreResult<Vec<TaskId>> {
        self.inner.blocks(task)
    }
    fn agent(&self, id: AgentId) -> StoreResult<Option<Agent>> {
        self.inner.agent(id)
    }
    fn agent_by_name(&self, name: &str) -> StoreResult<Option<Agent>> {
        self.inner.agent_by_name(name)
    }
    fn children_summary(
        &self,
        closed: &[model::Status],
    ) -> StoreResult<std::collections::HashMap<TaskId, (usize, usize)>> {
        self.inner.children_summary(closed)
    }
    fn open_counts(
        &self,
        closed: &[model::Status],
    ) -> StoreResult<std::collections::HashMap<model::ProjectId, usize>> {
        self.inner.open_counts(closed)
    }
    fn blocked_tasks(
        &self,
        closed: &[model::Status],
    ) -> StoreResult<std::collections::HashSet<TaskId>> {
        self.inner.blocked_tasks(closed)
    }
    fn locked_subtrees(&self) -> StoreResult<std::collections::HashSet<TaskId>> {
        self.inner.locked_subtrees()
    }
    fn next_candidates(
        &self,
        filter: &store::NextFilter,
    ) -> StoreResult<Vec<store::NextCandidate>> {
        self.inner.next_candidates(filter)
    }
    fn idempotent(&self, actor: &model::Actor, key: &str) -> StoreResult<Option<(String, String)>> {
        self.inner.idempotent(actor, key)
    }
    fn comment_counts(&self) -> StoreResult<std::collections::HashMap<TaskId, usize>> {
        self.inner.comment_counts()
    }
    fn interface(&self, id: model::InterfaceId) -> StoreResult<Option<model::Interface>> {
        self.inner.interface(id)
    }
    fn interface_by_name(&self, name: &str) -> StoreResult<Option<model::Interface>> {
        self.inner.interface_by_name(name)
    }
    fn interface_by_token(&self, token_hash: &str) -> StoreResult<Option<model::Interface>> {
        self.inner.interface_by_token(token_hash)
    }
    fn interfaces(&self) -> StoreResult<Vec<model::Interface>> {
        self.inner.interfaces()
    }
    fn agents(&self) -> StoreResult<Vec<Agent>> {
        self.inner.agents()
    }
    fn search_tasks(
        &self,
        query: &str,
        project: Option<ProjectId>,
        limit: usize,
    ) -> StoreResult<Vec<TaskId>> {
        self.inner.search_tasks(query, project, limit)
    }
    fn session(&self, id: SessionId) -> StoreResult<Option<AgentSession>> {
        self.inner.session(id)
    }
    fn open_sessions(&self, agent: Option<AgentId>) -> StoreResult<Vec<AgentSession>> {
        self.inner.open_sessions(agent)
    }
    fn comments(&self, task: TaskId) -> StoreResult<Vec<Comment>> {
        self.inner.comments(task)
    }
    fn comment_count(&self, task: TaskId) -> StoreResult<usize> {
        self.inner.comment_count(task)
    }
    fn activity(&self, filter: &ActivityFilter) -> StoreResult<Vec<model::ActivityEntry>> {
        self.inner.activity(filter)
    }
    fn activity_entry(&self, id: model::ActivityId) -> StoreResult<Option<model::ActivityEntry>> {
        self.inner.activity_entry(id)
    }
    fn claimed_tasks(&self) -> StoreResult<Vec<Task>> {
        self.inner.claimed_tasks()
    }
    fn inbox_item(&self, id: InboxId) -> StoreResult<Option<InboxItem>> {
        self.inner.inbox_item(id)
    }
    fn inbox_items(&self, status: Option<InboxStatus>) -> StoreResult<Vec<InboxItem>> {
        self.inner.inbox_items(status)
    }
    fn epic(&self, id: model::EpicId) -> StoreResult<Option<model::Epic>> {
        self.inner.epic(id)
    }
    fn epics(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Epic>> {
        self.inner.epics(project)
    }
    fn template(&self, id: model::TemplateId) -> StoreResult<Option<model::Template>> {
        self.inner.template(id)
    }
    fn template_by_name(&self, name: &str) -> StoreResult<Option<model::Template>> {
        self.inner.template_by_name(name)
    }
    fn templates(&self) -> StoreResult<Vec<model::Template>> {
        self.inner.templates()
    }
    fn attachment(&self, id: model::AttachmentId) -> StoreResult<Option<model::Attachment>> {
        self.inner.attachment(id)
    }
    fn attachments(&self, task: TaskId) -> StoreResult<Vec<model::Attachment>> {
        self.inner.attachments(task)
    }
    fn attachment_data(&self, sha256: &str) -> StoreResult<Option<Vec<u8>>> {
        self.inner.attachment_data(sha256)
    }
    fn relations(&self, task: Option<TaskId>) -> StoreResult<Vec<model::Relation>> {
        self.inner.relations(task)
    }
    fn mentions_of(&self, target: TaskId) -> StoreResult<Vec<model::Mention>> {
        self.inner.mentions_of(target)
    }

    fn diagram_mentions_of(
        &self,
        target: model::DiagramId,
    ) -> StoreResult<Vec<model::DiagramMention>> {
        self.inner.diagram_mentions_of(target)
    }

    fn diagram_mentions_from(&self, source: TaskId) -> StoreResult<Vec<model::DiagramMention>> {
        self.inner.diagram_mentions_from(source)
    }
    fn time_entries(&self, filter: &store::TimeFilter) -> StoreResult<Vec<model::TimeEntry>> {
        self.inner.time_entries(filter)
    }

    fn transitions(&self, filter: &store::TransitionFilter) -> StoreResult<Vec<model::Transition>> {
        self.inner.transitions(filter)
    }
    fn tasks_without_transitions(&self) -> StoreResult<Vec<TaskId>> {
        self.inner.tasks_without_transitions()
    }
    fn cost_entries(&self, filter: &store::CostFilter) -> StoreResult<Vec<model::CostEntry>> {
        self.inner.cost_entries(filter)
    }

    fn git_links(&self, task: Option<TaskId>) -> StoreResult<Vec<model::GitLink>> {
        self.inner.git_links(task)
    }

    fn cycle(&self, id: model::CycleId) -> StoreResult<Option<model::Cycle>> {
        self.inner.cycle(id)
    }

    fn cycles(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Cycle>> {
        self.inner.cycles(project)
    }

    fn diagram(&self, id: model::DiagramId) -> StoreResult<Option<model::Diagram>> {
        self.inner.diagram(id)
    }

    fn diagram_by_number(
        &self,
        project: ProjectId,
        number: u64,
    ) -> StoreResult<Option<model::Diagram>> {
        self.inner.diagram_by_number(project, number)
    }

    fn diagrams(&self, project: Option<ProjectId>) -> StoreResult<Vec<model::Diagram>> {
        self.inner.diagrams(project)
    }
    fn views(&self) -> StoreResult<Vec<model::SavedView>> {
        self.inner.views()
    }
    fn today_marks(&self) -> StoreResult<std::collections::HashMap<TaskId, model::Date>> {
        self.inner.today_marks()
    }
    fn automations(&self) -> StoreResult<Vec<model::Automation>> {
        self.inner.automations()
    }
    fn webhooks(&self) -> StoreResult<Vec<model::Webhook>> {
        self.inner.webhooks()
    }
    fn deliveries_due(&self, now: Timestamp, limit: usize) -> StoreResult<Vec<model::Delivery>> {
        self.inner.deliveries_due(now, limit)
    }
    fn delivery(&self, id: model::DeliveryId) -> StoreResult<Option<model::Delivery>> {
        self.inner.delivery(id)
    }
    fn deliveries(
        &self,
        webhook: Option<model::WebhookId>,
        limit: usize,
    ) -> StoreResult<Vec<model::Delivery>> {
        self.inner.deliveries(webhook, limit)
    }
}

impl Writer for Recorder<'_> {
    fn update_workspace(&mut self, ws: &WorkspaceInfo) -> StoreResult<()> {
        let before = self.inner.workspace()?;
        self.inner.update_workspace(ws)?;
        self.push(Change {
            action: Action::Updated,
            entity: EntityKind::Workspace,
            entity_id: ws.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(Some(&before), ws),
        });
        Ok(())
    }

    fn insert_project(&mut self, p: &Project) -> StoreResult<()> {
        self.inner.insert_project(p)?;
        self.push(Change {
            action: Action::Created,
            entity: EntityKind::Project,
            entity_id: p.id.to_string(),
            project_id: Some(p.id),
            task_id: None,
            changes: diff(None, p),
        });
        Ok(())
    }

    fn update_project(&mut self, p: &Project) -> StoreResult<()> {
        let before = self.inner.project(p.id)?;
        self.inner.update_project(p)?;
        let mut changes = diff(before.as_ref(), p);
        // Los contadores de claves cambian con cada tarea o diagrama nuevo: no son
        // un cambio del proyecto.
        changes.retain(|c| c.field != "next_number" && c.field != "next_diagram");
        self.push(Change {
            action: Action::Updated,
            entity: EntityKind::Project,
            entity_id: p.id.to_string(),
            project_id: Some(p.id),
            task_id: None,
            changes,
        });
        Ok(())
    }

    fn insert_task(&mut self, t: &Task) -> StoreResult<()> {
        self.inner.insert_task(t)?;
        self.push(task_change(Action::Created, t, diff(None, t)));
        self.steps.push(Step {
            task: t.id,
            project: t.project_id,
            from: None,
            to: (t.status, t.stage.clone()),
        });
        Ok(())
    }

    fn update_task(&mut self, t: &Task) -> StoreResult<()> {
        let before = self.inner.task(t.id)?;
        self.inner.update_task(t)?;
        // Cambió de estado o de columna: un paso más del recorrido (D24).
        if let Some(b) = before
            .as_ref()
            .filter(|b| b.status != t.status || b.stage != t.stage)
        {
            self.steps.push(Step {
                task: t.id,
                project: t.project_id,
                from: Some((b.status, b.stage.clone())),
                to: (t.status, t.stage.clone()),
            });
        }
        let mut changes = diff(before.as_ref(), t);
        // Renovar el lease (mismo agente y mismo inicio) no es un cambio que
        // interese al historial: sería una entrada por latido.
        let renewal = before
            .as_ref()
            .and_then(|b| b.claim.zip(t.claim))
            .is_some_and(|(old, new)| old.agent == new.agent && old.since == new.since);
        if renewal {
            changes.retain(|c| c.field != "claim");
        }
        let deleted = changes
            .iter()
            .any(|c| c.field == "deleted_at" && c.before.is_null() && !c.after.is_null());
        let action = if deleted {
            Action::Deleted
        } else {
            Action::Updated
        };
        self.push(task_change(action, t, changes));
        Ok(())
    }

    fn purge_task(&mut self, id: TaskId) -> StoreResult<()> {
        let before = self.inner.task(id)?;
        self.inner.purge_task(id)?;
        if let Some(t) = before {
            let changes = vec![FieldChange {
                field: "purged".into(),
                before: Value::Bool(false),
                after: Value::Bool(true),
            }];
            self.push(task_change(Action::Deleted, &t, changes));
        }
        Ok(())
    }

    // D23: lo que se borra queda anotado en una entrada nueva (el historial
    // del proyecto o el anterior se va con él).
    fn delete_project(&mut self, id: ProjectId) -> StoreResult<()> {
        let before = self.inner.project(id)?;
        self.inner.delete_project(id)?;
        if let Some(p) = before {
            self.push(Change {
                action: Action::Deleted,
                entity: EntityKind::Project,
                entity_id: p.id.to_string(),
                project_id: None,
                task_id: None,
                changes: vec![FieldChange {
                    field: "key".into(),
                    before: Value::String(p.key),
                    after: Value::Null,
                }],
            });
        }
        Ok(())
    }

    fn wipe_workspace(&mut self) -> StoreResult<()> {
        let ws = self.inner.workspace()?;
        self.inner.wipe_workspace()?;
        self.push(Change {
            action: Action::Deleted,
            entity: EntityKind::Workspace,
            entity_id: ws.id.to_string(),
            project_id: None,
            task_id: None,
            changes: vec![FieldChange {
                field: "data".into(),
                before: Value::String(ws.name),
                after: Value::Null,
            }],
        });
        Ok(())
    }

    fn prune_activity(&mut self, before: Timestamp) -> StoreResult<usize> {
        let n = self.inner.prune_activity(before)?;
        let ws = self.inner.workspace()?;
        self.push(Change {
            action: Action::Deleted,
            entity: EntityKind::Workspace,
            entity_id: ws.id.to_string(),
            project_id: None,
            task_id: None,
            changes: vec![FieldChange {
                field: "activity_before".into(),
                before: Value::String(before.to_string()),
                after: Value::from(n),
            }],
        });
        Ok(n)
    }

    fn insert_label(&mut self, l: &Label) -> StoreResult<()> {
        self.inner.insert_label(l)?;
        self.push(Change {
            action: Action::Created,
            entity: EntityKind::Label,
            entity_id: l.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(None, l),
        });
        Ok(())
    }

    fn update_label(&mut self, l: &Label) -> StoreResult<()> {
        let before = self.inner.label(l.id)?;
        self.inner.update_label(l)?;
        self.push(Change {
            action: Action::Updated,
            entity: EntityKind::Label,
            entity_id: l.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(before.as_ref(), l),
        });
        Ok(())
    }

    fn add_dependency(&mut self, dep: Dependency) -> StoreResult<()> {
        let existed = self.inner.blockers_of(dep.blocked)?.contains(&dep.blocker);
        self.inner.add_dependency(dep)?;
        if !existed {
            let project = self.inner.task(dep.blocked)?.map(|t| t.project_id);
            self.push(dependency_change(Action::Linked, dep, project));
        }
        Ok(())
    }

    fn remove_dependency(&mut self, dep: Dependency) -> StoreResult<bool> {
        let removed = self.inner.remove_dependency(dep)?;
        if removed {
            let project = self.inner.task(dep.blocked)?.map(|t| t.project_id);
            self.push(dependency_change(Action::Unlinked, dep, project));
        }
        Ok(removed)
    }

    /// El token no entra en el historial (`Interface` no lo serializa).
    fn insert_interface(&mut self, i: &model::Interface) -> StoreResult<()> {
        self.inner.insert_interface(i)?;
        self.push(Change {
            action: Action::Created,
            entity: EntityKind::Interface,
            entity_id: i.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(None, i),
        });
        Ok(())
    }

    fn update_interface(&mut self, i: &model::Interface) -> StoreResult<()> {
        let before = self.inner.interface(i.id)?;
        self.inner.update_interface(i)?;
        self.push(Change {
            action: Action::Updated,
            entity: EntityKind::Interface,
            entity_id: i.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(before.as_ref(), i),
        });
        Ok(())
    }

    fn insert_agent(&mut self, a: &Agent) -> StoreResult<()> {
        self.inner.insert_agent(a)?;
        self.push(Change {
            action: Action::Created,
            entity: EntityKind::Agent,
            entity_id: a.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(None, a),
        });
        Ok(())
    }

    fn update_agent(&mut self, a: &Agent) -> StoreResult<()> {
        let before = self.inner.agent(a.id)?;
        self.inner.update_agent(a)?;
        self.push(Change {
            action: Action::Updated,
            entity: EntityKind::Agent,
            entity_id: a.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(before.as_ref(), a),
        });
        Ok(())
    }

    /// Las sesiones y latidos no van al historial: serían ruido.
    fn upsert_session(&mut self, s: &AgentSession) -> StoreResult<()> {
        self.inner.upsert_session(s)
    }

    fn insert_comment(&mut self, c: &Comment) -> StoreResult<()> {
        self.inner.insert_comment(c)?;
        let project_id = self.inner.task(c.task_id)?.map(|t| t.project_id);
        self.push(Change {
            action: Action::Created,
            entity: EntityKind::Comment,
            entity_id: c.id.to_string(),
            project_id,
            task_id: Some(c.task_id),
            changes: diff(None, c),
        });
        Ok(())
    }

    fn insert_inbox_item(&mut self, i: &InboxItem) -> StoreResult<()> {
        self.inner.insert_inbox_item(i)?;
        self.push(inbox_change(Action::Created, i, diff(None, i)));
        Ok(())
    }

    fn update_inbox_item(&mut self, i: &InboxItem) -> StoreResult<()> {
        let before = self.inner.inbox_item(i.id)?;
        self.inner.update_inbox_item(i)?;
        self.push(inbox_change(Action::Updated, i, diff(before.as_ref(), i)));
        Ok(())
    }
    fn insert_epic(&mut self, e: &model::Epic) -> StoreResult<()> {
        self.inner.insert_epic(e)?;
        self.push(epic_change(Action::Created, e, diff(None, e)));
        Ok(())
    }

    fn update_epic(&mut self, e: &model::Epic) -> StoreResult<()> {
        let before = self.inner.epic(e.id)?;
        self.inner.update_epic(e)?;
        self.push(epic_change(Action::Updated, e, diff(before.as_ref(), e)));
        Ok(())
    }

    fn delete_epic(&mut self, id: model::EpicId) -> StoreResult<()> {
        let before = self.inner.epic(id)?;
        self.inner.delete_epic(id)?;
        if let Some(e) = before {
            self.push(epic_change(Action::Deleted, &e, Vec::new()));
        }
        Ok(())
    }

    fn insert_template(&mut self, t: &model::Template) -> StoreResult<()> {
        self.inner.insert_template(t)?;
        self.push(template_change(Action::Created, t, diff(None, t)));
        Ok(())
    }

    fn update_template(&mut self, t: &model::Template) -> StoreResult<()> {
        let before = self.inner.template(t.id)?;
        self.inner.update_template(t)?;
        self.push(template_change(
            Action::Updated,
            t,
            diff(before.as_ref(), t),
        ));
        Ok(())
    }

    fn delete_template(&mut self, id: model::TemplateId) -> StoreResult<()> {
        let before = self.inner.template(id)?;
        self.inner.delete_template(id)?;
        if let Some(t) = before {
            self.push(template_change(Action::Deleted, &t, Vec::new()));
        }
        Ok(())
    }

    fn insert_attachment(&mut self, a: &model::Attachment, data: &[u8]) -> StoreResult<()> {
        self.inner.insert_attachment(a, data)?;
        let project = self.inner.task(a.task_id)?.map(|t| t.project_id);
        self.push(attachment_change(Action::Created, a, project));
        Ok(())
    }

    fn delete_attachment(&mut self, id: model::AttachmentId) -> StoreResult<()> {
        let before = self.inner.attachment(id)?;
        self.inner.delete_attachment(id)?;
        if let Some(a) = before {
            let project = self.inner.task(a.task_id)?.map(|t| t.project_id);
            self.push(attachment_change(Action::Deleted, &a, project));
        }
        Ok(())
    }

    fn add_relation(&mut self, r: model::Relation) -> StoreResult<()> {
        self.inner.add_relation(r)?;
        let project = self.inner.task(r.a)?.map(|t| t.project_id);
        self.push(relation_change(Action::Linked, r, project));
        Ok(())
    }

    fn remove_relation(&mut self, r: model::Relation) -> StoreResult<bool> {
        let removed = self.inner.remove_relation(r)?;
        if removed {
            let project = self.inner.task(r.a)?.map(|t| t.project_id);
            self.push(relation_change(Action::Unlinked, r, project));
        }
        Ok(removed)
    }

    fn insert_time_entry(&mut self, e: &model::TimeEntry) -> StoreResult<()> {
        self.inner.insert_time_entry(e)?;
        self.push(time_change(Action::Created, e, diff(None, e)));
        Ok(())
    }

    fn update_time_entry(&mut self, e: &model::TimeEntry) -> StoreResult<()> {
        let before = self
            .inner
            .time_entries(&store::TimeFilter {
                task: Some(e.task_id),
                ..store::TimeFilter::default()
            })?
            .into_iter()
            .find(|x| x.id == e.id);
        self.inner.update_time_entry(e)?;
        self.push(time_change(Action::Updated, e, diff(before.as_ref(), e)));
        Ok(())
    }

    fn insert_transition(&mut self, t: &model::Transition) -> StoreResult<()> {
        self.inner.insert_transition(t)
    }

    fn insert_cost_entry(&mut self, e: &model::CostEntry) -> StoreResult<()> {
        self.inner.insert_cost_entry(e)?;
        self.push(Change {
            action: Action::Created,
            entity: EntityKind::Cost,
            entity_id: e.id.to_string(),
            project_id: None,
            task_id: Some(e.task_id),
            changes: diff(None, e),
        });
        Ok(())
    }

    fn insert_cycle(&mut self, c: &model::Cycle) -> StoreResult<()> {
        self.inner.insert_cycle(c)?;
        self.push(cycle_change(Action::Created, c, diff(None, c)));
        Ok(())
    }

    fn update_cycle(&mut self, c: &model::Cycle) -> StoreResult<()> {
        let before = self.inner.cycle(c.id)?;
        self.inner.update_cycle(c)?;
        self.push(cycle_change(Action::Updated, c, diff(before.as_ref(), c)));
        Ok(())
    }

    fn delete_cycle(&mut self, id: model::CycleId) -> StoreResult<()> {
        let before = self.inner.cycle(id)?;
        self.inner.delete_cycle(id)?;
        if let Some(c) = before {
            self.push(cycle_change(Action::Deleted, &c, Vec::new()));
        }
        Ok(())
    }

    fn insert_diagram(&mut self, d: &model::Diagram) -> StoreResult<()> {
        self.inner.insert_diagram(d)?;
        self.push(diagram_change(Action::Created, d, diff(None, d)));
        Ok(())
    }

    fn update_diagram(&mut self, d: &model::Diagram) -> StoreResult<()> {
        let before = self.inner.diagram(d.id)?;
        self.inner.update_diagram(d)?;
        self.push(diagram_change(Action::Updated, d, diff(before.as_ref(), d)));
        Ok(())
    }

    fn delete_diagram(&mut self, id: model::DiagramId) -> StoreResult<()> {
        let before = self.inner.diagram(id)?;
        self.inner.delete_diagram(id)?;
        if let Some(d) = before {
            self.push(diagram_change(Action::Deleted, &d, Vec::new()));
        }
        Ok(())
    }

    fn insert_git_link(&mut self, l: &model::GitLink) -> StoreResult<bool> {
        let inserted = self.inner.insert_git_link(l)?;
        if inserted {
            self.push(git_change(Action::Created, l, diff(None, l)));
        }
        Ok(inserted)
    }

    fn delete_git_link(&mut self, id: model::GitLinkId) -> StoreResult<bool> {
        let before = self.inner.git_links(None)?.into_iter().find(|l| l.id == id);
        let deleted = self.inner.delete_git_link(id)?;
        if let (true, Some(l)) = (deleted, before) {
            self.push(git_change(Action::Deleted, &l, Vec::new()));
        }
        Ok(deleted)
    }

    fn save_view(&mut self, v: &model::SavedView) -> StoreResult<()> {
        let before = self.inner.views()?.into_iter().find(|x| x.id == v.id);
        self.inner.save_view(v)?;
        let action = if before.is_some() {
            Action::Updated
        } else {
            Action::Created
        };
        self.push(view_change(action, v, diff(before.as_ref(), v)));
        Ok(())
    }

    fn delete_view(&mut self, id: model::ViewId) -> StoreResult<()> {
        let before = self.inner.views()?.into_iter().find(|x| x.id == id);
        self.inner.delete_view(id)?;
        if let Some(v) = before {
            self.push(view_change(Action::Deleted, &v, Vec::new()));
        }
        Ok(())
    }

    /// Queda en el historial de la tarea como el campo `today`.
    fn set_today_mark(&mut self, task: TaskId, day: Option<model::Date>) -> StoreResult<()> {
        let before = self.inner.today_marks()?.get(&task).copied();
        self.inner.set_today_mark(task, day)?;
        let project = self.inner.task(task)?.map(|t| t.project_id);
        self.push(Change {
            action: Action::Updated,
            entity: EntityKind::Task,
            entity_id: task.to_string(),
            project_id: project,
            task_id: Some(task),
            changes: vec![FieldChange {
                field: "today".into(),
                before: serde_json::to_value(before).unwrap_or_default(),
                after: serde_json::to_value(day).unwrap_or_default(),
            }],
        });
        Ok(())
    }

    fn save_automation(&mut self, a: &model::Automation) -> StoreResult<()> {
        let before = self.inner.automations()?.into_iter().find(|x| x.id == a.id);
        self.inner.save_automation(a)?;
        let action = if before.is_some() {
            Action::Updated
        } else {
            Action::Created
        };
        self.push(Change {
            action,
            entity: EntityKind::Automation,
            entity_id: a.id.to_string(),
            project_id: a.project_id,
            task_id: None,
            changes: diff(before.as_ref(), a),
        });
        Ok(())
    }

    fn delete_automation(&mut self, id: model::AutomationId) -> StoreResult<()> {
        let before = self.inner.automations()?.into_iter().find(|x| x.id == id);
        self.inner.delete_automation(id)?;
        if let Some(a) = before {
            self.push(Change {
                action: Action::Deleted,
                entity: EntityKind::Automation,
                entity_id: a.id.to_string(),
                project_id: a.project_id,
                task_id: None,
                changes: Vec::new(),
            });
        }
        Ok(())
    }

    /// El secreto no entra en el historial (`Webhook` no lo serializa).
    fn save_webhook(&mut self, w: &model::Webhook) -> StoreResult<()> {
        let before = self.inner.webhooks()?.into_iter().find(|x| x.id == w.id);
        self.inner.save_webhook(w)?;
        let action = if before.is_some() {
            Action::Updated
        } else {
            Action::Created
        };
        self.push(Change {
            action,
            entity: EntityKind::Webhook,
            entity_id: w.id.to_string(),
            project_id: None,
            task_id: None,
            changes: diff(before.as_ref(), w),
        });
        Ok(())
    }

    fn delete_webhook(&mut self, id: model::WebhookId) -> StoreResult<()> {
        self.inner.delete_webhook(id)?;
        self.push(Change {
            action: Action::Deleted,
            entity: EntityKind::Webhook,
            entity_id: id.to_string(),
            project_id: None,
            task_id: None,
            changes: Vec::new(),
        });
        Ok(())
    }

    // La cola de envíos es entrega, no un cambio del dominio.
    fn save_delivery(&mut self, d: &model::Delivery) -> StoreResult<()> {
        self.inner.save_delivery(d)
    }

    // La idempotencia es memoria de respuestas, no un cambio del dominio.
    fn save_idempotent(
        &mut self,
        actor: &model::Actor,
        key: &str,
        command: &str,
        response: &str,
        at: Timestamp,
    ) -> StoreResult<()> {
        self.inner
            .save_idempotent(actor, key, command, response, at)
    }
    fn purge_idempotency(&mut self, before: Timestamp) -> StoreResult<usize> {
        self.inner.purge_idempotency(before)
    }

    // Las menciones se derivan del texto, que ya queda en el historial.
    fn set_mentions(
        &mut self,
        source: TaskId,
        comment: Option<model::CommentId>,
        targets: &[TaskId],
    ) -> StoreResult<()> {
        self.inner.set_mentions(source, comment, targets)
    }

    fn set_diagram_mentions(
        &mut self,
        source: TaskId,
        comment: Option<model::CommentId>,
        targets: &[model::DiagramId],
    ) -> StoreResult<()> {
        self.inner.set_diagram_mentions(source, comment, targets)
    }
}

fn time_change(action: Action, e: &model::TimeEntry, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Time,
        entity_id: e.id.to_string(),
        project_id: None,
        task_id: Some(e.task_id),
        changes,
    }
}

fn cycle_change(action: Action, c: &model::Cycle, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Cycle,
        entity_id: c.id.to_string(),
        project_id: Some(c.project_id),
        task_id: None,
        changes,
    }
}

fn diagram_change(action: Action, d: &model::Diagram, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Diagram,
        entity_id: d.id.to_string(),
        project_id: Some(d.project_id),
        task_id: None,
        changes,
    }
}

fn git_change(action: Action, l: &model::GitLink, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Git,
        entity_id: l.id.to_string(),
        project_id: None,
        task_id: Some(l.task_id),
        changes,
    }
}

fn epic_change(action: Action, e: &model::Epic, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Epic,
        entity_id: e.id.to_string(),
        project_id: Some(e.project_id),
        task_id: None,
        changes,
    }
}

fn view_change(action: Action, v: &model::SavedView, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::View,
        entity_id: v.id.to_string(),
        project_id: v.project_id,
        task_id: None,
        changes,
    }
}

fn template_change(action: Action, t: &model::Template, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Template,
        entity_id: t.id.to_string(),
        project_id: None,
        task_id: None,
        changes,
    }
}

fn attachment_change(action: Action, a: &model::Attachment, project: Option<ProjectId>) -> Change {
    Change {
        action,
        entity: EntityKind::Attachment,
        entity_id: a.id.to_string(),
        project_id: project,
        task_id: Some(a.task_id),
        changes: vec![FieldChange {
            field: "name".into(),
            before: Value::Null,
            after: Value::String(a.name.clone()),
        }],
    }
}

fn relation_change(action: Action, r: model::Relation, project: Option<ProjectId>) -> Change {
    Change {
        action,
        entity: EntityKind::Relation,
        entity_id: format!("{}~{}~{}", r.a, r.kind, r.b),
        project_id: project,
        task_id: Some(r.a),
        changes: vec![FieldChange {
            field: r.kind.as_str().into(),
            before: Value::Null,
            after: Value::String(r.b.to_string()),
        }],
    }
}

fn inbox_change(action: Action, i: &InboxItem, changes: Vec<FieldChange>) -> Change {
    Change {
        action,
        entity: EntityKind::Inbox,
        entity_id: i.id.to_string(),
        project_id: None,
        task_id: i.task_id,
        changes,
    }
}
