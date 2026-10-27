//! Automatizaciones (RF-INT-03) y webhooks (RF-INT-02). Solo el supervisor
//! las configura: una automatización actúa como System y un webhook saca
//! datos del equipo.

use std::collections::BTreeMap;

use i18n::Msg;
use model::{Automation, AutomationId, DeliveryStatus, Status, Trigger, Webhook, WebhookId};
use serde_json::{Value, json};
use store::Reader;

use super::agent::supervisor_only;
use super::{save, view};
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, Registry, lookup, triggers};

command!(Create, "automation.create", "cmd-automation-create", create, write, [opt "name": K::Text, opt "trigger": K::Enum(Trigger::NAMES), opt "condition": K::Text, opt "command": K::Text, opt "args": K::Json, opt "project": K::Text, opt "preset": K::Enum(PRESETS)]);
command!(
    List,
    "automation.list",
    "cmd-automation-list",
    list,
    read,
    []
);
command!(Update, "automation.update", "cmd-automation-update", update, write, [req "automation": K::Text, opt "enabled": K::Bool, opt "condition": K::Text]);
command!(Delete, "automation.delete", "cmd-automation-delete", delete, write, [req "automation": K::Text]);
command!(ReviewParent, "task.review_parent", "cmd-task-review_parent", review_parent, write, [req "task": K::Text]);
command!(WebhookCreate, "webhook.create", "cmd-webhook-create", webhook_create, write, [req "url": K::Text, opt "triggers": K::List]);
command!(
    WebhookList,
    "webhook.list",
    "cmd-webhook-list",
    webhook_list,
    read,
    []
);
command!(WebhookDelete, "webhook.delete", "cmd-webhook-delete", webhook_delete, write, [req "webhook": K::Text]);
command!(Deliveries, "webhook.deliveries", "cmd-webhook-deliveries", deliveries, read, [opt "webhook": K::Text, opt "limit": K::Int]);
command!(Record, "webhook.record", "cmd-webhook-record", record, write, [req "delivery": K::Text, req "ok": K::Bool, opt "error": K::Text]);

/// Reglas que vienen hechas.
const PRESETS: &[&str] = &["parent_review"];

fn find(r: &dyn Reader, reference: &str) -> Result<Automation, EngineError> {
    r.automations()?
        .into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(reference) || a.id.to_string() == reference)
        .ok_or_else(|| {
            EngineError::not_found(Msg::new("notfound-automation").arg("automation", reference))
        })
}

fn automation_json(r: &dyn Reader, a: &Automation) -> Result<Value, EngineError> {
    let project = a
        .project_id
        .map(|p| r.project(p).map(|p| p.map(|p| p.key)))
        .transpose()?
        .flatten();
    Ok(json!({
        "id": a.id,
        "name": a.name,
        "trigger": a.trigger,
        "condition": a.condition,
        "command": a.command,
        "args": a.args,
        "enabled": a.enabled,
        "project": project,
    }))
}

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    // Hoy hay una sola regla hecha (el parámetro solo admite `PRESETS`):
    // «todas las hijas cerradas → padre a revisión» (RF-INT-03).
    let preset = args.get("preset").map(|_| {
        (
            "parent_review",
            Trigger::StatusChanged,
            "closed:true parent!=none",
            "task.review_parent",
            BTreeMap::from([("task".to_owned(), "{parent}".to_owned())]),
        )
    });
    let (name, trigger, condition, command, call_args) = match preset {
        Some((n, t, c, cmd, a)) => (
            args.get("name").unwrap_or(n).to_owned(),
            t,
            c.to_owned(),
            cmd.to_owned(),
            a,
        ),
        None => {
            let raw: BTreeMap<String, Value> = match args.get("args") {
                Some(json) => serde_json::from_str(json).map_err(|e| {
                    EngineError::arg("args", Msg::new("arg-json").arg("detail", e.to_string()))
                })?,
                None => BTreeMap::new(),
            };
            let call_args = raw
                .into_iter()
                .map(|(k, v)| {
                    let text = match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (k, text)
                })
                .collect();
            (
                args.require("name")?.trim().to_owned(),
                args.parse("trigger")?
                    .ok_or_else(|| EngineError::MissingArg("trigger".into()))?,
                args.get("condition").unwrap_or_default().trim().to_owned(),
                args.require("command")?.trim().to_owned(),
                call_args,
            )
        }
    };
    if Registry::standard().get(&command).is_none() {
        return Err(EngineError::arg(
            "command",
            Msg::new("automation-command").arg("command", command.as_str()),
        ));
    }
    view::parse(&condition)?;
    let project_id = args
        .get("project")
        .map(|p| lookup::project(&*cx.tx, p).map(|p| p.id))
        .transpose()?;
    let existing = find(&*cx.tx, &name).ok();
    let automation = Automation {
        id: existing
            .as_ref()
            .map_or_else(AutomationId::generate, |a| a.id),
        name,
        trigger,
        condition,
        command,
        args: call_args,
        enabled: true,
        project_id,
        created_at: existing.as_ref().map_or(cx.now, |a| a.created_at),
        updated_at: cx.now,
    };
    cx.tx.save_automation(&automation)?;
    automation_json(&*cx.tx, &automation)
}

