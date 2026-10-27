use i18n::{Localize, Msg};
use model::{Actor, Priority, ProjectStatus, Status, Task, TaskId};
use serde_json::Value;
use store::{Reader, TaskFilter};

use super::{detail, ensure_label, paged, save};
use crate::args::parse_when;
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(Create, "task.create", "cmd-task-create", create, write, [opt "project": K::Text, req "title": K::Text, opt "description": K::Text, opt "priority": K::Enum(model::Priority::NAMES), opt "status": K::Text, opt "parent": K::Text, opt "start": K::Date, opt "due": K::Date, opt "labels": K::List, opt "estimate": K::Text, opt "epic": K::Text, opt "milestone": K::Text, opt "idempotency_key": K::Text]);
command!(Get, "task.get", "cmd-task-get", get, read, [req "task": K::Text]);
command!(List, "task.list", "cmd-task-list", list_tasks, read, [opt "project": K::Text, opt "filter": K::Text, opt "view": K::Text, opt "status": K::List, opt "parent": K::Text, opt "label": K::Text, opt "archived": K::Bool, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Search, "task.search", "cmd-task-search", search, read, [req "query": K::Text, opt "project": K::Text, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Update, "task.update", "cmd-task-update", update, write, [req "task": K::Text, opt "title": K::Text, opt "description": K::Text, opt "priority": K::Enum(model::Priority::NAMES), opt "start": K::Date, opt "due": K::Date, opt "estimate": K::Text, opt "revision": K::Int]);
command!(Move, "task.move", "cmd-task-move", move_task, write, [req "task": K::Text, req "status": K::Text, opt "note": K::Text, opt "revision": K::Int]);
command!(SetParent, "task.set_parent", "cmd-task-set_parent", set_parent, write, [req "task": K::Text, req "parent": K::Text, opt "revision": K::Int]);
command!(Assign, "task.assign", "cmd-task-assign", assign, write, [req "task": K::Text, req "to": K::Text, opt "revision": K::Int]);
command!(AddAssignee, "task.add_assignee", "cmd-task-add_assignee", add_assignee, write, [req "task": K::Text, req "agent": K::Text, opt "revision": K::Int]);
command!(RemoveAssignee, "task.remove_assignee", "cmd-task-remove_assignee", remove_assignee, write, [req "task": K::Text, req "agent": K::Text, opt "revision": K::Int]);
command!(Label, "task.label", "cmd-task-label", label, write, [req "task": K::Text, opt "add": K::List, opt "remove": K::List, opt "revision": K::Int]);

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let parent = args
        .get("parent")
        .map(|p| lookup::editable_task(&*cx.tx, p))
        .transpose()?;
    let project = match (args.get("project"), &parent) {
        (Some(p), _) => lookup::project(&*cx.tx, p)?,
        (None, Some(parent)) => cx
            .tx
            .project(parent.project_id)?
            .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?,
        (None, None) => return Err(EngineError::MissingArg("project".into())),
    };
    let new = NewTask {
        description: args.get("description").unwrap_or_default().to_owned(),
        status: args.get("status").map(str::to_owned),
        priority: args.parse("priority")?.unwrap_or(Priority::Medium),
        start: args
            .get("start")
            .map(|s| parse_when("start", s))
            .transpose()?,
        due: args.get("due").map(|s| parse_when("due", s)).transpose()?,
        labels: args.list("labels"),
        estimate: args.get("estimate").map(str::to_owned),
        epic: args.get("epic").map(str::to_owned),
        milestone: args.get("milestone").map(str::to_owned),
        ..NewTask::titled(args.require("title")?)
    };
    let task = insert_new(cx, project.id, parent.as_ref(), new)?;
    detail(cx, &task)
}

/// Datos de una tarea nueva (de `task.create`, `plan.create`, la bandeja o
/// una plantilla). Lo que depende del proyecto llega como texto y se valida
/// contra él al crearla.
pub(super) struct NewTask {
    pub title: String,
    pub description: String,
    /// Clave del flujo del proyecto (estado base o etapa, D10); `None` = `todo`.
    pub status: Option<String>,
    pub priority: Priority,
    pub start: Option<model::When>,
    pub due: Option<model::When>,
    pub labels: Vec<String>,
    pub checklist: Vec<String>,
    pub criteria: Vec<String>,
    pub estimate: Option<String>,
    pub epic: Option<String>,
    pub milestone: Option<String>,
    pub fields: Vec<(String, String)>,
}

impl NewTask {
    pub fn titled(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            description: String::new(),
            status: None,
            priority: Priority::Medium,
            start: None,
            due: None,
            labels: Vec::new(),
            checklist: Vec::new(),
            criteria: Vec::new(),
            estimate: None,
            epic: None,
            milestone: None,
            fields: Vec::new(),
        }
    }
}

