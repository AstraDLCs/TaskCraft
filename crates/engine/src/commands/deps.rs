//! Bloqueos entre tareas (RF-REL-01) sin ciclos (RF-REL-02), y relaciones
//! no bloqueantes: relacionada y duplicada de (RF-REL-03).

use i18n::Msg;
use model::Dependency;
use serde_json::{Value, json};
use store::Reader;

use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(Link, "task.link", "cmd-task-link", link, write, [req "blocker": K::Text, req "blocked": K::Text]);
command!(Unlink, "task.unlink", "cmd-task-unlink", unlink, write, [req "blocker": K::Text, req "blocked": K::Text]);
command!(Relate, "task.relate", "cmd-task-relate", relate, write, [req "task": K::Text, req "other": K::Text, opt "kind": K::Enum(model::RelationKind::NAMES)]);
command!(Unrelate, "task.unrelate", "cmd-task-unrelate", unrelate, write, [req "task": K::Text, req "other": K::Text, opt "kind": K::Enum(model::RelationKind::NAMES)]);

/// `relates` es simétrica: se guarda con el par ordenado para no duplicarla.
fn relation(a: model::TaskId, b: model::TaskId, kind: model::RelationKind) -> model::Relation {
    match kind {
        model::RelationKind::Relates if b < a => model::Relation { a: b, b: a, kind },
        _ => model::Relation { a, b, kind },
    }
}

/// `duplicates`: `task` es duplicada de `other` y se cierra como cancelada.
fn relate(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = crate::guard::task_for_write(cx, args)?;
    let other = lookup::task(&*cx.tx, args.require("other")?)?;
    if other.id == task.id {
        return Err(EngineError::invalid(Msg::new("relation-self")));
    }
    let kind = args.parse("kind")?.unwrap_or(model::RelationKind::Relates);
    cx.tx.add_relation(relation(task.id, other.id, kind))?;
    if kind == model::RelationKind::Duplicates && !task.status.category().is_closed() {
        rules::check_transition(task.status, model::Status::Cancelled)?;
        task.status = model::Status::Cancelled;
        task.claim = None;
        super::save(cx, &mut task)?;
    }
    super::detail(cx, &task)
}

fn unrelate(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = crate::guard::task_for_write(cx, args)?;
    let other = lookup::task_any(&*cx.tx, args.require("other")?)?;
    let kind = args.parse("kind")?.unwrap_or(model::RelationKind::Relates);
    if !cx.tx.remove_relation(relation(task.id, other.id, kind))? {
        return Err(EngineError::not_found(Msg::new("notfound-relation")));
    }
    super::detail(cx, &task)
}

fn link(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let blocker = lookup::task(r, args.require("blocker")?)?;
    let blocked = lookup::task(r, args.require("blocked")?)?;
    crate::guard::check_write(cx, &blocked, None)?;
    let dep = Dependency {
        blocker: blocker.id,
        blocked: blocked.id,
    };
    if let Some(cycle) = rules::find_cycle(&r.dependencies()?, dep) {
        let keys = Keys::load(r)?;
        let path = cycle
            .into_iter()
            .map(|id| keys.of_id(r, id))
            .collect::<Result<Vec<_>, _>>()?
            .join(" -> ");
        return Err(EngineError::invalid(
            Msg::new("dependency-cycle").arg("path", path),
        ));
    }
    cx.tx.add_dependency(dep)?;
    result(cx, &blocker, &blocked)
}

fn unlink(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let blocker = lookup::task_any(&*cx.tx, args.require("blocker")?)?;
    let blocked = lookup::task_any(&*cx.tx, args.require("blocked")?)?;
    let dep = Dependency {
        blocker: blocker.id,
        blocked: blocked.id,
    };
    if !cx.tx.remove_dependency(dep)? {
        return Err(EngineError::not_found(Msg::new("notfound-dependency")));
    }
    result(cx, &blocker, &blocked)
}

fn result(cx: &Cx<'_>, blocker: &model::Task, blocked: &model::Task) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    Ok(json!({
        "blocker": keys.of(blocker),
        "blocked": keys.of(blocked),
        "blocked_ready": views::is_ready(r, blocked)?,
    }))
}
