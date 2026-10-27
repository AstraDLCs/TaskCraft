//! Planificación en lote (RF-COO-11): un árbol de tareas, subtareas y
//! dependencias en una sola operación. Si algo falla, no se crea nada (la
//! transacción del comando se descarta).

use std::collections::HashMap;

use i18n::{Localize, Msg};
use model::{Dependency, Priority, Task, TaskId};
use serde::Deserialize;
use serde_json::{Value, json};

use super::task::{NewTask, insert_new};
use crate::args::parse_when;
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError};

command!(Create, "plan.create", "cmd-plan-create", create, write, [req "project": K::Text, req "items": K::Json, opt "idempotency_key": K::Text]);

/// Un elemento del plan. `ref` es un nombre temporal para referirse a él desde
/// `parent` y `blocked_by` dentro del mismo plan; también valen claves existentes.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Item {
    #[serde(rename = "ref")]
    reference: Option<String>,
    title: String,
    #[serde(default)]
    description: String,
    priority: Option<String>,
    status: Option<String>,
    parent: Option<String>,
    #[serde(default)]
    blocked_by: Vec<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    checklist: Vec<String>,
    #[serde(default)]
    criteria: Vec<String>,
    start: Option<String>,
    due: Option<String>,
    estimate: Option<String>,
    epic: Option<String>,
    milestone: Option<String>,
    #[serde(default)]
    fields: std::collections::BTreeMap<String, String>,
}

fn invalid(detail: impl ToString) -> EngineError {
    EngineError::arg(
        "items",
        Msg::new("plan-invalid").arg("detail", detail.to_string()),
    )
}

fn parse_enum<T>(value: Option<&str>, default: T) -> Result<T, EngineError>
where
    T: std::str::FromStr<Err = model::ParseEnumError>,
{
    match value {
        None => Ok(default),
        Some(v) => v
            .parse()
            .map_err(|e: model::ParseEnumError| EngineError::arg("items", e.msg())),
    }
}

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    let items: Vec<Item> = serde_json::from_str(args.require("items")?).map_err(invalid)?;
    create_items(cx, &project, None, &items)
}

/// Crea un árbol de elementos en `project`; los que no tienen padre cuelgan
/// de `root` si se indica (plantillas instanciadas bajo una tarea).
pub(super) fn create_items(
    cx: &mut Cx<'_>,
    project: &model::Project,
    root: Option<&Task>,
    items: &[Item],
) -> Result<Value, EngineError> {
    let mut refs: HashMap<String, TaskId> = HashMap::new();
    for item in items {
        if let Some(r) = &item.reference
            && refs.insert(r.clone(), TaskId::generate()).is_some()
        {
            return Err(EngineError::invalid(
                Msg::new("plan-duplicate-ref").arg("ref", r),
            ));
        }
    }
    refs.clear();

    // Crear por rondas: un elemento entra cuando su padre ya existe.
    // (índice del elemento, tarea creada), en orden de creación.
    let mut created: Vec<(usize, Task)> = Vec::new();
    let mut pending: Vec<usize> = (0..items.len()).collect();
    while !pending.is_empty() {
        let mut waiting = Vec::new();
        let before = pending.len();
        for index in pending {
            let Some(item) = items.get(index) else {
                continue;
            };
            let parent = match &item.parent {
                None => root.cloned(),
                Some(p) => match refs.get(p) {
                    Some(id) => cx.tx.task(*id)?,
                    None if items.iter().any(|i| i.reference.as_deref() == Some(p)) => {
                        waiting.push(index);
                        continue;
                    }
                    None => Some(lookup::editable_task(&*cx.tx, p)?),
                },
            };
            let new = NewTask {
                description: item.description.clone(),
                status: item.status.clone(),
                priority: parse_enum(item.priority.as_deref(), Priority::Medium)?,
                start: item
                    .start
                    .as_deref()
                    .map(|s| parse_when("start", s))
                    .transpose()?,
                due: item
                    .due
                    .as_deref()
                    .map(|s| parse_when("due", s))
                    .transpose()?,
                labels: item.labels.clone(),
                checklist: item.checklist.clone(),
                criteria: item.criteria.clone(),
                estimate: item.estimate.clone(),
                epic: item.epic.clone(),
                milestone: item.milestone.clone(),
                fields: item.fields.clone().into_iter().collect(),
                ..NewTask::titled(&item.title)
            };
            let task = insert_new(cx, project.id, parent.as_ref(), new)?;
            if let Some(r) = &item.reference {
                refs.insert(r.clone(), task.id);
            }
            created.push((index, task));
        }
        if waiting.len() == before {
            let names: Vec<&str> = waiting
                .iter()
                .filter_map(|i| items.get(*i)?.parent.as_deref())
                .collect();
            return Err(EngineError::invalid(
                Msg::new("plan-unresolved").arg("refs", names.join(", ")),
            ));
        }
        pending = waiting;
    }

    // Dependencias, comprobando ciclos contra todo lo ya existente.
    for (index, task) in &created {
        let Some(item) = items.get(*index) else {
            continue;
        };
        for b in &item.blocked_by {
            let blocker = match refs.get(b) {
                Some(id) => *id,
                None => lookup::task(&*cx.tx, b)?.id,
            };
            let dep = Dependency {
                blocker,
                blocked: task.id,
            };
            if let Some(cycle) = rules::find_cycle(&cx.tx.dependencies()?, dep) {
                let keys = Keys::load(&*cx.tx)?;
                let path = cycle
                    .into_iter()
                    .map(|id| keys.of_id(&*cx.tx, id))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(" -> ");
                return Err(EngineError::invalid(
                    Msg::new("dependency-cycle").arg("path", path),
                ));
            }
            cx.tx.add_dependency(dep)?;
        }
    }

    let keys = Keys::load(&*cx.tx)?;
    let out: Vec<Value> = created
        .iter()
        .map(|(i, t)| json!({ "ref": items.get(*i).and_then(|it| it.reference.clone()), "key": keys.of(t) }))
        .collect();
    Ok(json!({ "count": out.len(), "created": out }))
}
