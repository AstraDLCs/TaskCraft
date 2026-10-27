use std::collections::BTreeMap;

use i18n::Msg;
use model::{Project, ProjectId, ProjectStatus};
use serde_json::{Value, json};
use store::TaskFilter;

use super::paged;
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup, views};

command!(Create, "project.create", "cmd-project-create", create, write, [req "key": K::Text, opt "name": K::Text, opt "description": K::Text, opt "estimate_unit": K::Enum(model::EstimateUnit::NAMES), opt "idempotency_key": K::Text]);
command!(List, "project.list", "cmd-project-list", list_projects, read, [opt "archived": K::Bool, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Get, "project.get", "cmd-project-get", get, read, [req "project": K::Text]);
command!(Update, "project.update", "cmd-project-update", update, write, [req "project": K::Text, opt "name": K::Text, opt "description": K::Text, opt "estimate_unit": K::Enum(model::EstimateUnit::NAMES)]);
command!(Archive, "project.archive", "cmd-project-archive", archive, write, [req "project": K::Text]);
command!(Unarchive, "project.unarchive", "cmd-project-unarchive", unarchive, write, [req "project": K::Text]);

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let key = rules::project_key(args.require("key")?)?;
    if cx.tx.project_by_key(&key)?.is_some() {
        return Err(EngineError::invalid(
            Msg::new("project-key-taken").arg("key", &key),
        ));
    }
    let project = Project {
        id: ProjectId::generate(),
        name: args.get("name").map(str::trim).unwrap_or(&key).to_owned(),
        key,
        description: args.get("description").unwrap_or_default().to_owned(),
        status: ProjectStatus::Active,
        next_number: 1,
        next_diagram: 1,
        created_at: cx.now,
        updated_at: cx.now,
        locked: false,
        workflow: Vec::new(),
        estimate_unit: args
            .parse("estimate_unit")?
            .unwrap_or(model::EstimateUnit::Hours),
        fields: Vec::new(),
    };
    cx.tx.insert_project(&project)?;
    Ok(views::project(&project))
}

fn list_projects(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let projects = cx.tx.projects(args.flag("archived")?)?;
    paged(projects, args, |p| Ok(views::project(p)))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for t in cx.tx.tasks(&TaskFilter::project(project.id))? {
        *counts.entry(t.status.as_str()).or_default() += 1;
    }
    let mut view = super::workflow::project_full(&*cx.tx, &project)?;
    if let Some(obj) = view.as_object_mut() {
        obj.insert("tasks".into(), json!(counts));
    }
    Ok(view)
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut project = lookup::project(&*cx.tx, args.require("project")?)?;
    if let Some(name) = args.get("name") {
        let name = name.trim();
        if name.is_empty() {
            return Err(EngineError::arg("name", Msg::new("arg-empty")));
        }
        project.name = name.to_owned();
    }
    if let Some(description) = args.get("description") {
        project.description = description.to_owned();
    }
    if let Some(unit) = args.parse("estimate_unit")? {
        project.estimate_unit = unit;
    }
    project.updated_at = cx.now;
    cx.tx.update_project(&project)?;
    Ok(views::project(&project))
}

fn set_status(cx: &mut Cx<'_>, args: &Args, status: ProjectStatus) -> Result<Value, EngineError> {
    let mut project = lookup::project(&*cx.tx, args.require("project")?)?;
    project.status = status;
    project.updated_at = cx.now;
    cx.tx.update_project(&project)?;
    Ok(views::project(&project))
}

fn archive(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_status(cx, args, ProjectStatus::Archived)
}

fn unarchive(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_status(cx, args, ProjectStatus::Active)
}
