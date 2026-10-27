//! Comandos incluidos de serie, por dominio.

mod activity;
mod agent;
mod aging;
mod attachment;
mod automation;
mod coord;
mod cost;
mod cycle;
mod data;
mod deps;
mod diagram;
mod epic;
mod exchange;
mod git;
mod graph;
mod inbox;
mod interface;
mod items;
mod labels;
mod lifecycle;
mod plan;
mod project;
pub(crate) mod recurrence;
mod revert;
mod session;
mod task;
mod template;
mod time;
mod view;

pub(crate) use view::task_matches;
pub use workspace::WEB_TOKEN_FILE;
mod workflow;
mod workspace;

use i18n::Msg;
use model::{LabelId, Task};
use serde_json::{Value, json};
use store::Reader;

use crate::lookup::Keys;
use crate::{Cx, EngineError, Registry, views};

pub(crate) fn register(r: &mut Registry) {
    r.register(workspace::Get);
    r.register(workspace::Configure);
    r.register(workspace::WebToken);
    r.register(exchange::Export);
    r.register(exchange::ImportPreview);
    r.register(exchange::Import);
    r.register(exchange::Backup);
    r.register(data::ProjectDelete);
    r.register(data::WorkspaceWipe);
    r.register(data::ActivityPrune);
    r.register(exchange::ExportMarkdown);
    r.register(exchange::ExportCsv);

    r.register(project::Create);
    r.register(project::List);
    r.register(project::Get);
    r.register(project::Update);
    r.register(project::Archive);
    r.register(project::Unarchive);

    r.register(task::Create);
    r.register(task::Get);
    r.register(task::List);
    r.register(task::Update);
    r.register(task::Move);
    r.register(task::SetParent);
    r.register(task::Assign);
    r.register(task::AddAssignee);
    r.register(task::RemoveAssignee);
    r.register(task::Label);
    r.register(task::Search);
    r.register(items::TaskCheck);

    r.register(items::ChecklistAdd);
    r.register(items::ChecklistCheck);
    r.register(items::ChecklistRemove);
    r.register(items::CriteriaAdd);
    r.register(items::CriteriaCheck);
    r.register(items::CriteriaRemove);
    r.register(items::ChecklistEdit);
    r.register(items::CriteriaEdit);

    r.register(labels::Create);
    r.register(labels::List);
    r.register(labels::Update);

    r.register(deps::Link);
    r.register(deps::Unlink);
    r.register(deps::Relate);
    r.register(deps::Unrelate);

    r.register(workflow::StatusList);
    r.register(workflow::StatusCreate);
    r.register(workflow::StatusUpdate);
    r.register(workflow::StatusDelete);
    r.register(workflow::FieldList);
    r.register(workflow::FieldCreate);
    r.register(workflow::FieldUpdate);
    r.register(workflow::FieldDelete);
    r.register(workflow::SetField);

    r.register(epic::Create);
    r.register(epic::Update);
    r.register(epic::Close);
    r.register(epic::Reopen);
    r.register(epic::Delete);
    r.register(epic::List);
    r.register(epic::Get);
    r.register(epic::SetEpic);
    r.register(epic::SetMilestone);

    r.register(template::Create);
    r.register(template::FromTask);
    r.register(template::List);
    r.register(template::Get);
    r.register(template::Delete);
    r.register(template::Instantiate);

    r.register(attachment::Add);
    r.register(attachment::List);
    r.register(attachment::Get);
    r.register(attachment::Remove);

    r.register(time::TimerStart);
    r.register(time::TimerStop);
    r.register(time::Log);
    r.register(time::Report);
    r.register(time::Sheet);
    r.register(time::Burndown);
    r.register(time::Cfd);
    r.register(time::Agents);
    r.register(cost::CostReport);
    r.register(cost::CostList);
    r.register(cost::CostSummary);
    r.register(git::GitLinkCmd);
    r.register(git::GitUnlink);
    r.register(git::GitLinks);
    r.register(git::GitIngest);
    r.register(recurrence::SetRecurrence);
    r.register(recurrence::ClearRecurrence);
    r.register(aging::Stale);
    r.register(aging::Someday);
    r.register(graph::ProjectGraph);
    r.register(graph::CriticalPath);
    r.register(graph::Matrix);
    r.register(cycle::CycleCreate);
    r.register(cycle::CycleList);
    r.register(cycle::CycleGet);
    r.register(cycle::CycleUpdate);
    r.register(cycle::CycleClose);
    r.register(cycle::CycleDelete);
    r.register(cycle::CycleRollover);
    r.register(cycle::SetCycle);
    r.register(cycle::AddCycle);
    r.register(cycle::RemoveCycle);
    r.register(diagram::Create);
    r.register(diagram::List);
    r.register(diagram::Get);
    r.register(diagram::Update);
    r.register(diagram::Delete);

    r.register(automation::Create);
    r.register(automation::List);
    r.register(automation::Update);
    r.register(automation::Delete);
    r.register(automation::ReviewParent);
    r.register(automation::WebhookCreate);
    r.register(automation::WebhookList);
    r.register(automation::WebhookDelete);
    r.register(automation::Deliveries);
    r.register(automation::Record);

    r.register(view::Save);
    r.register(view::List);
    r.register(view::Get);
    r.register(view::Delete);
    r.register(view::Mark);
    r.register(view::Unmark);
    r.register(view::Today);

    r.register(lifecycle::Archive);
    r.register(lifecycle::Unarchive);
    r.register(lifecycle::AutoArchive);
    r.register(lifecycle::Delete);
    r.register(lifecycle::Restore);
    r.register(lifecycle::TrashList);
    r.register(lifecycle::TrashPurge);

    r.register(interface::Register);
    r.register(interface::List);
    r.register(interface::Get);
    r.register(interface::Update);
    r.register(interface::Pause);
    r.register(interface::Resume);
    r.register(interface::Disable);
    r.register(interface::Enable);
    r.register(interface::Revoke);
    r.register(interface::RotateToken);
    r.register(interface::Token);

    r.register(agent::Register);
    r.register(agent::Create);
    r.register(agent::Mine);
    r.register(agent::List);
    r.register(agent::Get);
    r.register(agent::Update);
    r.register(agent::Disable);
    r.register(agent::Enable);
    r.register(agent::Revoke);
    r.register(agent::Pause);
    r.register(agent::Resume);

    r.register(session::Open);
    r.register(session::Heartbeat);
    r.register(session::Close);

    r.register(activity::ActivityList);
    r.register(activity::CommentAdd);
    r.register(activity::CommentList);
    r.register(activity::Summary);
    r.register(revert::Revert);

    r.register(coord::Take);
    r.register(coord::Heartbeat);
    r.register(coord::Release);
    r.register(coord::ExpireLeases);
    r.register(coord::Next);
    r.register(coord::SubmitReview);
    r.register(coord::Escalate);
    r.register(coord::Answer);
    r.register(coord::Reject);
    r.register(coord::Housekeeping);
    r.register(coord::LockSet);
    r.register(coord::LockClear);

    r.register(plan::Create);

    r.register(inbox::Add);
    r.register(inbox::List);
    r.register(inbox::Convert);
    r.register(inbox::Discard);
}

