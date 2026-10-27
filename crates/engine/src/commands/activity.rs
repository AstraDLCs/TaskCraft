//! Historial (RF-HIS-01) y comentarios (RF-HIS-02).

use model::{Action, Actor, Comment, CommentId, EntityKind};
use serde_json::Value;
use store::{ActivityFilter, Reader};

use super::{list, paged, select_fields};
use crate::command::Kind as K;
use crate::command::command;
use crate::error::ArgError;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(ActivityList, "activity.list", "cmd-activity-list", activity_list, read, [opt "task": K::Text, opt "project": K::Text, opt "actor": K::Text, opt "action": K::List, opt "entity": K::Enum(EntityKind::NAMES), opt "limit": K::Int, opt "before": K::Text, opt "fields": K::List]);
command!(CommentAdd, "comment.add", "cmd-comment-add", comment_add, write, [req "task": K::Text, req "body": K::Text, opt "idempotency_key": K::Text]);
command!(Summary, "activity.summary", "cmd-activity-summary", summary, read, [req "since": K::Text, opt "project": K::Text]);
command!(CommentList, "comment.list", "cmd-comment-list", comment_list, read, [req "task": K::Text, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);

const MAX_LIMIT: usize = 500;

fn activity_list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let mut filter = ActivityFilter {
        limit: args
            .parse::<usize>("limit")?
            .unwrap_or(50)
            .clamp(1, MAX_LIMIT),
        before: args.parse("before")?,
        entity: args.parse("entity")?,
        ..ActivityFilter::default()
    };
    for action in args.list("action") {
        let parsed = action
            .parse::<Action>()
            .map_err(|e| EngineError::arg("action", e.reason(&action)))?;
        filter.actions.push(parsed);
    }
    if let Some(task) = args.get("task") {
        filter.task = Some(lookup::task_any(r, task)?.id);
    }
    if let Some(project) = args.get("project") {
        filter.project = Some(lookup::project(r, project)?.id);
    }
    if let Some(actor) = args.get("actor") {
        // Acepta el nombre del agente además de la forma canónica.
        filter.actor = Some(match actor.parse::<Actor>() {
            Ok(a) => a,
            Err(_) => Actor::Agent(lookup::agent(r, actor)?.id),
        });
    }
    let keys = Keys::load(r)?;
    let entries = r.activity(&filter)?;
    // Página llena: puede haber más, a partir de la última entrada.
    let next = (entries.len() == filter.limit)
        .then(|| entries.last().map(|e| e.id.to_string()))
        .flatten();
    let fields = args.list("fields");
    let items = entries
        .iter()
        .map(|e| views::activity(r, &keys, e).map(|v| select_fields(v, &fields)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = list(items);
    if let Some(obj) = out.as_object_mut() {
        obj.insert("next_cursor".into(), serde_json::json!(next));
    }
    Ok(out)
}

fn comment_add(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_scope(cx, task.project_id)?;
    let comment = Comment {
        id: CommentId::generate(),
        task_id: task.id,
        author: cx.ctx.actor.clone(),
        body: rules::comment_body(args.require("body")?)?,
        created_at: cx.now,
    };
    cx.tx.insert_comment(&comment)?;
    super::sync_mentions(cx, task.id, Some(comment.id), &comment.body)?;
    let r: &dyn Reader = &*cx.tx;
    views::comment(r, &Keys::load(r)?, &comment)
}

fn comment_list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let task = lookup::task_any(r, args.require("task")?)?;
    let keys = Keys::load(r)?;
    paged(r.comments(task.id)?, args, |c| views::comment(r, &keys, c))
}

/// Qué pasó desde un instante (RF-SUP-09): tareas terminadas, entregadas a
/// revisión y escaladas, lo creado, los hallazgos y los leases vencidos.
fn summary(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let since: model::Timestamp = args.require("since")?.parse().map_err(|e: jiff::Error| {
        EngineError::arg("since", e.reason(args.get("since").unwrap_or_default()))
    })?;
    let r: &dyn Reader = &*cx.tx;
    let mut filter = ActivityFilter {
        since: Some(since),
        limit: 100_000,
        ..ActivityFilter::default()
    };
    if let Some(project) = args.get("project") {
        filter.project = Some(lookup::project(r, project)?.id);
    }
    let entries = r.activity(&filter)?;
    let keys = Keys::load(r)?;
    let changed_to = |e: &model::ActivityEntry, field: &str, value: &str| {
        e.changes
            .iter()
            .any(|c| c.field == field && c.after.as_str() == Some(value))
    };
    let mut done = Vec::new();
    let mut review = Vec::new();
    let mut escalated = Vec::new();
    let mut created = 0usize;
    let mut comments = 0usize;
    let mut findings = 0usize;
    let mut expired = 0usize;
    let mut agents: Vec<String> = Vec::new();
    let push = |list: &mut Vec<String>, task: Option<model::TaskId>| -> Result<(), EngineError> {
        if let Some(id) = task {
            let key = keys.of_id(r, id)?;
            if !list.contains(&key) {
                list.push(key);
            }
        }
        Ok(())
    };
    for e in entries.iter().filter(|e| e.at > since) {
        if let model::Actor::Agent(_) = e.actor {
            let name = views::actor_name(r, &e.actor)?;
            if !agents.contains(&name) {
                agents.push(name);
            }
        }
        if e.command == "system.expire_leases" {
            expired += 1;
        }
        match (e.entity, e.action) {
            (model::EntityKind::Task, model::Action::Created) => created += 1,
            (model::EntityKind::Comment, model::Action::Created) => comments += 1,
            (model::EntityKind::Inbox, model::Action::Created) => findings += 1,
            (model::EntityKind::Task, model::Action::Updated) => {
                if changed_to(e, "status", "done") {
                    push(&mut done, e.task_id)?;
                }
                if changed_to(e, "status", "in_review") {
                    push(&mut review, e.task_id)?;
                }
                if e.changes
                    .iter()
                    .any(|c| c.field == "needs_human" && c.after.is_string())
                {
                    push(&mut escalated, e.task_id)?;
                }
            }
            _ => {}
        }
    }
    Ok(serde_json::json!({
        "since": since,
        "done": done,
        "in_review": review,
        "escalated": escalated,
        "created": created,
        "comments": comments,
        "findings": findings,
        "expired_leases": expired,
        "agents": agents,
    }))
}
