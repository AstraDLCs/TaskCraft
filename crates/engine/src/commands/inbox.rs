//! Bandeja de hallazgos (RF-COO-12): ideas, bugs y deuda técnica que un agente
//! deja sin proyecto, para que el supervisor los clasifique.

use i18n::Msg;
use model::{InboxId, InboxItem, InboxKind, InboxStatus, Priority};
use serde_json::{Value, json};
use store::Reader;

use super::agent::supervisor_only;
use super::paged;
use super::task::{NewTask, insert_new};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(Add, "inbox.add", "cmd-inbox-add", add, write, [req "title": K::Text, opt "body": K::Text, opt "kind": K::Enum(InboxKind::NAMES), opt "idempotency_key": K::Text]);
command!(List, "inbox.list", "cmd-inbox-list", list_items, read, [opt "status": K::Enum(InboxStatus::NAMES), opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Convert, "inbox.convert", "cmd-inbox-convert", convert, write, [req "item": K::Text, req "project": K::Text, opt "priority": K::Enum(Priority::NAMES)]);
command!(Discard, "inbox.discard", "cmd-inbox-discard", discard, write, [req "item": K::Text]);

fn view(r: &dyn Reader, i: &InboxItem) -> Result<Value, EngineError> {
    let task = i
        .task_id
        .map(|id| Keys::load(r)?.of_id(r, id))
        .transpose()?;
    Ok(json!({
        "id": i.id,
        "kind": i.kind,
        "title": i.title,
        "body": i.body,
        "author": i.author,
        "author_name": views::actor_name(r, &i.author)?,
        "status": i.status,
        "task": task,
        "created_at": i.created_at,
    }))
}

/// Hallazgo todavía abierto, por ID.
fn open_item(cx: &Cx<'_>, args: &Args) -> Result<InboxItem, EngineError> {
    let reference = args.require("item")?;
    let not_found =
        || EngineError::not_found(Msg::new("notfound-inbox-item").arg("item", reference));
    let id: InboxId = reference.parse().map_err(|_| not_found())?;
    let item = cx.tx.inbox_item(id)?.ok_or_else(not_found)?;
    if item.status != InboxStatus::Open {
        return Err(EngineError::invalid(
            Msg::new("inbox-not-open").arg("item", reference),
        ));
    }
    Ok(item)
}

fn add(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let item = InboxItem {
        id: InboxId::generate(),
        kind: args.parse("kind")?.unwrap_or(InboxKind::Idea),
        title: rules::task_title(args.require("title")?)?,
        body: args.get("body").unwrap_or_default().to_owned(),
        author: cx.ctx.actor.clone(),
        status: InboxStatus::Open,
        task_id: None,
        created_at: cx.now,
        updated_at: cx.now,
    };
    cx.tx.insert_inbox_item(&item)?;
    view(&*cx.tx, &item)
}

fn list_items(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let status = args.parse("status")?.unwrap_or(InboxStatus::Open);
    let r: &dyn Reader = &*cx.tx;
    paged(r.inbox_items(Some(status))?, args, |i| view(r, i))
}

fn convert(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let mut item = open_item(cx, args)?;
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    let task = insert_new(
        cx,
        project.id,
        None,
        NewTask {
            description: item.body.clone(),
            priority: args.parse("priority")?.unwrap_or(Priority::Medium),
            labels: vec![item.kind.as_str().to_owned()],
            ..NewTask::titled(&item.title)
        },
    )?;
    item.status = InboxStatus::Converted;
    item.task_id = Some(task.id);
    item.updated_at = cx.now;
    cx.tx.update_inbox_item(&item)?;
    view(&*cx.tx, &item)
}

fn discard(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let mut item = open_item(cx, args)?;
    item.status = InboxStatus::Discarded;
    item.updated_at = cx.now;
    cx.tx.update_inbox_item(&item)?;
    view(&*cx.tx, &item)
}
