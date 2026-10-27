//! Coordinación multiagente (P4): claims con lease, siguiente tarea, entrega,
//! escalado al humano y bloqueo humano. Sin papeles fijos ni revisión
//! obligatoria: cada tarea tiene los asignados que haga falta (D20).

use i18n::Msg;
use model::{Actor, AgentId, Claim, Comment, CommentId, Status, Task};
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::agent::supervisor_only;
use super::items::position;
use super::{detail, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::guard::{self, agent_id};
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(Take, "task.claim", "cmd-task-claim", claim, write, [req "task": K::Text, opt "revision": K::Int]);
command!(Heartbeat, "task.heartbeat", "cmd-task-heartbeat", heartbeat, write, [req "task": K::Text]);
command!(Release, "task.release", "cmd-task-release", release, write, [req "task": K::Text, opt "reason": K::Text]);
command!(
    ExpireLeases,
    "system.expire_leases",
    "cmd-system-expire_leases",
    expire_leases,
    write,
    []
);
command!(Next, "task.next", "cmd-task-next", next, write, [opt "project": K::Text, opt "claim": K::Bool]);
command!(SubmitReview, "task.submit_review", "cmd-task-submit_review", submit_review, write, [req "task": K::Text, req "summary": K::Text, opt "criteria": K::List, opt "revision": K::Int]);
command!(Escalate, "task.escalate", "cmd-task-escalate", escalate, write, [req "task": K::Text, req "question": K::Text, opt "revision": K::Int]);
command!(Answer, "task.answer", "cmd-task-answer", answer, write, [req "task": K::Text, req "answer": K::Text]);
command!(Reject, "task.reject", "cmd-task-reject", reject, write, [req "task": K::Text, req "reason": K::Text, opt "keep_assignee": K::Bool]);
command!(
    Housekeeping,
    "system.housekeeping",
    "cmd-system-housekeeping",
    housekeeping,
    write,
    []
);
command!(LockSet, "lock.set", "cmd-lock-set", lock_set, write, [opt "task": K::Text, opt "project": K::Text, opt "epic": K::Text]);
command!(LockClear, "lock.clear", "cmd-lock-clear", lock_clear, write, [opt "task": K::Text, opt "project": K::Text, opt "epic": K::Text]);

fn require_agent(cx: &Cx<'_>) -> Result<AgentId, EngineError> {
    agent_id(cx).ok_or_else(|| EngineError::forbidden(Msg::new("agent-only")))
}

fn key_of(cx: &Cx<'_>, task: &Task) -> Result<String, EngineError> {
    Ok(Keys::load(&*cx.tx)?.of(task))
}

/// Deja una nota del actor en la tarea (motivo, resumen, pregunta, respuesta):
/// así queda en los comentarios y en el historial.
pub(super) fn note(cx: &mut Cx<'_>, task: &Task, body: &str) -> Result<(), EngineError> {
    let comment = Comment {
        id: CommentId::generate(),
        task_id: task.id,
        author: cx.ctx.actor.clone(),
        body: rules::comment_body(body)?,
        created_at: cx.now,
    };
    cx.tx.insert_comment(&comment)?;
    Ok(())
}

/// Bloqueadores vivos que siguen abiertos.
fn open_blockers(r: &dyn Reader, task: &Task) -> Result<Vec<Task>, EngineError> {
    let mut out = Vec::new();
    for id in r.blockers_of(task.id)? {
        if let Some(b) = r.task(id)?
            && b.deleted_at.is_none()
            && !b.status.category().is_closed()
        {
            out.push(b);
        }
    }
    Ok(out)
}

/// ¿Puede un agente tomarla ahora? (sin contar claims ni bloqueos humanos)
fn check_ready(cx: &Cx<'_>, task: &Task) -> Result<(), EngineError> {
    let key = key_of(cx, task)?;
    if !matches!(
        task.status,
        Status::Backlog | Status::Todo | Status::InProgress
    ) {
        return Err(EngineError::NotReady(
            Msg::new("not-claimable")
                .arg("task", &key)
                .arg("status", task.status.as_str()),
        ));
    }
    let r: &dyn Reader = &*cx.tx;
    let blockers = open_blockers(r, task)?;
    if !blockers.is_empty() {
        let keys = Keys::load(r)?;
        let list: Vec<String> = blockers.iter().map(|b| keys.of(b)).collect();
        return Err(EngineError::NotReady(
            Msg::new("not-ready-blocked")
                .arg("task", &key)
                .arg("blockers", list.join(", ")),
        ));
    }
    Ok(())
}

/// Límite de tareas en progreso a la vez por agente (RF-COO-07).
fn check_wip(cx: &Cx<'_>, agent: AgentId, except: model::TaskId) -> Result<(), EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let limit = r.workspace()?.settings.agent_wip_limit;
    let mine: Vec<Task> = r
        .claimed_tasks()?
        .into_iter()
        .filter(|t| {
            t.id != except
                && t.status == Status::InProgress
                && t.active_claim(cx.now).is_some_and(|c| c.agent == agent)
        })
        .collect();
    if mine.len() >= usize::try_from(limit).unwrap_or(usize::MAX) {
        let keys = Keys::load(r)?;
        let list: Vec<String> = mine.iter().map(|t| keys.of(t)).collect();
        return Err(EngineError::invalid(
            Msg::new("agent-wip-limit")
                .arg("limit", limit)
                .arg("tasks", list.join(", ")),
        ));
    }
    Ok(())
}

/// Renueva el lease sin cambiar la revisión (un latido no es una edición).
fn renew(cx: &mut Cx<'_>, task: &mut Task) -> Result<(), EngineError> {
    let lease = guard::lease(&*cx.tx)?;
    if let Some(claim) = task.claim.as_mut() {
        claim.until = cx.now.checked_add(lease).unwrap_or(claim.until);
    }
    cx.tx.update_task(task)?;
    Ok(())
}

/// ¿Puede `agent` tomarla? Si tiene agentes asignados, solo ellos; si no,
/// cualquiera (D20).
fn may_take(assignees: &[Actor], agent: AgentId) -> bool {
    let mut agents = assignees.iter().filter(|a| a.is_agent()).peekable();
    agents.peek().is_none() || agents.any(|a| *a == Actor::Agent(agent))
}

/// Toma la tarea para `agent`: la asigna, le da el lease y la pasa a
/// `in_progress`, en la columna `stage` o, si no se da, en la primera
/// columna «en progreso» que sea suya (D20).
pub(super) fn take(
    cx: &mut Cx<'_>,
    agent: AgentId,
    mut task: Task,
    stage: Option<String>,
) -> Result<Task, EngineError> {
    if task.active_claim(cx.now).is_some_and(|c| c.agent == agent) {
        renew(cx, &mut task)?;
        return Ok(task);
    }
    // Un lease vencido libera la tarea aunque el barrido no haya pasado.
    if task.claim.is_some_and(|c| !c.is_active(cx.now)) {
        unclaim(&mut task);
    }
    if !may_take(&task.assignees, agent) {
        let key = key_of(cx, &task)?;
        let names: Vec<String> = task
            .assignees
            .iter()
            .filter(|a| a.is_agent())
            .map(|a| views::actor_name(&*cx.tx, a))
            .collect::<Result<_, _>>()?;
        return Err(EngineError::invalid(
            Msg::new("task-other-assignees")
                .arg("task", key)
                .arg("agents", names.join(", ")),
        ));
    }
    check_ready(cx, &task)?;
    check_wip(cx, agent, task.id)?;
    let lease = guard::lease(&*cx.tx)?;
    let until = cx
        .now
        .checked_add(lease)
        .map_err(|_| EngineError::invalid(Msg::new("arg-out-of-range").arg("value", "lease")))?;
    task.claim = Some(Claim {
        agent,
        since: cx.now,
        until,
        revision: task.revision,
    });
    task.assign(Actor::Agent(agent));
    task.status = Status::InProgress;
    task.stage = match stage {
        Some(stage) => Some(stage),
        None => cx.tx.project(task.project_id)?.and_then(|p| {
            p.workflow
                .iter()
                .find(|d| d.base == Status::InProgress && d.owner == Some(agent))
                .map(|d| d.key.clone())
        }),
    };
    // `save` sube la revisión y la anota en el claim (es el agente quien escribe).
    save(cx, &mut task)?;
    Ok(task)
}

fn claim(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let agent = require_agent(cx)?;
    let task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    guard::check_write(cx, &task, args.parse("revision")?)?;
    let task = take(cx, agent, task, None)?;
    with_handoff(cx, &task, agent)
}

/// El detalle más lo que necesita quien recoge el relevo (RF-COO-14): las
/// últimas notas de quienes trabajaron antes.
fn with_handoff(cx: &Cx<'_>, task: &Task, agent: AgentId) -> Result<Value, EngineError> {
    let mut out = detail(cx, task)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let comments = r.comments(task.id)?;
    let notes: Vec<Value> = comments
        .iter()
        .rev()
        .take(HANDOFF_NOTES)
        .rev()
        .map(|c| views::comment(r, &keys, c))
        .collect::<Result<_, _>>()?;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("notes".into(), json!(notes));
        obj.insert(
            "acting_as".into(),
            json!(views::actor_name(r, &Actor::Agent(agent))?),
        );
    }
    Ok(out)
}

