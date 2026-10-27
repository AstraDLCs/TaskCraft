//! Épicas e hitos (RF-MOD-13): agrupan tareas de un proyecto con una fecha
//! objetivo y un progreso calculado (por número de tareas y por esfuerzo).

use i18n::Msg;
use model::{Epic, EpicId, EpicKind, EpicStatus, Status, Task};
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::{detail, paged, save};
use crate::args::parse_when;
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError};

command!(Create, "epic.create", "cmd-epic-create", create, write, [req "project": K::Text, req "name": K::Text, opt "kind": K::Enum(EpicKind::NAMES), opt "description": K::Text, opt "target": K::Date, opt "idempotency_key": K::Text]);
command!(Update, "epic.update", "cmd-epic-update", update, write, [req "epic": K::Text, opt "project": K::Text, opt "name": K::Text, opt "description": K::Text, opt "target": K::Date]);
command!(Close, "epic.close", "cmd-epic-close", close, write, [req "epic": K::Text, opt "project": K::Text]);
command!(Reopen, "epic.reopen", "cmd-epic-reopen", reopen, write, [req "epic": K::Text, opt "project": K::Text]);
command!(Delete, "epic.delete", "cmd-epic-delete", delete, write, [req "epic": K::Text, opt "project": K::Text]);
command!(List, "epic.list", "cmd-epic-list", list_epics, read, [opt "project": K::Text, opt "kind": K::Enum(EpicKind::NAMES), opt "status": K::Enum(EpicStatus::NAMES), opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Get, "epic.get", "cmd-epic-get", get, read, [req "epic": K::Text, opt "project": K::Text]);
command!(SetEpic, "task.set_epic", "cmd-task-set_epic", set_epic, write, [req "task": K::Text, req "epic": K::Text, opt "revision": K::Int]);
command!(SetMilestone, "task.set_milestone", "cmd-task-set_milestone", set_milestone, write, [req "task": K::Text, req "milestone": K::Text, opt "revision": K::Int]);

/// Progreso de una épica: las canceladas no cuentan.
pub(crate) fn progress(r: &dyn Reader, epic: EpicId) -> Result<Value, EngineError> {
    let tasks: Vec<Task> = r
        .tasks(&TaskFilter {
            include_archived: true,
            group: Some(epic),
            ..TaskFilter::default()
        })?
        .into_iter()
        .filter(|t| t.status != Status::Cancelled)
        .collect();
    let done = tasks.iter().filter(|t| t.status == Status::Done).count();
    let effort: u64 = tasks
        .iter()
        .filter_map(|t| t.estimate)
        .map(|e| u64::from(e.0))
        .sum();
    let effort_done: u64 = tasks
        .iter()
        .filter(|t| t.status == Status::Done)
        .filter_map(|t| t.estimate)
        .map(|e| u64::from(e.0))
        .sum();
    let percent = |part: u64, whole: u64| part.saturating_mul(100).checked_div(whole).unwrap_or(0);
    let total = u64::try_from(tasks.len()).unwrap_or(u64::MAX);
    let done = u64::try_from(done).unwrap_or(0);
    Ok(json!({
        "tasks": total,
        "done": done,
        "percent": percent(done, total),
        "effort": effort as f64 / 100.0,
        "effort_done": effort_done as f64 / 100.0,
        "effort_percent": percent(effort_done, effort),
    }))
}

pub(crate) fn view(r: &dyn Reader, e: &Epic) -> Result<Value, EngineError> {
    let project = r.project(e.project_id)?.map(|p| p.key).unwrap_or_default();
    Ok(json!({
        "id": e.id,
        "project": project,
        "kind": e.kind,
        "name": e.name,
        "description": e.description,
        "target": e.target,
        "status": e.status,
        "locked": e.locked,
        "progress": progress(r, e.id)?,
        "created_at": e.created_at,
        "updated_at": e.updated_at,
    }))
}

fn target_project(cx: &Cx<'_>, args: &Args) -> Result<Option<model::ProjectId>, EngineError> {
    args.get("project")
        .map(|p| lookup::project(&*cx.tx, p).map(|p| p.id))
        .transpose()
}

fn find(cx: &Cx<'_>, args: &Args) -> Result<Epic, EngineError> {
    let project = target_project(cx, args)?;
    lookup::epic(&*cx.tx, args.require("epic")?, project, None)
}

fn name(raw: &str) -> Result<String, EngineError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(EngineError::arg(
            "name",
            Msg::new("arg-length").arg("min", 1u32).arg("max", 120u32),
        ));
    }
    Ok(name.to_owned())
}

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    crate::guard::check_scope(cx, project.id)?;
    let epic = Epic {
        id: EpicId::generate(),
        project_id: project.id,
        kind: args.parse("kind")?.unwrap_or(EpicKind::Epic),
        name: name(args.require("name")?)?,
        description: args.get("description").unwrap_or_default().to_owned(),
        target: args
            .get("target")
            .map(|t| parse_when("target", t))
            .transpose()?,
        status: EpicStatus::Open,
        locked: false,
        created_at: cx.now,
        updated_at: cx.now,
    };
    cx.tx.insert_epic(&epic)?;
    view(&*cx.tx, &epic)
}

