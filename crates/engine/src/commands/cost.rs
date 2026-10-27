//! Coste reportado por los agentes (RF-HIS-06): tokens e importe por tarea,
//! agregados por tarea (con sus subtareas), épica o proyecto.

use std::collections::{BTreeMap, HashSet};

use i18n::Msg;
use model::{Actor, CostEntry, CostEntryId, TaskId};
use rules::cost::{currency, format_amount, parse_amount};
use serde_json::{Value, json};
use store::{CostFilter, Reader, TaskFilter};

use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(CostReport, "cost.report", "cmd-cost-report", report, write, [req "task": K::Text, opt "tokens_in": K::Int, opt "tokens_out": K::Int, opt "amount": K::Text, opt "currency": K::Text, opt "model": K::Text, opt "note": K::Text]);
command!(CostList, "cost.list", "cmd-cost-list", list, read, [opt "task": K::Text, opt "epic": K::Text, opt "project": K::Text, opt "actor": K::Text, opt "from": K::Date, opt "to": K::Date, opt "days": K::Int]);
command!(CostSummary, "cost.summary", "cmd-cost-summary", summary, read, [opt "task": K::Text, opt "epic": K::Text, opt "project": K::Text, opt "actor": K::Text, opt "from": K::Date, opt "to": K::Date, opt "days": K::Int, opt "group": K::Enum(&["agent", "model", "task", "day"])]);

/// Moneda si el reporte no dice otra.
pub const DEFAULT_CURRENCY: &str = "USD";

fn report(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_scope(cx, task.project_id)?;
    let tokens_in: u64 = args.parse("tokens_in")?.unwrap_or(0);
    let tokens_out: u64 = args.parse("tokens_out")?.unwrap_or(0);
    let amount_micros = args.get("amount").map(parse_amount).transpose()?;
    if tokens_in == 0 && tokens_out == 0 && amount_micros.unwrap_or(0) == 0 {
        return Err(EngineError::invalid(Msg::new("cost-empty")));
    }
    let entry = CostEntry {
        id: CostEntryId::generate(),
        task_id: task.id,
        actor: cx.ctx.actor.clone(),
        model: args.get("model").unwrap_or_default().trim().to_owned(),
        tokens_in,
        tokens_out,
        amount_micros: amount_micros.unwrap_or(0),
        currency: currency(args.get("currency").unwrap_or(DEFAULT_CURRENCY))?,
        note: args.get("note").unwrap_or_default().to_owned(),
        at: cx.now,
    };
    cx.tx.insert_cost_entry(&entry)?;
    let r: &dyn Reader = &*cx.tx;
    entry_view(r, &Keys::load(r)?, &entry)
}

fn entry_view(r: &dyn Reader, keys: &Keys, e: &CostEntry) -> Result<Value, EngineError> {
    Ok(json!({
        "id": e.id,
        "task": keys.of_id(r, e.task_id)?,
        "actor": views::actor_name(r, &e.actor)?,
        "model": e.model,
        "tokens_in": e.tokens_in,
        "tokens_out": e.tokens_out,
        "amount": format_amount(e.amount_micros),
        "currency": e.currency,
        "note": e.note,
        "at": e.at,
    }))
}

/// Los costes que piden los filtros: una tarea con sus subtareas, una épica,
/// un proyecto, un actor y un rango de días.
fn select(cx: &Cx<'_>, args: &Args) -> Result<Vec<CostEntry>, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = args
        .get("project")
        .map(|p| lookup::project(r, p))
        .transpose()?;
    let mut only: Option<HashSet<TaskId>> = None;
    if let Some(t) = args.get("task") {
        let root = lookup::task_any(r, t)?;
        only = Some(lookup::subtree(r, root)?.iter().map(|t| t.id).collect());
    }
    if let Some(e) = args.get("epic") {
        let epic = lookup::epic(r, e, project.as_ref().map(|p| p.id), None)?;
        let ids: HashSet<TaskId> = r
            .tasks(&TaskFilter {
                include_archived: true,
                group: Some(epic.id),
                ..TaskFilter::default()
            })?
            .iter()
            .map(|t| t.id)
            .collect();
        only = Some(match only {
            Some(prev) => prev.intersection(&ids).copied().collect(),
            None => ids,
        });
    }
    let actor = args
        .get("actor")
        .map(|a| match a.parse::<Actor>() {
            Ok(actor) => Ok(actor),
            Err(_) => lookup::agent(r, a).map(|ag| Actor::Agent(ag.id)),
        })
        .transpose()?;
    // Sin `from`, `days` cuenta hacia atrás desde hoy; sin ninguno, todo.
    let from = match (
        super::time::parse_day(args, "from")?,
        args.parse::<i64>("days")?,
    ) {
        (Some(d), _) => Some(d),
        (None, Some(days)) => {
            let today = super::time::day_of(cx.now);
            today
                .checked_sub(jiff::Span::new().days(days.clamp(1, 3660) - 1))
                .ok()
        }
        (None, None) => None,
    };
    let since = from.map(super::time::start_of);
    let until = super::time::parse_day(args, "to")?
        .map(|d| d.tomorrow().map(super::time::start_of).unwrap_or(cx.now));
    Ok(r.cost_entries(&CostFilter {
        task: None,
        project: project.map(|p| p.id),
        actor,
        since,
        until,
    })?
    .into_iter()
    .filter(|e| only.as_ref().is_none_or(|ids| ids.contains(&e.task_id)))
    .collect())
}

fn list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let entries = select(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let items = entries
        .iter()
        .map(|e| entry_view(r, &keys, e))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items, "total": totals(&entries) }))
}

/// Suma de tokens e importes por moneda.
#[derive(Default)]
struct Sum {
    entries: usize,
    tokens_in: u64,
    tokens_out: u64,
    amounts: BTreeMap<String, u64>,
}

impl Sum {
    fn add(&mut self, e: &CostEntry) {
        self.entries += 1;
        self.tokens_in = self.tokens_in.saturating_add(e.tokens_in);
        self.tokens_out = self.tokens_out.saturating_add(e.tokens_out);
        let slot = self.amounts.entry(e.currency.clone()).or_default();
        *slot = slot.saturating_add(e.amount_micros);
    }

    fn view(&self) -> Value {
        json!({
            "entries": self.entries,
            "tokens_in": self.tokens_in,
            "tokens_out": self.tokens_out,
            "amounts": self
                .amounts
                .iter()
                .filter(|(_, m)| **m > 0)
                .map(|(c, m)| json!({ "currency": c, "amount": format_amount(*m) }))
                .collect::<Vec<_>>(),
        })
    }
}

fn totals(entries: &[CostEntry]) -> Value {
    let mut sum = Sum::default();
    entries.iter().for_each(|e| sum.add(e));
    sum.view()
}

fn summary(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let group = args.get("group").unwrap_or("agent");
    let entries = select(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let mut groups: BTreeMap<String, Sum> = BTreeMap::new();
    for e in &entries {
        let key = match group {
            "model" => e.model.clone(),
            "task" => keys.of_id(r, e.task_id)?,
            "day" => super::time::day_of(e.at).to_string(),
            _ => views::actor_name(r, &e.actor)?,
        };
        groups.entry(key).or_default().add(e);
    }
    Ok(json!({
        "group": group,
        "total": totals(&entries),
        "groups": groups
            .iter()
            .map(|(key, sum)| {
                let mut v = sum.view();
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("key".into(), json!(key));
                }
                v
            })
            .collect::<Vec<_>>(),
    }))
}