/// Registra un cambio en la tarea (revisión y fecha) y lo guarda. Si el
/// estado base cambió y la etapa ya no le corresponde, la etapa se limpia (D10).
fn save(cx: &mut Cx<'_>, task: &mut Task) -> Result<(), EngineError> {
    task.revision = task.revision.saturating_add(1);
    task.updated_at = cx.now;
    crate::guard::renew_if_claimer(cx, task)?;
    let project = cx.tx.project(task.project_id)?;
    if let Some(stage) = &task.stage {
        let fits = project.as_ref().is_some_and(|p| {
            rules::workflow(p)
                .iter()
                .any(|d| &d.key == stage && d.base == task.status)
        });
        if !fits {
            task.stage = None;
        }
    }
    if let Some(project) = &project {
        check_wip(cx, project, task)?;
    }
    cx.tx.update_task(task)?;
    Ok(())
}

/// Columna del flujo en la que está una tarea (su etapa o su estado base).
fn column_of(task: &Task) -> String {
    task.stage
        .clone()
        .unwrap_or_else(|| task.status.as_str().to_owned())
}

/// Límite WIP de la columna a la que entra la tarea (RF-COO-08): con
/// `wip_reject` se rechaza la entrada si ya está llena; si no, solo se avisa
/// en la vista.
pub(super) fn check_wip(
    cx: &Cx<'_>,
    project: &model::Project,
    task: &Task,
) -> Result<(), EngineError> {
    let column = column_of(task);
    let Some(def) = rules::workflow(project)
        .into_iter()
        .find(|d| d.key == column)
        .filter(|d| d.wip_reject && d.wip_limit.is_some())
    else {
        return Ok(());
    };
    let before = cx.tx.task(task.id)?;
    if before.as_ref().is_some_and(|b| column_of(b) == column) {
        return Ok(());
    }
    let limit = def.wip_limit.unwrap_or(u32::MAX);
    let filter = store::TaskFilter {
        statuses: vec![def.base],
        ..store::TaskFilter::project(project.id)
    };
    let inside = cx
        .tx
        .tasks(&filter)?
        .iter()
        .filter(|t| t.id != task.id && column_of(t) == column)
        .count();
    if u32::try_from(inside).unwrap_or(u32::MAX) >= limit {
        return Err(EngineError::invalid(
            Msg::new("wip-full")
                .arg(
                    "column",
                    if def.name.is_empty() {
                        def.key.clone()
                    } else {
                        def.name.clone()
                    },
                )
                .arg("limit", limit),
        ));
    }
    Ok(())
}

