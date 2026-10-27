//! Eventos de tarea que salen del historial de un comando (RF-INT-02,
//! RF-INT-03): los webhooks se encolan en la misma transacción y las
//! automatizaciones se ejecutan tras confirmarla.

use model::{
    Action, ActivityEntry, Delivery, DeliveryId, DeliveryStatus, EntityKind, TaskId, Timestamp,
    Trigger,
};
use serde_json::json;
use store::{Reader, Writer};

use crate::lookup::Keys;
use crate::{Context, EngineError, views};

/// Qué le pasó a qué tarea, sin repetir.
pub(crate) fn detect(entries: &[ActivityEntry]) -> Vec<(Trigger, TaskId)> {
    let mut out: Vec<(Trigger, TaskId)> = Vec::new();
    let mut push = |t: Trigger, id: TaskId| {
        if !out.contains(&(t, id)) {
            out.push((t, id));
        }
    };
    for e in entries {
        let Some(task) = e.task_id else { continue };
        match (e.entity, e.action) {
            (EntityKind::Task, Action::Created) => push(Trigger::Created, task),
            (EntityKind::Comment, Action::Created) => push(Trigger::Commented, task),
            (EntityKind::Task, Action::Updated) => {
                for c in &e.changes {
                    match c.field.as_str() {
                        "status" => {
                            push(Trigger::StatusChanged, task);
                            if c.after == json!("done") {
                                push(Trigger::Completed, task);
                            }
                        }
                        "assignee" if !c.after.is_null() => push(Trigger::Assigned, task),
                        "needs_human" if !c.after.is_null() => push(Trigger::Escalated, task),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Encola un envío por cada webhook activo interesado en cada evento.
pub(crate) fn enqueue(
    tx: &mut dyn Writer,
    ctx: &Context,
    command: &str,
    events: &[(Trigger, TaskId)],
    now: Timestamp,
) -> Result<(), EngineError> {
    if events.is_empty() {
        return Ok(());
    }
    let hooks: Vec<_> = tx.webhooks()?.into_iter().filter(|w| w.enabled).collect();
    if hooks.is_empty() {
        return Ok(());
    }
    let r: &dyn Reader = &*tx;
    let keys = Keys::load(r)?;
    let actor = views::actor_name(r, &ctx.actor)?;
    let mut queued = Vec::new();
    for (trigger, id) in events {
        let Some(task) = r.task(*id)? else { continue };
        let payload = json!({
            "event": trigger,
            "at": now,
            "actor": actor,
            "command": command,
            "correlation": ctx.correlation.to_string(),
            "task": views::task_summary(r, &keys, &task)?,
        });
        for hook in hooks.iter().filter(|h| h.triggers.contains(trigger)) {
            queued.push(Delivery {
                id: DeliveryId::generate(),
                webhook_id: hook.id,
                trigger: *trigger,
                payload: payload.to_string(),
                attempts: 0,
                next_at: now,
                status: DeliveryStatus::Pending,
                last_error: String::new(),
                created_at: now,
            });
        }
    }
    for d in &queued {
        tx.save_delivery(d)?;
    }
    Ok(())
}

/// `{task}`, `{parent}` y `{project}` en los argumentos de una automatización.
pub(crate) fn fill(template: &str, task: &str, parent: &str, project: &str) -> String {
    template
        .replace("{task}", task)
        .replace("{parent}", parent)
        .replace("{project}", project)
}

/// Lo que se envía tras un intento: hecho, o reintento con espera creciente
/// (30 s, 1 min, 2 min… hasta 8 intentos).
pub fn after_attempt(d: &mut Delivery, ok: bool, error: &str, now: Timestamp) {
    d.attempts = d.attempts.saturating_add(1);
    if ok {
        d.status = DeliveryStatus::Delivered;
        d.last_error.clear();
        return;
    }
    d.last_error = error.chars().take(500).collect();
    if d.attempts >= MAX_ATTEMPTS {
        d.status = DeliveryStatus::Failed;
        return;
    }
    let wait = 30i64.saturating_mul(1 << d.attempts.saturating_sub(1).min(10));
    d.next_at = now
        .checked_add(jiff::SignedDuration::from_secs(wait))
        .unwrap_or(now);
}

pub const MAX_ATTEMPTS: u32 = 8;
