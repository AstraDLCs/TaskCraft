//! Flujo de cada proyecto (D10, RF-MOD-07) y campos personalizados
//! (RF-MOD-15). Las columnas las crea cualquier agente, también para sí
//! mismo o para otro (D20); los campos, el supervisor; rellenar un campo,
//! quien puede escribir la tarea.

use i18n::Msg;
use model::{FieldKind, Project, Status, StatusDef, TaskId};
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::{detail, list, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup, views};

command!(StatusList, "status.list", "cmd-status-list", status_list, read, [req "project": K::Text]);
command!(StatusCreate, "status.create", "cmd-status-create", status_create, write, [req "project": K::Text, req "name": K::Text, req "base": K::Enum(Status::NAMES), opt "owner": K::Text, opt "position": K::Int]);
command!(StatusUpdate, "status.update", "cmd-status-update", status_update, write, [req "project": K::Text, req "status": K::Text, opt "name": K::Text, opt "owner": K::Text, opt "position": K::Int, opt "wip": K::Int, opt "wip_reject": K::Bool]);
command!(StatusDelete, "status.delete", "cmd-status-delete", status_delete, write, [req "project": K::Text, req "status": K::Text]);
command!(FieldList, "field.list", "cmd-field-list", field_list, read, [req "project": K::Text]);
command!(FieldCreate, "field.create", "cmd-field-create", field_create, write, [req "project": K::Text, req "name": K::Text, req "kind": K::Enum(FieldKind::NAMES), opt "options": K::List]);
command!(FieldUpdate, "field.update", "cmd-field-update", field_update, write, [req "project": K::Text, req "field": K::Text, opt "name": K::Text, opt "options": K::List]);
command!(FieldDelete, "field.delete", "cmd-field-delete", field_delete, write, [req "project": K::Text, req "field": K::Text]);
command!(SetField, "task.set_field", "cmd-task-set_field", set_field, write, [req "task": K::Text, req "field": K::Text, req "value": K::Text, opt "revision": K::Int]);

/// Tareas vivas del proyecto en cada clave del flujo.
fn counts(r: &dyn Reader, project: &Project) -> Result<Vec<(String, usize)>, EngineError> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for t in r.tasks(&TaskFilter::project(project.id))? {
        let key = t
            .stage
            .clone()
            .unwrap_or_else(|| t.status.as_str().to_owned());
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => out.push((key, 1)),
        }
    }
    Ok(out)
}

fn status_view(r: &dyn Reader, d: &StatusDef, tasks: usize) -> Result<Value, EngineError> {
    let owner = d
        .owner
        .map(|a| views::actor_name(r, &model::Actor::Agent(a)))
        .transpose()?;
    Ok(json!({
        "key": d.key,
        "name": d.name,
        "base": d.base,
        "category": d.base.category(),
        "is_base": d.is_base(),
        "tasks": tasks,
        "wip_limit": d.wip_limit,
        "wip_reject": d.wip_reject,
        "over_limit": d.wip_limit.is_some_and(|l| tasks > l as usize),
        "owner": owner,
    }))
}

fn flow_view(r: &dyn Reader, project: &Project) -> Result<Value, EngineError> {
    let counts = counts(r, project)?;
    let items = rules::workflow(project)
        .iter()
        .map(|d| {
            let n = counts
                .iter()
                .find(|(k, _)| *k == d.key)
                .map_or(0, |(_, n)| *n);
            status_view(r, d, n)
        })
        .collect::<Result<_, _>>()?;
    Ok(list(items))
}

fn status_list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    flow_view(&*cx.tx, &project)
}

/// Posición 1..=n del flujo (fuera de rango = al final).
fn place(flow: &mut Vec<StatusDef>, def: StatusDef, position: Option<i64>) {
    let at = position
        .and_then(|p| usize::try_from(p.saturating_sub(1)).ok())
        .filter(|p| *p <= flow.len())
        .unwrap_or(flow.len());
    flow.insert(at, def);
}

fn save_flow(
    cx: &mut Cx<'_>,
    mut project: Project,
    flow: Vec<StatusDef>,
) -> Result<Value, EngineError> {
    project.workflow = flow;
    project.updated_at = cx.now;
    cx.tx.update_project(&project)?;
    flow_view(&*cx.tx, &project)
}

fn status_create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    let name = args.require("name")?.trim().to_owned();
    let key = rules::stage_key(&project, &name)?;
    let base: Status = args.parse_required("base")?;
    let mut flow = rules::workflow(&project);
    // Por defecto, justo después de la última columna de su estado base.
    let default_at = flow
        .iter()
        .rposition(|d| d.base == base)
        .map(|i| i64::try_from(i + 2).unwrap_or(i64::MAX));
    let position = args.parse::<i64>("position")?.or(default_at);
    let mut def = StatusDef::new(key, name, base);
    def.owner = owner(&*cx.tx, args)?.flatten();
    place(&mut flow, def, position);
    save_flow(cx, project, flow)
}

/// `owner=`: un agente por nombre; vacío o `none` lo quita.
fn owner(r: &dyn Reader, args: &Args) -> Result<Option<Option<model::AgentId>>, EngineError> {
    args.clearable("owner", |name| lookup::agent(r, name).map(|a| a.id))
}

