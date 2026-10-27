//! Checklist (RF-MOD-05) y criterios de aceptación (RF-MOD-11): ítems
//! marcables de uno en uno, sin reescribir la descripción.

use i18n::Msg;
use model::{CheckItem, ItemId, Task};
use serde_json::Value;

use super::{detail, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Checklist,
    Criteria,
}

impl Kind {
    fn items(self, task: &mut Task) -> &mut Vec<CheckItem> {
        match self {
            Self::Checklist => &mut task.checklist,
            Self::Criteria => &mut task.criteria,
        }
    }

    fn not_found_key(self) -> &'static str {
        match self {
            Self::Checklist => "notfound-checklist-item",
            Self::Criteria => "notfound-criterion",
        }
    }
}

command!(ChecklistAdd, "checklist.add", "cmd-checklist-add", checklist_add, write, [req "task": K::Text, req "text": K::Text, opt "revision": K::Int, opt "idempotency_key": K::Text]);
command!(ChecklistCheck, "checklist.check", "cmd-checklist-check", checklist_check, write, [req "task": K::Text, req "item": K::Text, opt "done": K::Bool, opt "revision": K::Int]);
command!(ChecklistRemove, "checklist.remove", "cmd-checklist-remove", checklist_remove, write, [req "task": K::Text, req "item": K::Text, opt "revision": K::Int]);
command!(CriteriaAdd, "criteria.add", "cmd-criteria-add", criteria_add, write, [req "task": K::Text, req "text": K::Text, opt "revision": K::Int, opt "idempotency_key": K::Text]);
command!(CriteriaCheck, "criteria.check", "cmd-criteria-check", criteria_check, write, [req "task": K::Text, req "item": K::Text, opt "done": K::Bool, opt "revision": K::Int]);
command!(CriteriaRemove, "criteria.remove", "cmd-criteria-remove", criteria_remove, write, [req "task": K::Text, req "item": K::Text, opt "revision": K::Int]);
command!(TaskCheck, "task.check", "cmd-task-check", task_check, write, [req "task": K::Text, req "kind": K::Enum(&["checklist", "criteria"]), req "item": K::Text, opt "done": K::Bool, opt "revision": K::Int]);
command!(ChecklistEdit, "checklist.edit", "cmd-checklist-edit", checklist_edit, write, [req "task": K::Text, req "item": K::Text, req "text": K::Text, opt "revision": K::Int]);
command!(CriteriaEdit, "criteria.edit", "cmd-criteria-edit", criteria_edit, write, [req "task": K::Text, req "item": K::Text, req "text": K::Text, opt "revision": K::Int]);

fn checklist_add(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    add(cx, a, Kind::Checklist)
}
fn checklist_check(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    check(cx, a, Kind::Checklist)
}
fn checklist_remove(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    remove(cx, a, Kind::Checklist)
}
fn criteria_add(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    add(cx, a, Kind::Criteria)
}
fn criteria_check(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    check(cx, a, Kind::Criteria)
}
fn criteria_remove(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    remove(cx, a, Kind::Criteria)
}

/// Alias único para agentes (RF-MCP-04): marca un paso o un criterio.
fn task_check(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    let kind = match a.require("kind")? {
        "criteria" => Kind::Criteria,
        "checklist" => Kind::Checklist,
        other => {
            return Err(EngineError::arg(
                "kind",
                Msg::new("arg-one-of")
                    .arg("value", other)
                    .arg("valid", "checklist, criteria"),
            ));
        }
    };
    check(cx, a, kind)
}

fn checklist_edit(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    edit(cx, a, Kind::Checklist)
}
fn criteria_edit(cx: &mut Cx<'_>, a: &Args) -> Result<Value, EngineError> {
    edit(cx, a, Kind::Criteria)
}

fn item_text(args: &Args) -> Result<String, EngineError> {
    let text = args.require("text")?.trim();
    if text.is_empty() || text.chars().count() > 500 {
        return Err(EngineError::arg(
            "text",
            Msg::new("arg-length").arg("min", 1u32).arg("max", 500u32),
        ));
    }
    Ok(text.to_owned())
}

fn edit(cx: &mut Cx<'_>, args: &Args, kind: Kind) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let text = item_text(args)?;
    let index = position(kind.items(&mut task), args.require("item")?, kind)?;
    if let Some(item) = kind.items(&mut task).get_mut(index) {
        item.text = text;
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn add(cx: &mut Cx<'_>, args: &Args, kind: Kind) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let text = args.require("text")?.trim();
    if text.is_empty() || text.chars().count() > 500 {
        return Err(EngineError::arg(
            "text",
            Msg::new("arg-length").arg("min", 1u32).arg("max", 500u32),
        ));
    }
    kind.items(&mut task).push(CheckItem {
        id: ItemId::generate(),
        text: text.to_owned(),
        done: false,
    });
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn check(cx: &mut Cx<'_>, args: &Args, kind: Kind) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let done = if args.get("done").is_some() {
        args.flag("done")?
    } else {
        true
    };
    let index = position(kind.items(&mut task), args.require("item")?, kind)?;
    if let Some(item) = kind.items(&mut task).get_mut(index) {
        item.done = done;
    }
    save(cx, &mut task)?;
    detail(cx, &task)
}

fn remove(cx: &mut Cx<'_>, args: &Args, kind: Kind) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let index = position(kind.items(&mut task), args.require("item")?, kind)?;
    kind.items(&mut task).remove(index);
    save(cx, &mut task)?;
    detail(cx, &task)
}

/// Índice de un ítem por número (1 = primero) o por ID.
pub(super) fn position(
    items: &[CheckItem],
    reference: &str,
    kind: Kind,
) -> Result<usize, EngineError> {
    let found = match reference.parse::<usize>() {
        Ok(n) if (1..=items.len()).contains(&n) => Some(n - 1),
        Ok(_) => None,
        Err(_) => reference
            .parse::<ItemId>()
            .ok()
            .and_then(|id| items.iter().position(|i| i.id == id)),
    };
    found.ok_or_else(|| {
        EngineError::not_found(
            Msg::new(kind.not_found_key())
                .arg("item", reference)
                .arg("count", items.len()),
        )
    })
}
