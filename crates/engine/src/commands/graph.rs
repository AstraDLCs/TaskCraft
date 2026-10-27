//! Grafo de dependencias (RF-REL-05), ruta crítica (RF-REL-06) y matriz de
//! priorización (RF-VIS-15) de un proyecto.

use std::collections::HashSet;

use model::{Status, Task};
use rules::graph;
use rules::matrix::{Quadrant, quadrant};
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError};

command!(ProjectGraph, "project.graph", "cmd-project-graph", project_graph, read, [req "project": K::Text, opt "closed": K::Bool, opt "format": K::Enum(&["json", "dot"])]);
command!(CriticalPath, "project.critical_path", "cmd-project-critical_path", critical_path, read, [req "project": K::Text]);
command!(Matrix, "task.matrix", "cmd-task-matrix", matrix, read, [opt "project": K::Text]);

/// Tareas vivas del proyecto; las cerradas solo si se piden.
fn tasks_of(
    r: &dyn Reader,
    project: model::ProjectId,
    closed: bool,
) -> Result<Vec<Task>, EngineError> {
    Ok(r.tasks(&TaskFilter::project(project))?
        .into_iter()
        .filter(|t| closed || !t.status.category().is_closed())
        .collect())
}

fn project_graph(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = lookup::project(r, args.require("project")?)?;
    let closed = args.flag("closed")?;
    let tasks = tasks_of(r, project.id, closed)?;
    let deps = r.dependencies()?;
    let keys = Keys::load(r)?;
    let layers = graph::layers(&tasks, &deps);
    // La ruta crítica se calcula siempre sobre lo que queda por hacer.
    let open: Vec<Task> = tasks
        .iter()
        .filter(|t| !t.status.category().is_closed())
        .cloned()
        .collect();
    let (path, _) = graph::critical_path(&open, &deps);
    let on_path: HashSet<_> = path.iter().copied().collect();
    let ids: HashSet<_> = tasks.iter().map(|t| t.id).collect();
    let critical_edge = |a: model::TaskId, b: model::TaskId| {
        path.windows(2)
            .any(|w| w.first() == Some(&a) && w.get(1) == Some(&b))
    };
    let edges: Vec<(model::TaskId, model::TaskId)> = deps
        .iter()
        .filter(|d| ids.contains(&d.blocker) && ids.contains(&d.blocked))
        .map(|d| (d.blocker, d.blocked))
        .collect();
    if args.get("format") == Some("dot") {
        let nodes: Vec<(String, String, bool)> = tasks
            .iter()
            .map(|t| (keys.of(t), t.title.clone(), on_path.contains(&t.id)))
            .collect();
        let lines: Vec<(String, String, bool)> = edges
            .iter()
            .map(|(a, b)| {
                Ok((
                    keys.of_id(r, *a)?,
                    keys.of_id(r, *b)?,
                    critical_edge(*a, *b),
                ))
            })
            .collect::<Result<_, EngineError>>()?;
        return Ok(json!({ "dot": graph::dot(&project.key, &nodes, &lines) }));
    }
    let nodes: Vec<Value> = tasks
        .iter()
        .map(|t| {
            json!({
                "key": keys.of(t),
                "title": t.title,
                "status": t.status,
                "priority": t.priority,
                "layer": layers.get(&t.id).copied().unwrap_or(0),
                "critical": on_path.contains(&t.id),
            })
        })
        .collect();
    let edges: Vec<Value> = edges
        .iter()
        .map(|(a, b)| {
            Ok(json!({
                "from": keys.of_id(r, *a)?,
                "to": keys.of_id(r, *b)?,
                "critical": critical_edge(*a, *b),
            }))
        })
        .collect::<Result<_, EngineError>>()?;
    Ok(json!({ "project": project.key, "nodes": nodes, "edges": edges }))
}

fn critical_path(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = lookup::project(r, args.require("project")?)?;
    let open = tasks_of(r, project.id, false)?;
    let deps = r.dependencies()?;
    let keys = Keys::load(r)?;
    let (path, total) = graph::critical_path(&open, &deps);
    let mut tasks = Vec::new();
    for id in &path {
        if let Some(t) = open.iter().find(|t| t.id == *id) {
            let (w, basis) = graph::weight(t);
            tasks.push(json!({
                "key": keys.of(t),
                "title": t.title,
                "status": t.status,
                "weight": w,
                "basis": basis.key(),
                "due": t.due,
            }));
        }
    }
    Ok(json!({
        "project": project.key,
        "total": total,
        "estimate_unit": project.estimate_unit,
        "tasks": tasks,
    }))
}

/// Tareas abiertas por cuadrante de Eisenhower (RF-VIS-15).
fn matrix(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let filter = match args.get("project") {
        Some(p) => TaskFilter::project(lookup::project(r, p)?.id),
        None => TaskFilter::default(),
    };
    let urgent_days = r.workspace()?.settings.urgent_days;
    let today = super::time::day_of(cx.now);
    let keys = Keys::load(r)?;
    let tasks: Vec<Task> = r
        .tasks(&filter)?
        .into_iter()
        .filter(|t| !t.status.category().is_closed() && t.status != Status::Backlog)
        .collect();
    let quadrants: Vec<Value> = Quadrant::ALL
        .iter()
        .map(|q| {
            let items: Vec<Value> = tasks
                .iter()
                .filter(|t| quadrant(t, today, urgent_days) == *q)
                .map(|t| json!({ "key": keys.of(t), "title": t.title, "priority": t.priority, "due": t.due }))
                .collect();
            json!({ "quadrant": q.key(), "important": q.important(), "urgent": q.urgent(), "items": items })
        })
        .collect();
    Ok(json!({ "urgent_days": urgent_days, "quadrants": quadrants }))
}