fn list(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let items = r
        .automations()?
        .iter()
        .map(|a| automation_json(r, a))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "count": items.len(), "items": items }))
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let mut a = find(&*cx.tx, args.require("automation")?)?;
    if args.get("enabled").is_some() {
        a.enabled = args.flag("enabled")?;
    }
    if let Some(c) = args.get("condition") {
        view::parse(c)?;
        a.condition = c.trim().to_owned();
    }
    a.updated_at = cx.now;
    cx.tx.save_automation(&a)?;
    automation_json(&*cx.tx, &a)
}

fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let a = find(&*cx.tx, args.require("automation")?)?;
    cx.tx.delete_automation(a.id)?;
    Ok(json!({ "deleted": a.name }))
}

/// Si todas las subtareas están cerradas y el padre sigue abierto, el
/// padre pasa a revisión: lo cierra una persona (D04).
fn review_parent(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut parent = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_write(cx, &parent, None)?;
    let children: Vec<_> = cx
        .tx
        .children(parent.id)?
        .into_iter()
        .filter(|c| c.deleted_at.is_none())
        .collect();
    let all_closed =
        !children.is_empty() && children.iter().all(|c| c.status.category().is_closed());
    let open = matches!(
        parent.status,
        Status::Backlog | Status::Todo | Status::InProgress
    );
    let moved = all_closed && open;
    if moved {
        parent.status = Status::InReview;
        parent.stage = None;
        save(cx, &mut parent)?;
    }
    Ok(json!({ "task": args.require("task")?, "moved": moved, "status": parent.status }))
}

fn find_webhook(r: &dyn Reader, reference: &str) -> Result<Webhook, EngineError> {
    r.webhooks()?
        .into_iter()
        .find(|w| w.id.to_string() == reference || w.url == reference)
        .ok_or_else(|| {
            EngineError::not_found(Msg::new("notfound-webhook").arg("webhook", reference))
        })
}

fn webhook_create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let url = args.require("url")?.trim().to_owned();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(EngineError::arg("url", Msg::new("webhook-url")));
    }
    let mut triggers = Vec::new();
    for t in args.list("triggers") {
        triggers.push(
            t.parse::<Trigger>()
                .map_err(|e| EngineError::arg("triggers", i18n::Localize::msg(&e)))?,
        );
    }
    if triggers.is_empty() {
        triggers = Trigger::ALL.to_vec();
    }
    let secret = foundation::secret::generate_secret()
        .ok_or_else(|| EngineError::invalid(Msg::new("token-random")))?;
    let hook = Webhook {
        id: WebhookId::generate(),
        url,
        secret: secret.clone(),
        triggers,
        enabled: true,
        created_at: cx.now,
    };
    cx.tx.save_webhook(&hook)?;
    // El secreto se enseña una sola vez, como los tokens (D09).
    Ok(json!({ "id": hook.id, "url": hook.url, "triggers": hook.triggers, "secret": secret }))
}

fn webhook_list(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    let items: Vec<Value> = cx
        .tx
        .webhooks()?
        .iter()
        .map(|w| json!({ "id": w.id, "url": w.url, "triggers": w.triggers, "enabled": w.enabled }))
        .collect();
    Ok(json!({ "count": items.len(), "items": items }))
}

fn webhook_delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let w = find_webhook(&*cx.tx, args.require("webhook")?)?;
    cx.tx.delete_webhook(w.id)?;
    Ok(json!({ "deleted": w.url }))
}

fn deliveries(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let webhook = args
        .get("webhook")
        .map(|w| find_webhook(r, w).map(|w| w.id))
        .transpose()?;
    let limit = args.parse::<usize>("limit")?.unwrap_or(50).clamp(1, 500);
    let items: Vec<Value> = r
        .deliveries(webhook, limit)?
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "webhook": d.webhook_id,
                "event": d.trigger,
                "status": d.status,
                "attempts": d.attempts,
                "next_at": d.next_at,
                "last_error": d.last_error,
                "created_at": d.created_at,
            })
        })
        .collect();
    Ok(json!({ "count": items.len(), "items": items }))
}

/// Resultado de un intento de envío; lo anota el daemon (System).
fn record(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let id = args.require("delivery")?;
    let mut d = id
        .parse()
        .ok()
        .map(|id| cx.tx.delivery(id))
        .transpose()?
        .flatten()
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-delivery").arg("delivery", id)))?;
    if d.status != DeliveryStatus::Pending {
        return Ok(json!({ "delivery": id, "status": d.status }));
    }
    triggers::after_attempt(
        &mut d,
        args.flag("ok")?,
        args.get("error").unwrap_or_default(),
        cx.now,
    );
    cx.tx.save_delivery(&d)?;
    Ok(json!({ "delivery": id, "status": d.status, "attempts": d.attempts, "next_at": d.next_at }))
}