/// Vuelve a calcular las menciones `[[API-42]]` (RF-REL-04) y las citas de
/// diagramas `[[API-D3]]` / `![[API-D3]]` (RF-MOD-19) de un texto: la
/// descripción de `task` (`comment = None`) o uno de sus comentarios. Las
/// claves que no existen se ignoran.
fn sync_mentions(
    cx: &mut Cx<'_>,
    task: model::TaskId,
    comment: Option<model::CommentId>,
    text: &str,
) -> Result<(), EngineError> {
    let mut targets = Vec::new();
    for key in rules::mentions(text) {
        let Some(project) = cx.tx.project_by_key(&key.project)? else {
            continue;
        };
        if let Some(t) = cx.tx.task_by_number(project.id, key.number)?
            && t.id != task
            && !targets.contains(&t.id)
        {
            targets.push(t.id);
        }
    }
    cx.tx.set_mentions(task, comment, &targets)?;
    let mut diagrams = Vec::new();
    for key in rules::diagram_mentions(text) {
        let Some(project) = cx.tx.project_by_key(&key.project)? else {
            continue;
        };
        if let Some(d) = cx.tx.diagram_by_number(project.id, key.number)?
            && !diagrams.contains(&d.id)
        {
            diagrams.push(d.id);
        }
    }
    cx.tx.set_diagram_mentions(task, comment, &diagrams)?;
    Ok(())
}

/// Detalle de la tarea tal como quedó, como resultado del comando.
fn detail(cx: &Cx<'_>, task: &Task) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    views::task_detail(r, &Keys::load(r)?, task)
}

fn list(items: Vec<Value>) -> Value {
    json!({ "count": items.len(), "items": items })
}

/// Tamaño de página por defecto y máximo de los listados (RF-MCP-08).
const PAGE_DEFAULT: usize = 50;
const PAGE_MAX: usize = 500;

/// Listado paginado con selección de campos (RF-MCP-08): `limit`, `cursor`
/// (opaco; hoy, la posición) y `fields`. Solo se construyen las vistas de la
/// página pedida.
fn paged<T>(
    items: Vec<T>,
    args: &crate::Args,
    view: impl Fn(&T) -> Result<Value, EngineError>,
) -> Result<Value, EngineError> {
    let limit = args
        .parse::<usize>("limit")?
        .unwrap_or(PAGE_DEFAULT)
        .clamp(1, PAGE_MAX);
    let offset = args.parse::<usize>("cursor")?.unwrap_or(0);
    let fields = args.list("fields");
    let total = items.len();
    let page = items
        .iter()
        .skip(offset)
        .take(limit)
        .map(|item| view(item).map(|v| select_fields(v, &fields)))
        .collect::<Result<Vec<_>, _>>()?;
    let end = offset.saturating_add(page.len());
    Ok(json!({
        "count": page.len(),
        "total": total,
        "items": page,
        "next_cursor": (end < total).then(|| end.to_string()),
    }))
}

/// Solo los campos pedidos; sin lista, el objeto entero.
fn select_fields(value: Value, fields: &[String]) -> Value {
    match value {
        Value::Object(map) if !fields.is_empty() => Value::Object(
            map.into_iter()
                .filter(|(k, _)| fields.iter().any(|f| f == k))
                .collect(),
        ),
        other => other,
    }
}

/// Etiqueta por nombre, creándola con un color por defecto si no existe.
fn ensure_label(cx: &mut Cx<'_>, raw: &str) -> Result<LabelId, EngineError> {
    let name = rules::label_name(raw)?;
    if let Some(l) = cx.tx.label_by_name(&name)? {
        return Ok(l.id);
    }
    let label = model::Label {
        id: LabelId::generate(),
        color: rules::default_label_color(&name),
        name,
    };
    cx.tx.insert_label(&label)?;
    Ok(label.id)
}

fn days_ago(cx: &Cx<'_>, days: u32) -> Result<model::Timestamp, EngineError> {
    cx.now
        .checked_sub(jiff::SignedDuration::from_hours(i64::from(days) * 24))
        .map_err(|_| EngineError::arg("days", Msg::new("arg-out-of-range").arg("value", days)))
}