/// Valida y crea una tarea con la siguiente clave del proyecto.
pub(super) fn insert_new(
    cx: &mut Cx<'_>,
    project: model::ProjectId,
    parent: Option<&Task>,
    new: NewTask,
) -> Result<Task, EngineError> {
    let mut project = cx
        .tx
        .project(project)?
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?;
    if project.status == ProjectStatus::Archived {
        return Err(EngineError::invalid(
            Msg::new("project-archived").arg("project", &project.key),
        ));
    }
    crate::guard::check_scope(cx, project.id)?;
    if let Some(parent) = parent {
        crate::guard::check_write(cx, parent, None)?;
        if parent.project_id != project.id {
            return Err(EngineError::invalid(Msg::new("parent-other-project")));
        }
        check_depth(&*cx.tx, parent, 1)?;
    }
    let (status, stage) = match &new.status {
        None => (Status::Todo, None),
        Some(key) => rules::resolve_status(&project, key)?,
    };
    if !matches!(status, Status::Backlog | Status::Todo) {
        return Err(EngineError::arg("status", Msg::new("task-new-status")));
    }
    let estimate = new
        .estimate
        .as_deref()
        .filter(|e| !e.trim().is_empty())
        .map(|e| rules::parse_estimate(project.estimate_unit, e))
        .transpose()?;
    let group = |reference: &Option<String>, kind| -> Result<Option<model::EpicId>, EngineError> {
        reference
            .as_deref()
            .filter(|r| !r.trim().is_empty())
            .map(|r| lookup::epic(&*cx.tx, r, Some(project.id), Some(kind)).map(|e| e.id))
            .transpose()
    };
    let epic_id = group(&new.epic, model::EpicKind::Epic)?;
    let milestone_id = group(&new.milestone, model::EpicKind::Milestone)?;
    let mut fields = std::collections::BTreeMap::new();
    for (key, value) in &new.fields {
        let def = rules::find_field(&project, key)?;
        let value = rules::field_value(def, value)?;
        if !value.is_empty() {
            fields.insert(def.key.clone(), value);
        }
    }
    let item = |text: String| -> Result<model::CheckItem, EngineError> {
        let text = text.trim().to_owned();
        if text.is_empty() || text.chars().count() > 500 {
            return Err(EngineError::arg(
                "text",
                Msg::new("arg-length").arg("min", 1u32).arg("max", 500u32),
            ));
        }
        Ok(model::CheckItem {
            id: model::ItemId::generate(),
            text,
            done: false,
        })
    };

    let mut task = Task {
        id: TaskId::generate(),
        project_id: project.id,
        number: project.next_number,
        parent_id: parent.map(|p| p.id),
        title: rules::task_title(&new.title)?,
        description: new.description,
        status,
        priority: new.priority,
        start: new.start,
        due: new.due,
        labels: Vec::new(),
        assignees: Vec::new(),
        creator: cx.ctx.actor.clone(),
        checklist: new
            .checklist
            .into_iter()
            .map(item)
            .collect::<Result<_, _>>()?,
        criteria: new
            .criteria
            .into_iter()
            .map(item)
            .collect::<Result<_, _>>()?,
        revision: 1,
        created_at: cx.now,
        updated_at: cx.now,
        archived_at: None,
        deleted_at: None,
        claim: None,
        needs_human: None,
        locked: false,
        stage,
        estimate,
        epic_id,
        milestone_id,
        fields,
        recurrence: None,
        cycles: Vec::new(),
    };
    for name in new.labels {
        let id = ensure_label(cx, &name)?;
        if !task.labels.contains(&id) {
            task.labels.push(id);
        }
    }

    super::check_wip(cx, &project, &task)?;
    project.next_number = project.next_number.saturating_add(1);
    project.updated_at = cx.now;
    cx.tx.update_project(&project)?;
    cx.tx.insert_task(&task)?;
    super::sync_mentions(cx, task.id, None, &task.description)?;
    Ok(task)
}