/// Comentarios que acompañan al relevo.
const HANDOFF_NOTES: usize = 10;

fn heartbeat(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let agent = require_agent(cx)?;
    let mut task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    if !task.active_claim(cx.now).is_some_and(|c| c.agent == agent) {
        guard::check_write(cx, &task, None)?;
        return Err(EngineError::invalid(
            Msg::new("not-your-claim").arg("task", key_of(cx, &task)?),
        ));
    }
    renew(cx, &mut task)?;
    Ok(json!({
        "task": key_of(cx, &task)?,
        "lease_until": task.claim.map(|c| c.until),
    }))
}

fn release(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    let reason = args.get("reason").map(str::trim).filter(|r| !r.is_empty());
    if let Some(agent) = agent_id(cx) {
        // Un agente solo libera lo suyo, y explica por qué (RF-COO-04).
        if !task.active_claim(cx.now).is_some_and(|c| c.agent == agent) {
            guard::check_write(cx, &task, None)?;
            return Err(EngineError::invalid(
                Msg::new("not-your-claim").arg("task", key_of(cx, &task)?),
            ));
        }
        if reason.is_none() {
            return Err(EngineError::MissingArg("reason".into()));
        }
    }
    unclaim(&mut task);
    if let Some(reason) = reason {
        note(cx, &task, reason)?;
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

/// Quita el claim. Si estaba en progreso vuelve a `todo` y quien la tenía
/// deja de estar asignado, para que otro pueda tomarla.
fn unclaim(task: &mut Task) {
    if let Some(claim) = task.claim.take()
        && task.status == Status::InProgress
    {
        task.assignees.retain(|a| *a != Actor::Agent(claim.agent));
        task.status = Status::Todo;
    }
}

fn expire_leases(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    if !matches!(cx.ctx.actor, Actor::System | Actor::Supervisor) {
        return Err(EngineError::forbidden(Msg::new("supervisor-only")));
    }
    let mut expired = Vec::new();
    for mut task in cx.tx.claimed_tasks()? {
        if task.claim.is_some_and(|c| !c.is_active(cx.now)) {
            expired.push(key_of(cx, &task)?);
            unclaim(&mut task);
            save(cx, &mut task)?;
        }
    }
    Ok(json!({ "count": expired.len(), "expired": expired }))
}

/// ¿Está disponible para un agente? Lista, libre y sin bloqueo humano.
fn next(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let take_it = args.flag("claim")?;
    let agent = if take_it {
        Some(require_agent(cx)?)
    } else {
        agent_id(cx)
    };
    let mut filter = TaskFilter {
        statuses: vec![Status::Todo],
        ..TaskFilter::default()
    };
    if let Some(p) = args.get("project") {
        filter.project = Some(lookup::project(&*cx.tx, p)?.id);
    }
    // Candidatas ligeras; solo la elegida se carga entera (RNF-06).
    let now = cx.now;
    // El alcance y las capacidades del agente acotan lo que se le ofrece (D11).
    let mut next = store::NextFilter {
        status: Status::Todo,
        closed: rules::closed_statuses(),
        projects: filter.project.into_iter().collect(),
        labels: Vec::new(),
    };
    if let Some(a) = agent.map(|id| cx.tx.agent(id)).transpose()?.flatten() {
        // Alcance del agente y de su interfaz (D11, D15).
        if let Some(scope) = crate::identity::scope(&*cx.tx, &a)? {
            next.projects = match filter.project {
                Some(p) if scope.contains(&p) => vec![p],
                Some(_) => return Ok(json!({ "task": null, "available": 0 })),
                // Sin ningún proyecto en común no se le ofrece nada.
                None if scope.is_empty() => return Ok(json!({ "task": null, "available": 0 })),
                None => scope,
            };
        }
        for name in &a.capabilities {
            if let Some(label) = cx.tx.label_by_name(name)? {
                next.labels.push(label.id);
            }
        }
        // Capacidades que no son etiquetas de nadie: ninguna tarea encaja aún.
        if !a.capabilities.is_empty() && next.labels.is_empty() {
            next.labels.push(model::LabelId::generate());
        }
    }
    let free = |c: &store::NextCandidate| c.lease_until.is_none_or(|until| until <= now);
    let mut candidates: Vec<store::NextCandidate> = cx
        .tx
        .next_candidates(&next)?
        .into_iter()
        .filter(|c| {
            // Quien tenía el lease vencido ya no la retiene (igual que el barrido).
            let assignees: Vec<Actor> = c
                .assignees
                .iter()
                .filter(|x| {
                    !(c.lease_until.is_some_and(|u| u <= now)
                        && c.claimed_by.map(Actor::Agent).as_ref() == Some(*x))
                })
                .cloned()
                .collect();
            free(c) && agent.is_none_or(|a| may_take(&assignees, a))
        })
        .collect();
    // Primero lo que está en sus columnas (su cola), luego lo que tiene
    // asignado; después la más prioritaria, la que vence antes, la más antigua.
    let owners: std::collections::HashMap<model::ProjectId, model::Project> = cx
        .tx
        .projects(false)?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
    let mine = |c: &store::NextCandidate| -> (bool, bool) {
        let Some(a) = agent else {
            return (true, true);
        };
        let in_my_column = owners
            .get(&c.project_id)
            .is_some_and(|p| rules::column_owner(p, Status::Todo, c.stage.as_deref()) == Some(a));
        (!in_my_column, !c.assignees.contains(&Actor::Agent(a)))
    };
    candidates.sort_by_key(|c| {
        (
            mine(c),
            c.priority.rank(),
            c.due.as_ref().map(|d| d.date()).is_none(),
            c.due.as_ref().map(|d| d.date()),
            c.number,
        )
    });
    let available_count = candidates.len();
    let Some(best) = candidates.first() else {
        return Ok(json!({ "task": null, "available": 0 }));
    };
    let task = cx.tx.task(best.id)?.ok_or_else(|| {
        EngineError::not_found(Msg::new("notfound-task").arg("task", best.id.to_string()))
    })?;
    let (task, claimed) = match (take_it, agent) {
        (true, Some(agent)) => (take(cx, agent, task, None)?, true),
        _ => (task, false),
    };
    let shown = match agent {
        Some(a) => with_handoff(cx, &task, a)?,
        None => detail(cx, &task)?,
    };
    Ok(json!({
        "task": shown,
        "claimed": claimed,
        "available": available_count,
    }))
}

/// Solo quien la tiene (o el supervisor) puede entregarla o escalarla.
fn check_owner(cx: &Cx<'_>, task: &Task) -> Result<(), EngineError> {
    if let Some(agent) = agent_id(cx) {
        let holds = task.active_claim(cx.now).is_some_and(|c| c.agent == agent)
            || task.is_assigned(&Actor::Agent(agent));
        if !holds {
            return Err(EngineError::invalid(
                Msg::new("not-your-claim").arg("task", key_of(cx, task)?),
            ));
        }
    }
    Ok(())
}

fn submit_review(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    check_owner(cx, &task)?;
    rules::check_transition(task.status, Status::InReview)?;
    for reference in args.list("criteria") {
        let index = position(&task.criteria, &reference, super::items::Kind::Criteria)?;
        if let Some(item) = task.criteria.get_mut(index) {
            item.done = true;
        }
    }
    note(cx, &task, args.require("summary")?)?;
    task.claim = None;
    task.status = Status::InReview;
    save(cx, &mut task)?;
    match agent_id(cx) {
        Some(a) => with_handoff(cx, &task, a),
        None => detail(cx, &task),
    }
}

fn escalate(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    check_owner(cx, &task)?;
    let question = args.require("question")?;
    note(cx, &task, question)?;
    task.needs_human = Some(question.trim().to_owned());
    task.claim = None;
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn answer(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let mut task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    if task.needs_human.is_none() {
        return Err(EngineError::invalid(
            Msg::new("answer-no-question").arg("task", key_of(cx, &task)?),
        ));
    }
    note(cx, &task, args.require("answer")?)?;
    task.needs_human = None;
    save(cx, &mut task)?;
    detail(cx, &task)
}

/// Rechaza una entrega (RF-SUP-06): vuelve a `todo` con el motivo como
/// comentario; con `keep_assignee=true` sigue asignada al mismo agente.
fn reject(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let mut task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    if task.status != Status::InReview {
        return Err(EngineError::invalid(
            Msg::new("reject-not-in-review").arg("task", key_of(cx, &task)?),
        ));
    }
    note(cx, &task, args.require("reason")?)?;
    task.status = Status::Todo;
    task.claim = None;
    if !args.flag("keep_assignee")? {
        task.set_assignee(None);
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn set_lock(cx: &mut Cx<'_>, args: &Args, locked: bool) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    if let Some(reference) = args.get("epic") {
        let project = args
            .get("project")
            .map(|p| lookup::project(&*cx.tx, p).map(|p| p.id))
            .transpose()?;
        let mut epic = lookup::epic(&*cx.tx, reference, project, None)?;
        epic.locked = locked;
        epic.updated_at = cx.now;
        cx.tx.update_epic(&epic)?;
        return super::epic::view(&*cx.tx, &epic);
    }
    match (args.get("task"), args.get("project")) {
        (Some(reference), None) => {
            let mut task = lookup::editable_task(&*cx.tx, reference)?;
            task.locked = locked;
            save(cx, &mut task)?;
            detail(cx, &task)
        }
        (None, Some(reference)) => {
            let mut project = lookup::project(&*cx.tx, reference)?;
            project.locked = locked;
            project.updated_at = cx.now;
            cx.tx.update_project(&project)?;
            Ok(views::project(&project))
        }
        _ => Err(EngineError::invalid(Msg::new("lock-target-required"))),
    }
}

/// Limpieza diaria del sistema: olvida las claves de idempotencia de más de
/// una semana (RF-MCP-09).
fn housekeeping(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let week = cx
        .now
        .checked_sub(jiff::SignedDuration::from_hours(24 * 7))
        .unwrap_or(cx.now);
    let keys = cx.tx.purge_idempotency(week)?;
    Ok(json!({ "idempotency_keys": keys }))
}

fn lock_set(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_lock(cx, args, true)
}

fn lock_clear(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_lock(cx, args, false)
}