fn find(flow: &[StatusDef], reference: &str) -> Result<usize, EngineError> {
    let wanted = rules::slug(reference);
    flow.iter()
        .position(|d| d.key == wanted || rules::slug(&d.name) == wanted)
        .ok_or_else(|| {
            EngineError::not_found(
                Msg::new("notfound-status").arg("status", reference).arg(
                    "valid",
                    flow.iter()
                        .map(|d| d.key.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            )
        })
}

fn status_update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    let mut flow = rules::workflow(&project);
    let index = find(&flow, args.require("status")?)?;
    let mut def = flow.remove(index);
    if let Some(name) = args.get("name") {
        let name = name.trim();
        if name.chars().count() > 40 || (name.is_empty() && !def.is_base()) {
            return Err(EngineError::arg(
                "name",
                Msg::new("arg-length").arg("min", 1u32).arg("max", 40u32),
            ));
        }
        def.name = name.to_owned();
    }
    if let Some(wip) = args.parse::<u32>("wip")? {
        def.wip_limit = (wip > 0).then_some(wip);
    }
    if args.get("wip_reject").is_some() {
        def.wip_reject = args.flag("wip_reject")?;
    }
    if let Some(owner) = owner(&*cx.tx, args)? {
        def.owner = owner;
    }
    let position = args
        .parse::<i64>("position")?
        .or_else(|| i64::try_from(index + 1).ok());
    place(&mut flow, def, position);
    save_flow(cx, project, flow)
}

/// Solo etapas: sus tareas se quedan en el estado base.
fn status_delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    let mut flow = rules::workflow(&project);
    let index = find(&flow, args.require("status")?)?;
    let def = flow.remove(index);
    if def.is_base() {
        return Err(EngineError::invalid(
            Msg::new("status-base-permanent").arg("status", &def.key),
        ));
    }
    let affected: Vec<TaskId> = cx
        .tx
        .tasks(&TaskFilter {
            include_archived: true,
            ..TaskFilter::project(project.id)
        })?
        .into_iter()
        .filter(|t| t.stage.as_deref() == Some(def.key.as_str()))
        .map(|t| t.id)
        .collect();
    let out = save_flow(cx, project, flow)?;
    for id in affected {
        if let Some(mut t) = cx.tx.task(id)? {
            t.stage = None;
            save(cx, &mut t)?;
        }
    }
    Ok(out)
}

fn field_view(project: &Project) -> Value {
    list(
        project
            .fields
            .iter()
            .map(|f| serde_json::to_value(f).unwrap_or_default())
            .collect(),
    )
}

fn field_list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    Ok(field_view(&project))
}

fn save_fields(cx: &mut Cx<'_>, mut project: Project) -> Result<Value, EngineError> {
    project.updated_at = cx.now;
    cx.tx.update_project(&project)?;
    Ok(field_view(&project))
}

fn field_create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut project = lookup::project(&*cx.tx, args.require("project")?)?;
    let def = rules::field_def(
        &project,
        args.require("name")?,
        args.parse_required("kind")?,
        args.list("options"),
    )?;
    project.fields.push(def);
    save_fields(cx, project)
}

fn field_update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut project = lookup::project(&*cx.tx, args.require("project")?)?;
    let key = rules::find_field(&project, args.require("field")?)?
        .key
        .clone();
    let name = args.get("name").map(str::trim).map(str::to_owned);
    let options = match args.get("options") {
        Some(_) => {
            let kind = rules::find_field(&project, &key)?.kind;
            Some(rules::field_options(kind, args.list("options"))?)
        }
        None => None,
    };
    if let Some(def) = project.fields.iter_mut().find(|f| f.key == key) {
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            def.name = name;
        }
        if let Some(options) = options {
            def.options = options;
        }
    }
    save_fields(cx, project)
}

/// Quita el campo y sus valores de todas las tareas del proyecto.
fn field_delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut project = lookup::project(&*cx.tx, args.require("project")?)?;
    let key = rules::find_field(&project, args.require("field")?)?
        .key
        .clone();
    project.fields.retain(|f| f.key != key);
    let filter = TaskFilter {
        include_archived: true,
        ..TaskFilter::project(project.id)
    };
    let out = save_fields(cx, project)?;
    for mut t in cx.tx.tasks(&filter)? {
        if t.fields.remove(&key).is_some() {
            save(cx, &mut t)?;
        }
    }
    Ok(out)
}

/// Valor vacío = quitarlo.
fn set_field(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let project = cx
        .tx
        .project(task.project_id)?
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?;
    let def = rules::find_field(&project, args.require("field")?)?;
    let value = rules::field_value(def, args.require("value")?)?;
    if value.is_empty() {
        task.fields.remove(&def.key);
    } else {
        task.fields.insert(def.key.clone(), value);
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

/// Vista del proyecto con lo que define (flujo, unidad y campos).
pub(super) fn project_full(r: &dyn Reader, project: &Project) -> Result<Value, EngineError> {
    let mut view = views::project(project);
    if let Some(obj) = view.as_object_mut() {
        let flow = flow_view(r, project)?;
        obj.insert(
            "workflow".into(),
            flow.get("items").cloned().unwrap_or_default(),
        );
        obj.insert("estimate_unit".into(), json!(project.estimate_unit));
        obj.insert("fields".into(), json!(project.fields));
    }
    Ok(view)
}
