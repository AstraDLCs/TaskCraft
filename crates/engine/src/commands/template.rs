//! Plantillas de tarea y de proyecto (RF-MOD-16). Una plantilla de tarea es
//! un plan (los mismos elementos que `plan.create`); una de proyecto añade
//! la configuración del proyecto. Un agente puede instanciarla por nombre.

use i18n::Msg;
use model::{
    EstimateUnit, FieldDef, Project, ProjectId, ProjectStatus, StatusDef, Task, Template,
    TemplateId, TemplateKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use store::Reader;

use super::paged;
use super::plan::{Item, create_items};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError};

command!(Create, "template.create", "cmd-template-create", create, write, [req "name": K::Text, req "kind": K::Enum(TemplateKind::NAMES), req "definition": K::Json, opt "description": K::Text]);
command!(FromTask, "template.from_task", "cmd-template-from_task", from_task, write, [req "task": K::Text, req "name": K::Text, opt "description": K::Text]);
command!(List, "template.list", "cmd-template-list", list_templates, read, [opt "kind": K::Enum(TemplateKind::NAMES), opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Get, "template.get", "cmd-template-get", get, read, [req "template": K::Text]);
command!(Delete, "template.delete", "cmd-template-delete", delete, write, [req "template": K::Text]);
command!(Instantiate, "template.instantiate", "cmd-template-instantiate", instantiate, write, [req "template": K::Text, opt "project": K::Text, opt "parent": K::Text, opt "key": K::Text, opt "name": K::Text, opt "idempotency_key": K::Text]);

/// Configuración que trae una plantilla de proyecto.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectPart {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    /// Política de cierre de antes de D20: se acepta al leer y se ignora.
    #[serde(default, rename = "close_policy", skip_serializing)]
    _close_policy: Option<String>,
    estimate_unit: Option<EstimateUnit>,
    #[serde(default)]
    workflow: Vec<StatusDef>,
    #[serde(default)]
    fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    #[serde(default)]
    project: Option<ProjectPart>,
    items: Vec<Item>,
}

fn invalid(detail: impl ToString) -> EngineError {
    EngineError::arg(
        "definition",
        Msg::new("template-invalid").arg("detail", detail.to_string()),
    )
}

fn parse_body(kind: TemplateKind, raw: &str) -> Result<Body, EngineError> {
    let body: Body = serde_json::from_str(raw).map_err(invalid)?;
    if kind == TemplateKind::Task && body.project.is_some() {
        return Err(invalid("a task template has no `project` part"));
    }
    Ok(body)
}

fn view(t: &Template) -> Value {
    let items = serde_json::from_str::<Body>(&t.body)
        .map(|b| b.items.len())
        .unwrap_or(0);
    json!({
        "id": t.id,
        "name": t.name,
        "kind": t.kind,
        "description": t.description,
        "items": items,
        "body": serde_json::from_str::<Value>(&t.body).unwrap_or(Value::Null),
        "updated_at": t.updated_at,
    })
}

fn name(cx: &Cx<'_>, raw: &str) -> Result<String, EngineError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(EngineError::arg(
            "name",
            Msg::new("arg-length").arg("min", 1u32).arg("max", 80u32),
        ));
    }
    if cx.tx.template_by_name(name)?.is_some() {
        return Err(EngineError::invalid(
            Msg::new("template-name-taken").arg("name", name),
        ));
    }
    Ok(name.to_owned())
}

fn save_new(
    cx: &mut Cx<'_>,
    name: String,
    kind: TemplateKind,
    description: &str,
    body: &Body,
) -> Result<Value, EngineError> {
    let template = Template {
        id: TemplateId::generate(),
        name,
        kind,
        description: description.to_owned(),
        body: serde_json::to_string(body).map_err(invalid)?,
        created_at: cx.now,
        updated_at: cx.now,
    };
    cx.tx.insert_template(&template)?;
    Ok(view(&template))
}

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let kind: TemplateKind = args.parse_required("kind")?;
    let body = parse_body(kind, args.require("definition")?)?;
    let name = name(cx, args.require("name")?)?;
    save_new(
        cx,
        name,
        kind,
        args.get("description").unwrap_or_default(),
        &body,
    )
}