/// Falla si colgar `extra_levels` niveles bajo `parent` supera la profundidad máxima.
fn check_depth(r: &dyn Reader, parent: &Task, extra_levels: u32) -> Result<(), EngineError> {
    let max = r.workspace()?.settings.max_depth;
    let parent_depth = u32::try_from(lookup::ancestors(r, parent)?.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    if parent_depth.saturating_add(extra_levels) > max {
        return Err(EngineError::invalid(
            Msg::new("depth-exceeded").arg("max", max),
        ));
    }
    Ok(())
}

/// Niveles del subárbol de `task`, contándola (una hoja mide 1).
fn height(r: &dyn Reader, task: &Task) -> Result<u32, EngineError> {
    let mut best = 0;
    for child in r.children(task.id)? {
        best = best.max(height(r, &child)?);
    }
    Ok(best.saturating_add(1))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    detail(cx, &task)
}

fn list_tasks(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let tasks = select(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    paged(tasks, args, |t| views::task_summary(r, &keys, t))
}

/// Las tareas que piden `project`, `status`, `parent`, `label`, `archived`,
/// `view` y `filter` (D12), en orden. Lo comparten `task.list` y la
/// exportación a CSV (RF-EXP-07).
pub(super) fn select(cx: &Cx<'_>, args: &Args) -> Result<Vec<Task>, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let mut filter = TaskFilter {
        include_archived: args.flag("archived")?,
        ..TaskFilter::default()
    };
    if let Some(p) = args.get("project") {
        filter.project = Some(lookup::project(r, p)?.id);
    }
    for s in args.list("status") {
        filter.statuses.push(
            s.parse()
                .map_err(|e: model::ParseEnumError| EngineError::arg("status", e.msg()))?,
        );
    }
    match args.get("parent") {
        Some("root") => filter.parent = Some(None),
        Some(p) => filter.parent = Some(Some(lookup::task(r, p)?.id)),
        None => {}
    }
    let label = args
        .get("label")
        .map(|name| {
            r.label_by_name(name)?.ok_or_else(|| {
                EngineError::not_found(Msg::new("notfound-label").arg("label", name))
            })
        })
        .transpose()?;
    let saved = args
        .get("view")
        .map(|v| super::view::find(r, v))
        .transpose()?;
    if let Some(view) = &saved
        && filter.project.is_none()
    {
        filter.project = view.project_id;
    }
    let mut tasks: Vec<Task> = r
        .tasks(&filter)?
        .into_iter()
        .filter(|t| label.as_ref().is_none_or(|l| t.labels.contains(&l.id)))
        .collect();
    // D12: el filtro de la vista y el escrito se combinan con Y.
    for src in saved
        .iter()
        .map(|v| v.filter.as_str())
        .chain(args.get("filter"))
    {
        tasks = super::view::apply(cx, tasks, src)?;
    }
    Ok(tasks)
}

/// Máximo de coincidencias que se ordenan por relevancia antes de paginar.
const SEARCH_MAX: usize = 1000;

/// Texto completo en título, descripción y comentarios (RF-VIS-11), por
/// relevancia. Incluye archivadas: siguen siendo buscables (RF-HIS-07).
fn search(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = args
        .get("project")
        .map(|p| lookup::project(r, p).map(|p| p.id))
        .transpose()?;
    let ids = r.search_tasks(args.require("query")?, project, SEARCH_MAX)?;
    let mut tasks = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(t) = r.task(id)? {
            tasks.push(t);
        }
    }
    let keys = Keys::load(r)?;
    paged(tasks, args, |t| views::task_summary(r, &keys, t))
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    if let Some(title) = args.get("title") {
        task.title = rules::task_title(title)?;
    }
    let description = args.get("description");
    if let Some(description) = description {
        task.description = description.to_owned();
    }
    if let Some(estimate) = args.get("estimate") {
        task.estimate = if estimate.trim().is_empty() || estimate.eq_ignore_ascii_case("none") {
            None
        } else {
            let unit = cx
                .tx
                .project(task.project_id)?
                .map(|p| p.estimate_unit)
                .unwrap_or(model::EstimateUnit::Hours);
            Some(rules::parse_estimate(unit, estimate)?)
        };
    }
    if let Some(priority) = args.parse("priority")? {
        task.priority = priority;
    }
    if let Some(start) = args.clearable("start", |s| parse_when("start", s))? {
        task.start = start;
    }
    if let Some(due) = args.clearable("due", |s| parse_when("due", s))? {
        task.due = due;
    }
    save(cx, &mut task)?;
    if description.is_some() {
        super::sync_mentions(cx, task.id, None, &task.description)?;
    }
    detail(cx, &task)
}

