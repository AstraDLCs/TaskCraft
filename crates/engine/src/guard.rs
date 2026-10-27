//! Guardia central de escritura sobre tareas (M1.2): concurrencia optimista
//! (RF-COO-06), bloqueo humano (RF-SUP-05), escalado pendiente (RF-COO-10),
//! claims ajenos (RF-COO-01) y prevalencia del supervisor (RF-SUP-04).
//!
//! Todo comando que modifica una tarea pasa por aquí; así ninguna regla de
//! coordinación depende de que cada comando se acuerde de comprobarla.

use i18n::Msg;
use model::{Actor, AgentId, Task};
use store::Reader;

use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

/// Agente que ejecuta el comando, si es un agente.
pub(crate) fn agent_id(cx: &Cx<'_>) -> Option<AgentId> {
    match cx.ctx.actor {
        Actor::Agent(id) => Some(id),
        _ => None,
    }
}

/// Duración de un lease según la configuración del workspace.
pub(crate) fn lease(r: &dyn Reader) -> Result<jiff::SignedDuration, EngineError> {
    let minutes = r.workspace()?.settings.lease_minutes;
    Ok(jiff::SignedDuration::from_mins(i64::from(minutes)))
}

/// Un agente con alcance (D11) solo escribe en sus proyectos.
pub(crate) fn check_scope(cx: &Cx<'_>, project: model::ProjectId) -> Result<(), EngineError> {
    let Some(agent) = agent_id(cx) else {
        return Ok(());
    };
    let Some(a) = cx.tx.agent(agent)? else {
        return Ok(());
    };
    // El de la interfaz también cuenta (D15).
    if crate::identity::scope(&*cx.tx, &a)?.is_none_or(|s| s.contains(&project)) {
        return Ok(());
    }
    let key = cx
        .tx
        .project(project)?
        .map(|p| p.key)
        .unwrap_or_else(|| project.to_string());
    Err(EngineError::Forbidden(
        Msg::new("agent-out-of-scope")
            .arg("agent", &a.name)
            .arg("project", key),
    ))
}

/// Tarea indicada en `task=` lista para modificarse por quien llama.
pub(crate) fn task_for_write(cx: &Cx<'_>, args: &Args) -> Result<Task, EngineError> {
    let task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    check_write(cx, &task, args.parse("revision")?)?;
    Ok(task)
}

fn conflict(r: &dyn Reader, task: &Task, msg: Msg) -> Result<EngineError, EngineError> {
    let current = views::task_detail(r, &Keys::load(r)?, task)?;
    Ok(EngineError::Conflict {
        msg,
        current: Box::new(current),
    })
}

/// Comprueba que quien llama puede modificar `task` ahora mismo.
pub(crate) fn check_write(
    cx: &Cx<'_>,
    task: &Task,
    expected_revision: Option<u64>,
) -> Result<(), EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let key = Keys::load(r)?.of(task);

    if let Some(expected) = expected_revision
        && expected != task.revision
    {
        let msg = Msg::new("conflict-revision")
            .arg("task", &key)
            .arg("expected", expected)
            .arg("current", task.revision);
        return Err(conflict(r, task, msg)?);
    }

    let Some(agent) = agent_id(cx) else {
        // El supervisor, el sistema y las integraciones prevalecen (RF-SUP-04).
        return Ok(());
    };
    check_scope(cx, task.project_id)?;

    if r.project(task.project_id)?.is_some_and(|p| p.locked) {
        return Err(EngineError::LockedByHuman(
            Msg::new("locked-project").arg("task", &key),
        ));
    }
    for t in std::iter::once(task.clone()).chain(lookup::ancestors(r, task)?) {
        if t.locked {
            return Err(EngineError::LockedByHuman(
                Msg::new("locked-task").arg("task", Keys::load(r)?.of(&t)),
            ));
        }
    }
    for group in [task.epic_id, task.milestone_id].into_iter().flatten() {
        if let Some(e) = r.epic(group)?.filter(|e| e.locked) {
            return Err(EngineError::LockedByHuman(
                Msg::new("locked-epic")
                    .arg("task", &key)
                    .arg("epic", e.name),
            ));
        }
    }
    if task.needs_human.is_some() {
        return Err(EngineError::LockedByHuman(
            Msg::new("needs-human-pending").arg("task", &key),
        ));
    }
    if let Some(claim) = task.active_claim(cx.now) {
        if claim.agent != agent {
            let holder = views::actor_name(r, &Actor::Agent(claim.agent))?;
            return Err(EngineError::ClaimedByOther(
                Msg::new("claimed-by-other")
                    .arg("task", &key)
                    .arg("agent", holder)
                    .arg("until", claim.until.to_string()),
            ));
        }
        // Alguien (el supervisor) la cambió desde la última escritura del agente.
        if expected_revision.is_none() && claim.revision != task.revision {
            let msg = Msg::new("conflict-changed-by-other")
                .arg("task", &key)
                .arg("current", task.revision);
            return Err(conflict(r, task, msg)?);
        }
    }
    Ok(())
}

/// Tras una escritura del agente que tiene la tarea, renueva el lease y anota
/// la revisión que ha visto (latido implícito, RF-COO-02).
pub(crate) fn renew_if_claimer(cx: &Cx<'_>, task: &mut Task) -> Result<(), EngineError> {
    let Some(agent) = agent_id(cx) else {
        return Ok(());
    };
    let lease = lease(&*cx.tx)?;
    if let Some(claim) = task.claim.as_mut()
        && claim.agent == agent
        && claim.is_active(cx.now)
    {
        claim.until = cx.now.checked_add(lease).unwrap_or(claim.until);
        claim.revision = task.revision;
    }
    Ok(())
}
