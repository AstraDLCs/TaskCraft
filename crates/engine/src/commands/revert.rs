//! Revertir desde el historial (RF-SUP-07): deshace una entrada concreta, o
//! todo lo que hizo un mismo comando, sin tocar los cambios posteriores de
//! otros actores. Si un campo cambió después, es un conflicto y no se toca
//! nada.

use i18n::Msg;
use model::{Action, ActivityEntry, ActivityId, Dependency, EntityKind, FieldChange};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use store::{ActivityFilter, Reader};

use super::agent::supervisor_only;
use super::save;
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, views};

command!(Revert, "activity.revert", "cmd-activity-revert", revert, write, [req "entry": K::Text, opt "whole_command": K::Bool]);

fn revert(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let reference = args.require("entry")?;
    let not_found =
        || EngineError::not_found(Msg::new("notfound-activity").arg("entry", reference));
    let id: ActivityId = reference.parse().map_err(|_| not_found())?;
    let entry = cx.tx.activity_entry(id)?.ok_or_else(not_found)?;
    // Todo el comando, de la última entrada a la primera.
    let entries = if args.flag("whole_command")? {
        cx.tx.activity(&ActivityFilter {
            correlation: Some(entry.correlation),
            limit: 10_000,
            ..ActivityFilter::default()
        })?
    } else {
        vec![entry]
    };
    let r: &dyn Reader = &*cx.tx;
    let keys = crate::lookup::Keys::load(r)?;
    let mut reverted = Vec::new();
    for e in &entries {
        undo(cx, e)?;
        reverted.push(views::activity_target(&*cx.tx, &keys, e)?);
    }
    Ok(json!({ "reverted": entries.len(), "targets": reverted }))
}

/// Pone en cada campo su valor anterior, si nadie lo cambió después.
fn restore_fields<T: Serialize + DeserializeOwned>(
    current: &T,
    changes: &[FieldChange],
) -> Result<T, Vec<String>> {
    let mut obj = match serde_json::to_value(current) {
        Ok(Value::Object(map)) => map,
        _ => return Err(Vec::new()),
    };
    let mut conflicts = Vec::new();
    for c in changes {
        let now = obj.get(&c.field).cloned().unwrap_or(Value::Null);
        if now != c.after {
            conflicts.push(c.field.clone());
        } else {
            obj.insert(c.field.clone(), c.before.clone());
        }
    }
    if !conflicts.is_empty() {
        return Err(conflicts);
    }
    serde_json::from_value(Value::Object(obj)).map_err(|e| vec![e.to_string()])
}

fn conflict(e: &ActivityEntry, fields: Vec<String>, current: Value) -> EngineError {
    EngineError::Conflict {
        msg: Msg::new("revert-conflict")
            .arg("entry", e.id.to_string())
            .arg("fields", fields.join(", ")),
        current: Box::new(current),
    }
}

fn unsupported(e: &ActivityEntry) -> EngineError {
    EngineError::invalid(
        Msg::new("revert-unsupported")
            .arg("action", e.action.as_str())
            .arg("entity", e.entity.as_str()),
    )
}

fn gone(e: &ActivityEntry) -> EngineError {
    EngineError::not_found(Msg::new("revert-gone").arg("entity", e.entity.as_str()))
}