/// Pasa la tarea a otra columna, con un mensaje opcional para quien siga
/// (D20). Una columna con dueño se la asigna a ese agente; un agente que la
/// pasa a «en progreso» la toma como con `task.claim`; y quien la tenía
/// tomada la suelta al sacarla de «en progreso» o dársela a otro.
fn move_task(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let project = cx
        .tx
        .project(task.project_id)?
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?;
    // Un estado base o una etapa del flujo del proyecto (D10).
    let (to, stage) = rules::resolve_status(&project, args.require("status")?)?;
    rules::check_transition(task.status, to)?;
    let owner = rules::column_owner(&project, to, stage.as_deref());
    if let Some(text) = args.get("note").map(str::trim).filter(|n| !n.is_empty()) {
        super::coord::note(cx, &task, text)?;
    }
    if let Some(agent) = crate::guard::agent_id(cx)
        && to == Status::InProgress
        && task.status != Status::InProgress
        && owner.is_none_or(|o| o == agent)
    {
        let task = super::coord::take(cx, agent, task, stage)?;
        return detail(cx, &task);
    }
    if task.status == to && task.stage == stage {
        return detail(cx, &task);
    }
    let handed_over = owner.is_some_and(|o| task.claim.is_some_and(|c| c.agent != o));
    if to != Status::InProgress || handed_over {
        task.claim = None;
    }
    task.status = to;
    task.stage = stage;
    if let Some(owner) = owner {
        task.assign(Actor::Agent(owner));
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn set_parent(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let new_parent = args.clearable("parent", |p| lookup::editable_task(&*cx.tx, p))?;
    let Some(new_parent) = new_parent else {
        return Err(EngineError::MissingArg("parent".into()));
    };

    if let Some(parent) = &new_parent {
        if parent.project_id != task.project_id {
            return Err(EngineError::invalid(Msg::new("parent-other-project")));
        }
        let lineage = lookup::ancestors(&*cx.tx, parent)?;
        if parent.id == task.id || lineage.iter().any(|a| a.id == task.id) {
            return Err(EngineError::invalid(Msg::new("parent-is-descendant")));
        }
        let levels = height(&*cx.tx, &task)?;
        check_depth(&*cx.tx, parent, levels)?;
    }
    task.parent_id = new_parent.map(|p| p.id);
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn assign(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let to = args.clearable("to", |a| actor_ref(r, a))?;
    let Some(to) = to else {
        return Err(EngineError::MissingArg("to".into()));
    };
    // Deja solo a este (o a nadie); para añadir sin quitar, task.add_assignee.
    task.set_assignee(to);
    save(cx, &mut task)?;
    detail(cx, &task)
}

/// `agent:<ULID>`, `supervisor` o el nombre de un agente.
fn actor_ref(r: &dyn Reader, text: &str) -> Result<Actor, EngineError> {
    match text.trim().parse::<Actor>() {
        Ok(a) => Ok(a),
        Err(_) => lookup::agent(r, text).map(|a| Actor::Agent(a.id)),
    }
}

/// Añade un asignado más; una tarea puede tener tantos como haga falta (D20).
fn add_assignee(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let actor = actor_ref(&*cx.tx, args.require("agent")?)?;
    if task.assign(actor) {
        save(cx, &mut task)?;
    }
    detail(cx, &task)
}

fn remove_assignee(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let actor = actor_ref(&*cx.tx, args.require("agent")?)?;
    let before = task.assignees.len();
    task.assignees.retain(|a| *a != actor);
    if task.assignees.len() != before {
        save(cx, &mut task)?;
    }
    detail(cx, &task)
}

fn label(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    for name in args.list("add") {
        let id = ensure_label(cx, &name)?;
        if !task.labels.contains(&id) {
            task.labels.push(id);
        }
    }
    for name in args.list("remove") {
        if let Some(l) = cx.tx.label_by_name(&name)? {
            task.labels.retain(|id| *id != l.id);
        }
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}
