//! Vistas guardadas con el lenguaje de filtros (RF-VIS-12, D12) y la vista
//! «Hoy» (RF-VIS-08): vence hoy, atrasadas y marcadas a mano.

use std::collections::{HashMap, HashSet};

use i18n::Msg;
use jiff::tz::TimeZone;
use model::{Date, SavedView, Task, TaskId, Timestamp, ViewId};
use rules::filter::{self, Expr, Extra, Names};
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::paged;
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

const LAYOUTS: &[&str] = &[
    "list", "board", "table", "today", "gantt", "matrix", "graph",
];

command!(Save, "view.save", "cmd-view-save", save, write, [req "name": K::Text, req "filter": K::Text, opt "layout": K::Enum(LAYOUTS), opt "columns": K::List, opt "project": K::Text]);
command!(List, "view.list", "cmd-view-list", list, read, []);
command!(Get, "view.get", "cmd-view-get", get, read, [req "view": K::Text, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Delete, "view.delete", "cmd-view-delete", delete, write, [req "view": K::Text]);
command!(Mark, "today.mark", "cmd-today-mark", mark, write, [req "task": K::Text]);
command!(Unmark, "today.unmark", "cmd-today-unmark", unmark, write, [req "task": K::Text]);
command!(Today, "today.list", "cmd-today-list", today_list, read, [opt "project": K::Text, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);

/// Día local del reloj del comando.
fn day(cx: &Cx<'_>) -> Date {
    cx.now.to_zoned(TimeZone::system()).date()
}

/// Lee un filtro; si está mal escrito, `INVALID` con posición (D12).
pub(super) fn parse(src: &str) -> Result<Expr, EngineError> {
    filter::parse(src).map_err(|e| {
        EngineError::Rule(rules::RuleError::Filter {
            position: e.position,
            expected: e.detail,
        })
    })
}

/// Lo necesario para armar los hechos de muchas tareas de una vez.
struct Facts {
    names: Names,
    keys: HashMap<TaskId, String>,
    blocked: HashSet<TaskId>,
    locked: HashSet<TaskId>,
    marks: HashSet<TaskId>,
    /// Envejecidas según el umbral del workspace (RF-VIS-16).
    stale: HashSet<TaskId>,
}

impl Facts {
    fn load(r: &dyn Reader, now: Timestamp, tasks: &[Task]) -> Result<Self, EngineError> {
        let today = now.to_zoned(TimeZone::system()).date();
        let weeks = r.workspace()?.settings.stale_weeks;
        let stale = tasks
            .iter()
            .filter(|t| rules::is_stale(t, weeks, now))
            .map(|t| t.id)
            .collect();
        let projects = r.projects(true)?;
        let project_keys: HashMap<_, _> = projects.iter().map(|p| (p.id, p.key.clone())).collect();
        let mut keys: HashMap<TaskId, String> = tasks
            .iter()
            .map(|t| {
                let p = project_keys.get(&t.project_id).map_or("?", String::as_str);
                (t.id, format!("{p}-{}", t.number))
            })
            .collect();
        let all = Keys::load(r)?;
        for parent in tasks.iter().filter_map(|t| t.parent_id) {
            if let std::collections::hash_map::Entry::Vacant(e) = keys.entry(parent) {
                e.insert(all.of_id(r, parent)?);
            }
        }
        let names = Names {
            projects: project_keys,
            labels: r.labels()?.into_iter().map(|l| (l.id, l.name)).collect(),
            agents: r.agents()?.into_iter().map(|a| (a.id, a.name)).collect(),
            epics: r.epics(None)?.into_iter().map(|e| (e.id, e.name)).collect(),
            cycles: r
                .cycles(None)?
                .into_iter()
                .map(|c| {
                    let current = c.closed_at.is_none() && c.start <= today;
                    (c.id, (c.name, current))
                })
                .collect(),
        };
        Ok(Self {
            names,
            keys,
            blocked: r.blocked_tasks(&rules::closed_statuses())?,
            locked: r.locked_subtrees()?,
            marks: marked(r, today)?,
            stale,
        })
    }

    fn matches(&self, expr: &Expr, t: &Task, today: Date) -> bool {
        let key = self.keys.get(&t.id).map_or("", String::as_str);
        let extra = Extra {
            key,
            parent: t
                .parent_id
                .and_then(|p| self.keys.get(&p))
                .map(String::as_str),
            ready: !self.blocked.contains(&t.id),
            locked: self.locked.contains(&t.id),
            today: self.marks.contains(&t.id),
            stale: self.stale.contains(&t.id),
        };
        filter::matches(expr, &self.names.facts(t, extra), today)
    }
}

/// Columnas por defecto del CSV.
pub(super) const CSV_COLUMNS: &[&str] = &[
    "key", "title", "status", "priority", "assignee", "labels", "due",
];

/// Valores de texto de las columnas pedidas, para el CSV (RF-EXP-07).
pub(super) fn rows(
    cx: &Cx<'_>,
    tasks: &[Task],
    columns: &[String],
) -> Result<Vec<Vec<String>>, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let facts = Facts::load(r, cx.now, tasks)?;
    let projects: HashMap<_, _> = r.projects(true)?.into_iter().map(|p| (p.id, p)).collect();
    let date = |w: &Option<model::When>| w.as_ref().map(ToString::to_string).unwrap_or_default();
    Ok(tasks
        .iter()
        .map(|t| {
            let extra = Extra {
                key: facts.keys.get(&t.id).map_or("", String::as_str),
                parent: t
                    .parent_id
                    .and_then(|p| facts.keys.get(&p))
                    .map(String::as_str),
                ready: !facts.blocked.contains(&t.id),
                locked: facts.locked.contains(&t.id),
                today: facts.marks.contains(&t.id),
                stale: facts.stale.contains(&t.id),
            };
            let f = facts.names.facts(t, extra);
            columns
                .iter()
                .map(|c| match c.as_str() {
                    "key" => f.key.to_owned(),
                    "title" => t.title.clone(),
                    "description" => t.description.clone(),
                    "status" => f.stage.unwrap_or(t.status.as_str()).to_owned(),
                    "priority" => t.priority.as_str().to_owned(),
                    "assignee" => f.assignee.unwrap_or_default().to_owned(),
                    "labels" => f.labels.join(";"),
                    "due" => date(&t.due),
                    "start" => date(&t.start),
                    "estimate" => t
                        .estimate
                        .zip(projects.get(&t.project_id))
                        .map(|(e, p)| rules::format_estimate(p.estimate_unit, e))
                        .unwrap_or_default(),
                    "epic" => f.epic.unwrap_or_default().to_owned(),
                    "milestone" => f.milestone.unwrap_or_default().to_owned(),
                    "project" => f.project.to_owned(),
                    "parent" => f.parent.unwrap_or_default().to_owned(),
                    "created" => t.created_at.to_string(),
                    "updated" => t.updated_at.to_string(),
                    "ready" => f.ready.to_string(),
                    "claimed" => f.claimed.to_string(),
                    other => other
                        .strip_prefix("field.")
                        .and_then(|k| t.fields.get(k).cloned())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .collect())
}

/// Tareas marcadas hoy; las marcas de días anteriores ya no cuentan.
fn marked(r: &dyn Reader, today: Date) -> Result<HashSet<TaskId>, EngineError> {
    Ok(r.today_marks()?
        .into_iter()
        .filter(|(_, d)| *d == today)
        .map(|(t, _)| t)
        .collect())
}

/// ¿Cumple `task` el filtro? Para las automatizaciones (RF-INT-03).
pub(crate) fn task_matches(
    r: &dyn Reader,
    now: Timestamp,
    task: &Task,
    src: &str,
) -> Result<bool, EngineError> {
    let expr = parse(src)?;
    if expr == Expr::And(Vec::new()) {
        return Ok(true);
    }
    let today = now.to_zoned(TimeZone::system()).date();
    let facts = Facts::load(r, now, std::slice::from_ref(task))?;
    Ok(facts.matches(&expr, task, today))
}

/// Las tareas que cumplen el filtro, en el mismo orden.
pub(super) fn apply(cx: &Cx<'_>, tasks: Vec<Task>, src: &str) -> Result<Vec<Task>, EngineError> {
    let expr = parse(src)?;
    if expr == Expr::And(Vec::new()) {
        return Ok(tasks);
    }
    let today = day(cx);
    let facts = Facts::load(&*cx.tx, cx.now, &tasks)?;
    Ok(tasks
        .into_iter()
        .filter(|t| facts.matches(&expr, t, today))
        .collect())
}

/// Vista por nombre (sin distinguir mayúsculas) o id.
pub(super) fn find(r: &dyn Reader, reference: &str) -> Result<SavedView, EngineError> {
    r.views()?
        .into_iter()
        .find(|v| v.name.eq_ignore_ascii_case(reference) || v.id.to_string() == reference)
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-view").arg("view", reference)))
}

fn view_json(r: &dyn Reader, v: &SavedView) -> Result<Value, EngineError> {
    let project = v
        .project_id
        .map(|p| r.project(p).map(|p| p.map(|p| p.key)))
        .transpose()?
        .flatten();
    Ok(json!({
        "id": v.id,
        "name": v.name,
        "filter": v.filter,
        "layout": v.layout,
        "columns": v.columns,
        "project": project,
        "updated_at": v.updated_at,
    }))
}

fn save(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let name = args.require("name")?.trim().to_owned();
    if name.is_empty() {
        return Err(EngineError::arg("name", Msg::new("view-name-empty")));
    }
    let src = args.require("filter")?;
    parse(src)?;
    let project_id = args
        .get("project")
        .filter(|p| !p.trim().is_empty())
        .map(|p| lookup::project(&*cx.tx, p).map(|p| p.id))
        .transpose()?;
    let existing = find(&*cx.tx, &name).ok();
    let view = SavedView {
        id: existing.as_ref().map_or_else(ViewId::generate, |v| v.id),
        name,
        filter: src.trim().to_owned(),
        layout: args.get("layout").unwrap_or("list").to_owned(),
        columns: args.list("columns"),
        project_id,
        created_at: existing.as_ref().map_or(cx.now, |v| v.created_at),
        updated_at: cx.now,
    };
    cx.tx.save_view(&view)?;
    view_json(&*cx.tx, &view)
}

fn list(cx: &mut Cx<'_>, _args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let items = r
        .views()?
        .iter()
        .map(|v| view_json(r, v))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "count": items.len(), "items": items }))
}

/// La vista y sus tareas, paginadas como `task.list`.
fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let view = find(&*cx.tx, args.require("view")?)?;
    let tasks = cx.tx.tasks(&TaskFilter {
        project: view.project_id,
        ..TaskFilter::default()
    })?;
    let tasks = apply(cx, tasks, &view.filter)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let mut out = paged(tasks, args, |t| views::task_summary(r, &keys, t))?;
    if let Value::Object(map) = &mut out {
        map.insert("view".into(), view_json(r, &view)?);
    }
    Ok(out)
}

fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let view = find(&*cx.tx, args.require("view")?)?;
    cx.tx.delete_view(view.id)?;
    Ok(json!({ "deleted": view.name }))
}