/// Captura una tarea con sus subtareas, checklist, criterios, etiquetas y
/// dependencias internas como plantilla de tarea.
fn from_task(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let root = lookup::task(&*cx.tx, args.require("task")?)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let project = r
        .project(root.project_id)?
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?;
    let tree: Vec<Task> = lookup::subtree(r, root.clone())?
        .into_iter()
        .filter(|t| t.deleted_at.is_none())
        .collect();
    let in_tree = |id: model::TaskId| tree.iter().any(|t| t.id == id);
    let mut items = Vec::new();
    for t in &tree {
        let labels = t
            .labels
            .iter()
            .filter_map(|l| r.label(*l).ok().flatten().map(|l| l.name))
            .collect::<Vec<_>>();
        let blocked_by: Vec<String> = r
            .blockers_of(t.id)?
            .into_iter()
            .filter(|b| in_tree(*b))
            .map(|b| keys.of_id(r, b))
            .collect::<Result<_, _>>()?;
        // La raíz de la plantilla no tiene padre: lo pone quien la instancia.
        let parent = if t.id == root.id {
            None
        } else {
            t.parent_id
                .filter(|p| in_tree(*p))
                .map(|p| keys.of_id(r, p))
                .transpose()?
        };
        let status = match (&t.stage, t.status) {
            (Some(stage), _) => Some(stage.clone()),
            (None, model::Status::Backlog) => Some("backlog".to_owned()),
            _ => None,
        };
        items.push(json!({
            "ref": keys.of(t),
            "title": t.title,
            "description": t.description,
            "priority": t.priority,
            "status": status,
            "parent": parent,
            "blocked_by": blocked_by,
            "labels": labels,
            "checklist": t.checklist.iter().map(|i| i.text.clone()).collect::<Vec<_>>(),
            "criteria": t.criteria.iter().map(|i| i.text.clone()).collect::<Vec<_>>(),
            "estimate": t.estimate.map(|e| rules::format_estimate(project.estimate_unit, e)),
            "fields": t.fields,
        }));
    }
    let body: Body = serde_json::from_value(json!({ "items": items })).map_err(invalid)?;
    let name = name(cx, args.require("name")?)?;
    save_new(
        cx,
        name,
        TemplateKind::Task,
        args.get("description").unwrap_or_default(),
        &body,
    )
}

fn find(cx: &Cx<'_>, reference: &str) -> Result<Template, EngineError> {
    let found = match reference.parse::<TemplateId>() {
        Ok(id) => cx.tx.template(id)?,
        Err(_) => cx.tx.template_by_name(reference)?,
    };
    found.ok_or_else(|| {
        EngineError::not_found(Msg::new("notfound-template").arg("template", reference))
    })
}

fn list_templates(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let kind: Option<TemplateKind> = args.parse("kind")?;
    let templates: Vec<Template> = cx
        .tx
        .templates()?
        .into_iter()
        .filter(|t| kind.is_none_or(|k| t.kind == k))
        .collect();
    paged(templates, args, |t| Ok(view(t)))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    Ok(view(&find(cx, args.require("template")?)?))
}

fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let t = find(cx, args.require("template")?)?;
    cx.tx.delete_template(t.id)?;
    Ok(json!({ "deleted": t.name }))
}

/// Tarea: crea el árbol en `project` (o bajo `parent`). Proyecto: crea el
/// proyecto `key` con su configuración y luego sus tareas.
fn instantiate(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let template = find(cx, args.require("template")?)?;
    let body = parse_body(template.kind, &template.body)?;
    let parent = args
        .get("parent")
        .map(|p| lookup::editable_task(&*cx.tx, p))
        .transpose()?;
    let project = match template.kind {
        TemplateKind::Task => match (args.get("project"), &parent) {
            (Some(p), _) => lookup::project(&*cx.tx, p)?,
            (None, Some(parent)) => cx
                .tx
                .project(parent.project_id)?
                .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?,
            (None, None) => return Err(EngineError::MissingArg("project".into())),
        },
        TemplateKind::Project => new_project(cx, args, body.project.clone().unwrap_or_default())?,
    };
    let mut out = create_items(cx, &project, parent.as_ref(), &body.items)?;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("project".into(), json!(project.key));
        obj.insert("template".into(), json!(template.name));
    }
    Ok(out)
}

fn new_project(cx: &mut Cx<'_>, args: &Args, part: ProjectPart) -> Result<Project, EngineError> {
    let key = rules::project_key(args.require("key")?)?;
    if cx.tx.project_by_key(&key)?.is_some() {
        return Err(EngineError::invalid(
            Msg::new("project-key-taken").arg("key", &key),
        ));
    }
    let name = args
        .get("name")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .or_else(|| (!part.name.is_empty()).then(|| part.name.clone()))
        .unwrap_or_else(|| key.clone());
    let project = Project {
        id: ProjectId::generate(),
        key,
        name,
        description: part.description,
        status: ProjectStatus::Active,
        next_number: 1,
        next_diagram: 1,
        created_at: cx.now,
        updated_at: cx.now,
        locked: false,
        workflow: part.workflow,
        estimate_unit: part.estimate_unit.unwrap_or(EstimateUnit::Hours),
        fields: part.fields,
    };
    cx.tx.insert_project(&project)?;
    Ok(project)
}
