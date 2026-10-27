//! Tareas recurrentes (RF-MOD-18, D17): la regla vive en la tarea y, al
//! terminarla, nace la siguiente con la fecha que toca.

use i18n::Msg;
use model::{CheckItem, ItemId, Status, Task, TaskId, When};
use serde_json::Value;
use store::Reader;

use super::{detail, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::Keys;
use crate::{Args, Cx, EngineError, guard};

command!(SetRecurrence, "task.set_recurrence", "cmd-task-set_recurrence", set, write, [req "task": K::Text, opt "every": K::Text, opt "interval": K::Int, opt "on": K::List, opt "day": K::Int, opt "after": K::Text, opt "revision": K::Int]);
command!(ClearRecurrence, "task.clear_recurrence", "cmd-task-clear_recurrence", clear, write, [req "task": K::Text, opt "revision": K::Int]);

fn set(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    let on = args.list("on").join(",");
    let rule = rules::recurrence::parse(
        args.get("every"),
        args.parse("interval")?,
        Some(on.as_str()),
        args.parse("day")?,
        args.get("after"),
    )?;
    task.recurrence = Some(rule);
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn clear(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    if task.recurrence.take().is_some() {
        save(cx, &mut task)?;
    }
    detail(cx, &task)
}

/// Desplaza una fecha `days` días, conservando la hora si la tenía.
fn shift(when: &When, days: i64) -> Option<When> {
    let span = jiff::Span::new().days(days);
    match when {
        When::Day(d) => d.checked_add(span).ok().map(When::Day),
        When::At(z) => z.checked_add(span).ok().map(When::At),
    }
}

fn reset(items: &[CheckItem]) -> Vec<CheckItem> {
    items
        .iter()
        .map(|i| CheckItem {
            id: ItemId::generate(),
            text: i.text.clone(),
            done: false,
        })
        .collect()
}

/// Si `id` acaba de terminarse y se repite, crea la siguiente y le pasa la
/// regla (D17). Devuelve su clave.
pub(crate) fn spawn_next(cx: &mut Cx<'_>, id: TaskId) -> Result<Option<String>, EngineError> {
    let Some(mut done) = cx.tx.task(id)? else {
        return Ok(None);
    };
    let Some(rule) = done.recurrence.clone() else {
        return Ok(None);
    };
    if done.status != Status::Done || done.deleted_at.is_some() {
        return Ok(None);
    }
    let mut project = cx
        .tx
        .project(done.project_id)?
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?;
    let today = super::time::day_of(cx.now);
    let previous = done.due.as_ref().map(When::date);
    let next = rules::recurrence::next_due(&rule, previous, today);
    let moved = previous.map_or(0, |p| i64::from((next - p).get_days()));
    let due = match &done.due {
        Some(w) => shift(w, moved),
        None => Some(When::Day(next)),
    };
    let start = match (&done.start, previous) {
        (Some(s), Some(_)) => shift(s, moved),
        _ => None,
    };
    let task = Task {
        id: TaskId::generate(),
        project_id: done.project_id,
        number: project.next_number,
        parent_id: done.parent_id,
        title: done.title.clone(),
        description: done.description.clone(),
        status: Status::Todo,
        priority: done.priority,
        start,
        due,
        labels: done.labels.clone(),
        assignees: done.assignees.clone(),
        creator: cx.ctx.actor.clone(),
        checklist: reset(&done.checklist),
        criteria: reset(&done.criteria),
        revision: 1,
        created_at: cx.now,
        updated_at: cx.now,
        archived_at: None,
        deleted_at: None,
        claim: None,
        needs_human: None,
        locked: false,
        stage: None,
        estimate: done.estimate,
        epic_id: done.epic_id,
        milestone_id: done.milestone_id,
        fields: done.fields.clone(),
        recurrence: Some(rule),
        cycles: Vec::new(),
    };
    project.next_number = project.next_number.saturating_add(1);
    project.updated_at = cx.now;
    cx.tx.update_project(&project)?;
    cx.tx.insert_task(&task)?;
    super::sync_mentions(cx, task.id, None, &task.description)?;
    // La regla sigue en la nueva: reabrir y cerrar esta no duplica la serie.
    done.recurrence = None;
    save(cx, &mut done)?;
    let r: &dyn Reader = &*cx.tx;
    Ok(Some(Keys::load(r)?.of(&task)))
}
