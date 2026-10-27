//! Gestión de la información (D23): eliminar un proyecto completo, vaciar el
//! workspace o podar el historial antiguo. Solo el supervisor, con una
//! confirmación escrita y siempre después de una copia de seguridad
//! (`antes-de-borrar-…taskcraft`) hecha en la misma transacción.

use i18n::Msg;
use jiff::civil::Date;
use serde_json::{Value, json};

use super::agent::supervisor_only;
use super::exchange::safety_copy;
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup};

command!(ProjectDelete, "project.delete", "cmd-project-delete", project_delete, write, [req "project": K::Text, req "confirm": K::Text, opt "dir": K::Text]);
command!(WorkspaceWipe, "workspace.wipe", "cmd-workspace-wipe", workspace_wipe, write, [req "confirm": K::Text, opt "dir": K::Text]);
command!(ActivityPrune, "activity.prune", "cmd-activity-prune", activity_prune, write, [req "before": K::Date, opt "dir": K::Text]);

/// `confirm` tiene que repetir `expected` (sin distinguir mayúsculas).
fn confirmed(args: &Args, expected: &str) -> Result<(), EngineError> {
    let given = args.require("confirm")?.trim();
    if given.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(EngineError::arg(
            "confirm",
            Msg::new("data-confirm-mismatch").arg("expected", expected),
        ))
    }
}

/// El proyecto con todas sus tareas, comentarios, ciclos, épicas,
/// diagramas, adjuntos e historial.
fn project_delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    confirmed(args, &project.key)?;
    let backup = safety_copy(cx, args)?;
    cx.tx.delete_project(project.id)?;
    Ok(json!({ "deleted": project.key, "name": project.name, "backup": backup }))
}

/// Todo menos el nombre y la configuración del workspace. Se confirma
/// escribiendo el nombre del workspace.
fn workspace_wipe(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let ws = cx.tx.workspace()?;
    confirmed(args, &ws.name)?;
    let backup = safety_copy(cx, args)?;
    cx.tx.wipe_workspace()?;
    Ok(json!({ "wiped": ws.name, "backup": backup }))
}

/// Historial anterior a `before` (un día, en UTC).
fn activity_prune(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let day: Date = super::time::parse_day(args, "before")?
        .ok_or_else(|| EngineError::MissingArg("before".into()))?;
    if day > super::time::day_of(cx.now) {
        return Err(EngineError::arg(
            "before",
            Msg::new("data-prune-future").arg("before", day.to_string()),
        ));
    }
    let before = day
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map(|z| z.timestamp())
        .map_err(|_| {
            EngineError::arg(
                "before",
                Msg::new("arg-out-of-range").arg("value", day.to_string()),
            )
        })?;
    let backup = safety_copy(cx, args)?;
    let deleted = cx.tx.prune_activity(before)?;
    Ok(json!({ "deleted": deleted, "before": day, "backup": backup }))
}