fn mark(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_scope(cx, task.project_id)?;
    let today = day(cx);
    cx.tx.set_today_mark(task.id, Some(today))?;
    Ok(json!({ "task": args.require("task")?, "today": today }))
}

fn unmark(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_scope(cx, task.project_id)?;
    if cx.tx.today_marks()?.contains_key(&task.id) {
        cx.tx.set_today_mark(task.id, None)?;
    }
    Ok(json!({ "task": args.require("task")?, "today": Value::Null }))
}

/// Por qué una tarea está en «Hoy».
fn reason(t: &Task, today: Date, marks: &HashSet<TaskId>) -> Option<&'static str> {
    let due = t.due.as_ref().map(model::When::date);
    match due {
        Some(d) if d < today => Some("overdue"),
        Some(d) if d == today => Some("due"),
        _ if marks.contains(&t.id) => Some("marked"),
        _ => None,
    }
}

/// Atrasadas primero, luego las que vencen hoy y las marcadas.
fn today_list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let today = day(cx);
    let project = args
        .get("project")
        .map(|p| lookup::project(r, p).map(|p| p.id))
        .transpose()?;
    let marks = marked(r, today)?;
    let mut rows: Vec<(&'static str, Task)> = r
        .tasks(&TaskFilter {
            project,
            ..TaskFilter::default()
        })?
        .into_iter()
        .filter(|t| !t.status.category().is_closed())
        .filter_map(|t| reason(&t, today, &marks).map(|why| (why, t)))
        .collect();
    let order = |why: &str| match why {
        "overdue" => 0,
        "due" => 1,
        _ => 2,
    };
    rows.sort_by(|(a, x), (b, y)| {
        order(a)
            .cmp(&order(b))
            .then(x.priority.rank().cmp(&y.priority.rank()))
            .then(
                x.due
                    .as_ref()
                    .map(model::When::date)
                    .cmp(&y.due.as_ref().map(model::When::date)),
            )
    });
    let keys = Keys::load(r)?;
    paged(rows, args, |(why, t)| {
        let mut v = views::task_summary(r, &keys, t)?;
        if let Value::Object(map) = &mut v {
            map.insert("today".into(), json!(why));
        }
        Ok(v)
    })
}