fn store(cx: &mut Cx<'_>, mut epic: Epic) -> Result<Value, EngineError> {
    crate::guard::check_scope(cx, epic.project_id)?;
    epic.updated_at = cx.now;
    cx.tx.update_epic(&epic)?;
    view(&*cx.tx, &epic)
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut epic = find(cx, args)?;
    if let Some(n) = args.get("name") {
        epic.name = name(n)?;
    }
    if let Some(d) = args.get("description") {
        epic.description = d.to_owned();
    }
    if let Some(target) = args.clearable("target", |t| parse_when("target", t))? {
        epic.target = target;
    }
    store(cx, epic)
}

fn close(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut epic = find(cx, args)?;
    epic.status = EpicStatus::Closed;
    store(cx, epic)
}

fn reopen(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut epic = find(cx, args)?;
    epic.status = EpicStatus::Open;
    store(cx, epic)
}

/// Sus tareas se quedan sin épica (o sin hito); no se borra ninguna.
fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let epic = find(cx, args)?;
    cx.tx.delete_epic(epic.id)?;
    Ok(json!({ "deleted": epic.id, "name": epic.name }))
}

fn list_epics(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = target_project(cx, args)?;
    let kind: Option<EpicKind> = args.parse("kind")?;
    let status: Option<EpicStatus> = args.parse("status")?;
    let epics: Vec<Epic> = r
        .epics(project)?
        .into_iter()
        .filter(|e| kind.is_none_or(|k| e.kind == k) && status.is_none_or(|s| e.status == s))
        .collect();
    paged(epics, args, |e| view(r, e))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let epic = find(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let mut out = view(r, &epic)?;
    let tasks: Vec<String> = r
        .tasks(&TaskFilter {
            group: Some(epic.id),
            ..TaskFilter::default()
        })?
        .iter()
        .map(|t| keys.of(t))
        .collect();
    if let Some(obj) = out.as_object_mut() {
        obj.insert("task_keys".into(), json!(tasks));
    }
    Ok(out)
}

/// `none` (o vacío) la quita.
fn assign_group(cx: &mut Cx<'_>, args: &Args, kind: EpicKind) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let param = match kind {
        EpicKind::Epic => "epic",
        EpicKind::Milestone => "milestone",
    };
    let raw = args.require(param)?;
    let id = if raw.trim().is_empty() || raw.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(lookup::epic(&*cx.tx, raw, Some(task.project_id), Some(kind))?.id)
    };
    match kind {
        EpicKind::Epic => task.epic_id = id,
        EpicKind::Milestone => task.milestone_id = id,
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn set_epic(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    assign_group(cx, args, EpicKind::Epic)
}

fn set_milestone(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    assign_group(cx, args, EpicKind::Milestone)
}