fn undo(cx: &mut Cx<'_>, e: &ActivityEntry) -> Result<(), EngineError> {
    match (e.entity, e.action) {
        // Mandar a la papelera también es un cambio de campo (`deleted_at`).
        (EntityKind::Task, Action::Updated | Action::Deleted) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let task = cx.tx.task(id)?.ok_or_else(|| gone(e))?;
            let mut restored = restore_fields(&task, &e.changes).map_err(|fields| {
                let current = serde_json::to_value(&task).unwrap_or_default();
                conflict(e, fields, current)
            })?;
            // La revisión sigue avanzando: es un cambio nuevo, no un viaje atrás.
            restored.revision = task.revision;
            save(cx, &mut restored)
        }
        (EntityKind::Task, Action::Created) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let mut task = cx.tx.task(id)?.ok_or_else(|| gone(e))?;
            if task.deleted_at.is_none() {
                task.deleted_at = Some(cx.now);
                save(cx, &mut task)?;
            }
            Ok(())
        }
        (EntityKind::Project, Action::Updated) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let project = cx.tx.project(id)?.ok_or_else(|| gone(e))?;
            let mut restored = restore_fields(&project, &e.changes).map_err(|fields| {
                conflict(
                    e,
                    fields,
                    serde_json::to_value(&project).unwrap_or_default(),
                )
            })?;
            restored.updated_at = cx.now;
            cx.tx.update_project(&restored)?;
            Ok(())
        }
        (EntityKind::Epic, Action::Updated) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let epic = cx.tx.epic(id)?.ok_or_else(|| gone(e))?;
            let mut restored = restore_fields(&epic, &e.changes).map_err(|fields| {
                conflict(e, fields, serde_json::to_value(&epic).unwrap_or_default())
            })?;
            restored.updated_at = cx.now;
            cx.tx.update_epic(&restored)?;
            Ok(())
        }
        (EntityKind::Diagram, Action::Updated) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let diagram = cx.tx.diagram(id)?.ok_or_else(|| gone(e))?;
            let mut restored = restore_fields(&diagram, &e.changes).map_err(|fields| {
                conflict(
                    e,
                    fields,
                    serde_json::to_value(&diagram).unwrap_or_default(),
                )
            })?;
            restored.updated_at = cx.now;
            cx.tx.update_diagram(&restored)?;
            Ok(())
        }
        (EntityKind::Diagram, Action::Created) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            cx.tx.diagram(id)?.ok_or_else(|| gone(e))?;
            cx.tx.delete_diagram(id)?;
            Ok(())
        }
        (EntityKind::Label, Action::Updated) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let label = cx.tx.label(id)?.ok_or_else(|| gone(e))?;
            let restored = restore_fields(&label, &e.changes).map_err(|fields| {
                conflict(e, fields, serde_json::to_value(&label).unwrap_or_default())
            })?;
            cx.tx.update_label(&restored)?;
            Ok(())
        }
        (EntityKind::Agent, Action::Updated) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            let agent = cx.tx.agent(id)?.ok_or_else(|| gone(e))?;
            let mut restored = restore_fields(&agent, &e.changes).map_err(|fields| {
                conflict(e, fields, serde_json::to_value(&agent).unwrap_or_default())
            })?;
            restored.updated_at = cx.now;
            cx.tx.update_agent(&restored)?;
            Ok(())
        }
        (EntityKind::Workspace, Action::Updated) => {
            let ws = cx.tx.workspace()?;
            let restored = restore_fields(&ws, &e.changes).map_err(|fields| {
                conflict(e, fields, serde_json::to_value(&ws).unwrap_or_default())
            })?;
            cx.tx.update_workspace(&restored)?;
            Ok(())
        }
        (EntityKind::Dependency, Action::Linked | Action::Unlinked) => {
            let (blocker, blocked) = e.entity_id.split_once('>').ok_or_else(|| gone(e))?;
            let dep = Dependency {
                blocker: blocker.parse().map_err(|_| gone(e))?,
                blocked: blocked.parse().map_err(|_| gone(e))?,
            };
            if e.action == Action::Linked {
                cx.tx.remove_dependency(dep)?;
            } else {
                if rules::find_cycle(&cx.tx.dependencies()?, dep).is_some() {
                    return Err(conflict(e, vec!["dependency".into()], Value::Null));
                }
                cx.tx.add_dependency(dep)?;
            }
            Ok(())
        }
        (EntityKind::Relation, Action::Linked | Action::Unlinked) => {
            let mut parts = e.entity_id.split('~');
            let (Some(a), Some(kind), Some(b)) = (parts.next(), parts.next(), parts.next()) else {
                return Err(gone(e));
            };
            let rel = model::Relation {
                a: a.parse().map_err(|_| gone(e))?,
                b: b.parse().map_err(|_| gone(e))?,
                kind: kind.parse().map_err(|_| gone(e))?,
            };
            if e.action == Action::Linked {
                cx.tx.remove_relation(rel)?;
            } else {
                cx.tx.add_relation(rel)?;
            }
            Ok(())
        }
        (EntityKind::Attachment, Action::Created) => {
            let id = e.entity_id.parse().map_err(|_| gone(e))?;
            cx.tx.delete_attachment(id)?;
            Ok(())
        }
        _ => Err(unsupported(e)),
    }
}
