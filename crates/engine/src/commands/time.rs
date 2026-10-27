//! Tiempo (RF-HIS-03/04/05) y métricas (RF-INT-06). El tiempo de los agentes
//! sale del historial; el del supervisor, de su cronómetro o de lo que anota.

use std::collections::{BTreeMap, HashMap};

use i18n::Msg;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use model::{Actor, Status, TaskId, TimeEntry, TimeEntryId, TimeSource, Timestamp};
use rules::metrics::{self, Interval};
use serde_json::{Value, json};
use store::{ActivityFilter, Reader, TimeFilter};

use super::flow::{Scope, journey};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(TimerStart, "timer.start", "cmd-timer-start", timer_start, write, [req "task": K::Text, opt "note": K::Text]);
command!(
    TimerStop,
    "timer.stop",
    "cmd-timer-stop",
    timer_stop,
    write,
    []
);
command!(Log, "time.log", "cmd-time-log", log, write, [req "task": K::Text, req "minutes": K::Int, opt "date": K::Date, opt "note": K::Text]);
command!(Report, "time.report", "cmd-time-report", report, read, [opt "project": K::Text, opt "task": K::Text, opt "actor": K::Text, opt "days": K::Int, opt "from": K::Date, opt "to": K::Date]);
command!(Sheet, "time.sheet", "cmd-time-sheet", sheet, read, [opt "period": K::Enum(&["day", "week", "month"]), opt "project": K::Text, opt "actor": K::Text, opt "days": K::Int, opt "from": K::Date, opt "to": K::Date]);
command!(Burndown, "metrics.burndown", "cmd-metrics-burndown", burndown, read, [opt "project": K::Text, opt "epic": K::Text, opt "days": K::Int]);
command!(Cfd, "metrics.cfd", "cmd-metrics-cfd", cfd, read, [opt "project": K::Text, opt "days": K::Int]);
command!(Agents, "metrics.agents", "cmd-metrics-agents", agents, read, [opt "days": K::Int]);

fn tz() -> TimeZone {
    TimeZone::system()
}

pub(super) fn day_of(ts: Timestamp) -> Date {
    ts.to_zoned(tz()).date()
}

/// Inicio del día local `d`.
pub(super) fn start_of(d: Date) -> Timestamp {
    d.to_zoned(tz())
        .map(|z| z.timestamp())
        .unwrap_or(Timestamp::UNIX_EPOCH)
}

/// Instantes que delimitan los días `[from, to]`: `[inicio de from, inicio
/// del día siguiente a to)`.
pub(super) fn bounds(from: Date, to: Date, now: Timestamp) -> (Timestamp, Timestamp) {
    (start_of(from), to.tomorrow().map(start_of).unwrap_or(now))
}

pub(super) fn parse_day(args: &Args, key: &str) -> Result<Option<Date>, EngineError> {
    args.get(key)
        .map(|v| crate::args::parse_when(key, v).map(|w| w.date()))
        .transpose()
}

/// Rango `[from, to]` en días locales; por defecto, los últimos `days` días.
pub(super) fn range(
    args: &Args,
    now: Timestamp,
    default_days: i64,
) -> Result<(Date, Date), EngineError> {
    let to = parse_day(args, "to")?.unwrap_or_else(|| day_of(now));
    let days = args
        .parse::<i64>("days")?
        .unwrap_or(default_days)
        .clamp(1, 366);
    let from = match parse_day(args, "from")? {
        Some(d) => d,
        None => to
            .checked_sub(jiff::Span::new().days(days - 1))
            .unwrap_or(to),
    };
    Ok((from.min(to), to.max(from)))
}

fn task_entries(
    r: &dyn Reader,
    project: Option<model::ProjectId>,
) -> Result<Vec<model::ActivityEntry>, EngineError> {
    Ok(r.activity(&ActivityFilter {
        project,
        entity: Some(model::EntityKind::Task),
        limit: usize::MAX >> 1,
        ..ActivityFilter::default()
    })?)
}

fn running(cx: &Cx<'_>) -> Result<Option<TimeEntry>, EngineError> {
    Ok(cx
        .tx
        .time_entries(&TimeFilter {
            actor: Some(cx.ctx.actor.clone()),
            running: true,
            ..TimeFilter::default()
        })?
        .into_iter()
        .next())
}

fn stop(cx: &mut Cx<'_>, mut entry: TimeEntry) -> Result<TimeEntry, EngineError> {
    let mins = cx.now.duration_since(entry.started_at).as_secs() / 60;
    entry.minutes = u32::try_from(mins.max(0)).unwrap_or(u32::MAX);
    entry.running = false;
    cx.tx.update_time_entry(&entry)?;
    Ok(entry)
}

fn entry_view(r: &dyn Reader, keys: &Keys, e: &TimeEntry) -> Result<Value, EngineError> {
    Ok(json!({
        "id": e.id,
        "task": keys.of_id(r, e.task_id)?,
        "actor": views::actor_name(r, &e.actor)?,
        "source": e.source,
        "started_at": e.started_at,
        "minutes": e.minutes,
        "running": e.running,
        "note": e.note,
    }))
}

/// Un cronómetro por actor: empezar otro para el anterior.
fn timer_start(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_scope(cx, task.project_id)?;
    if let Some(previous) = running(cx)? {
        stop(cx, previous)?;
    }
    let entry = TimeEntry {
        id: TimeEntryId::generate(),
        task_id: task.id,
        actor: cx.ctx.actor.clone(),
        source: TimeSource::Timer,
        started_at: cx.now,
        minutes: 0,
        running: true,
        note: args.get("note").unwrap_or_default().to_owned(),
        created_at: cx.now,
    };
    cx.tx.insert_time_entry(&entry)?;
    let r: &dyn Reader = &*cx.tx;
    entry_view(r, &Keys::load(r)?, &entry)
}

fn timer_stop(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    let Some(entry) = running(cx)? else {
        return Err(EngineError::invalid(Msg::new("timer-not-running")));
    };
    let entry = stop(cx, entry)?;
    let r: &dyn Reader = &*cx.tx;
    entry_view(r, &Keys::load(r)?, &entry)
}

fn log(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_scope(cx, task.project_id)?;
    let minutes: u32 = args.parse_required("minutes")?;
    if !(1..=24 * 60).contains(&minutes) {
        return Err(EngineError::arg(
            "minutes",
            Msg::new("arg-range")
                .arg("min", 1u32)
                .arg("max", 24u32 * 60),
        ));
    }
    let started_at = match parse_day(args, "date")? {
        Some(d) => start_of(d),
        None => cx.now,
    };
    let entry = TimeEntry {
        id: TimeEntryId::generate(),
        task_id: task.id,
        actor: cx.ctx.actor.clone(),
        source: TimeSource::Manual,
        started_at,
        minutes,
        running: false,
        note: args.get("note").unwrap_or_default().to_owned(),
        created_at: cx.now,
    };
    cx.tx.insert_time_entry(&entry)?;
    let r: &dyn Reader = &*cx.tx;
    entry_view(r, &Keys::load(r)?, &entry)
}

/// Lo que se cuenta: tramos automáticos (recortados al rango) y registros.
struct Worked {
    actor: Actor,
    task: TaskId,
    day: Date,
    minutes: i64,
    manual: bool,
}

/// Parte un tramo en trozos por día local.
fn split_by_day(i: &Interval, from: Timestamp, to: Timestamp) -> Vec<(Date, i64)> {
    let mut out = Vec::new();
    let mut start = i.start.max(from);
    let end = i.end.min(to);
    while start < end {
        let day = day_of(start);
        let next = day.tomorrow().map(start_of).unwrap_or(end).min(end);
        out.push((day, next.duration_since(start).as_secs() / 60));
        start = next;
    }
    out
}

fn collect(cx: &Cx<'_>, args: &Args, from: Date, to: Date) -> Result<Vec<Worked>, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = args
        .get("project")
        .map(|p| lookup::project(r, p).map(|p| p.id))
        .transpose()?;
    let task = args
        .get("task")
        .map(|t| lookup::task_any(r, t).map(|t| t.id))
        .transpose()?;
    let actor = args
        .get("actor")
        .map(|a| match a.parse::<Actor>() {
            Ok(actor) => Ok(actor),
            Err(_) => lookup::agent(r, a).map(|ag| Actor::Agent(ag.id)),
        })
        .transpose()?;
    let (lo, hi) = bounds(from, to, cx.now);
    let wanted =
        |a: &Actor, t: TaskId| actor.as_ref().is_none_or(|x| x == a) && task.is_none_or(|x| x == t);
    let mut out = Vec::new();
    for i in metrics::worked(&task_entries(r, project)?, cx.now) {
        if !wanted(&i.actor, i.task) {
            continue;
        }
        for (day, minutes) in split_by_day(&i, lo, hi) {
            out.push(Worked {
                actor: i.actor.clone(),
                task: i.task,
                day,
                minutes,
                manual: false,
            });
        }
    }
    let manual = r.time_entries(&TimeFilter {
        task,
        actor: actor.clone(),
        since: Some(lo),
        until: Some(hi),
        ..TimeFilter::default()
    })?;
    for e in manual {
        if let Some(p) = project
            && r.task(e.task_id)?.is_none_or(|t| t.project_id != p)
        {
            continue;
        }
        let minutes = if e.running {
            cx.now.duration_since(e.started_at).as_secs() / 60
        } else {
            i64::from(e.minutes)
        };
        out.push(Worked {
            actor: e.actor,
            task: e.task_id,
            day: day_of(e.started_at),
            minutes,
            manual: true,
        });
    }
    Ok(out)
}

/// Totales por actor y por tarea (RF-HIS-03 y RF-HIS-04).
fn report(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let (from, to) = range(args, cx.now, 30)?;
    let worked = collect(cx, args, from, to)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let mut by_actor: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut by_task: BTreeMap<String, i64> = BTreeMap::new();
    for w in &worked {
        let name = views::actor_name(r, &w.actor)?;
        let slot = by_actor.entry(name).or_default();
        if w.manual {
            slot.1 += w.minutes;
        } else {
            slot.0 += w.minutes;
        }
        *by_task.entry(keys.of_id(r, w.task)?).or_default() += w.minutes;
    }
    let total: i64 = worked.iter().map(|w| w.minutes).sum();
    Ok(json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "total_minutes": total,
        "by_actor": by_actor
            .into_iter()
            .map(|(actor, (auto, manual))| json!({
                "actor": actor, "minutes": auto + manual, "auto": auto, "manual": manual,
            }))
            .collect::<Vec<_>>(),
        "by_task": by_task
            .into_iter()
            .map(|(task, minutes)| json!({ "task": task, "minutes": minutes }))
            .collect::<Vec<_>>(),
    }))
}

/// Etiqueta del periodo de un día: el día, el lunes de su semana o el mes.
fn period_of(day: Date, period: &str) -> String {
    match period {
        "week" => {
            let back = i64::from(day.weekday().to_monday_zero_offset());
            day.checked_sub(jiff::Span::new().days(back))
                .unwrap_or(day)
                .to_string()
        }
        "month" => format!("{:04}-{:02}", day.year(), day.month()),
        _ => day.to_string(),
    }
}

/// Hoja de horas (RF-HIS-05): minutos por periodo, proyecto, tarea y actor.
fn sheet(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let period = args.get("period").unwrap_or("day");
    let (from, to) = range(args, cx.now, if period == "month" { 180 } else { 30 })?;
    let worked = collect(cx, args, from, to)?;
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let mut rows: BTreeMap<(String, String, String, String), i64> = BTreeMap::new();
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut project_of: HashMap<TaskId, String> = HashMap::new();
    for w in &worked {
        let label = period_of(w.day, period);
        let project = match project_of.get(&w.task) {
            Some(p) => p.clone(),
            None => {
                let key = keys.of_id(r, w.task)?;
                let p = key
                    .rsplit_once('-')
                    .map(|(p, _)| p.to_owned())
                    .unwrap_or_default();
                project_of.insert(w.task, p.clone());
                p
            }
        };
        let row = (
            label.clone(),
            project,
            keys.of_id(r, w.task)?,
            views::actor_name(r, &w.actor)?,
        );
        *rows.entry(row).or_default() += w.minutes;
        *totals.entry(label).or_default() += w.minutes;
    }
    Ok(json!({
        "period": period,
        "from": from.to_string(),
        "to": to.to_string(),
        "rows": rows
            .into_iter()
            .map(|((period, project, task, actor), minutes)| json!({
                "period": period, "project": project, "task": task, "actor": actor, "minutes": minutes,
            }))
            .collect::<Vec<_>>(),
        "totals": totals
            .into_iter()
            .map(|(period, minutes)| json!({ "period": period, "minutes": minutes }))
            .collect::<Vec<_>>(),
    }))
}

/// Días del rango con el instante en que acaba cada uno.
pub(super) fn day_cuts(from: Date, to: Date, now: Timestamp) -> Vec<(Date, Timestamp)> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        let end = d.tomorrow().map(start_of).unwrap_or(now).min(now);
        out.push((d, end));
        match d.tomorrow() {
            Ok(next) => d = next,
            Err(_) => break,
        }
    }
    out
}

/// Pendiente y hecho al final de cada día (RF-INT-06), de un proyecto, de
/// una épica o de todos los proyectos del alcance. Sale del recorrido (D24).
fn burndown(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let scope = Scope::load(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let epic = args
        .get("epic")
        .map(|e| lookup::epic(r, e, scope.project.as_ref().map(|p| p.id), None))
        .transpose()?;
    let (from, to) = range(args, cx.now, 30)?;
    let cuts = day_cuts(from, to, cx.now);
    let counts = metrics::status_counts(
        &journey(r, &scope.projects, epic.map(|e| e.id), None)?,
        &cuts.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
    );
    let series: Vec<Value> = cuts
        .iter()
        .zip(counts)
        .map(|((day, _), count)| {
            let mut remaining = 0;
            let mut done = 0;
            for (status, n) in count {
                match status {
                    Status::Done => done += n,
                    Status::Cancelled => {}
                    _ => remaining += n,
                }
            }
            json!({ "date": day.to_string(), "remaining": remaining, "done": done })
        })
        .collect();
    Ok(json!({ "project": scope.key(), "series": series }))
}

/// Flujo acumulado (RF-INT-06): tareas en cada estado base al final de cada
/// día, de un proyecto o de todos los del alcance. Sale del recorrido (D24).
fn cfd(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let scope = Scope::load(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let (from, to) = range(args, cx.now, 30)?;
    let cuts = day_cuts(from, to, cx.now);
    let counts = metrics::status_counts(
        &journey(r, &scope.projects, None, None)?,
        &cuts.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
    );
    let series: Vec<Value> = cuts
        .iter()
        .zip(counts)
        .map(|((day, _), count)| {
            let mut row = serde_json::Map::new();
            row.insert("date".into(), json!(day.to_string()));
            for s in Status::ALL {
                let n = count.get(s).copied().unwrap_or(0);
                row.insert(s.as_str().into(), json!(n));
            }
            Value::Object(row)
        })
        .collect();
    Ok(json!({ "project": scope.key(), "series": series }))
}

/// Rendimiento por agente (RF-INT-06): entregas, aprobadas, rechazadas, tasa
/// de rechazo, ciclo medio y minutos trabajados en el periodo.
fn agents(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let (from, to) = range(args, cx.now, 30)?;
    let (lo, hi) = bounds(from, to, cx.now);
    let entries = task_entries(r, None)?;
    let stats = metrics::agent_stats(&entries, lo, hi);
    let mut minutes: HashMap<model::AgentId, i64> = HashMap::new();
    for i in metrics::worked(&entries, cx.now) {
        if let Actor::Agent(a) = i.actor {
            let clipped: i64 = split_by_day(&i, lo, hi).iter().map(|(_, m)| m).sum();
            *minutes.entry(a).or_default() += clipped;
        }
    }
    let mut items = Vec::new();
    for agent in r.agents()? {
        let s = stats.get(&agent.id).cloned().unwrap_or_default();
        items.push(json!({
            "agent": agent.name,
            "submitted": s.submitted,
            "approved": s.approved,
            "rejected": s.rejected,
            "rejection_rate": s.rejection_rate(),
            "avg_cycle_minutes": s.avg_cycle_minutes,
            "minutes": minutes.get(&agent.id).copied().unwrap_or(0),
        }));
    }
    Ok(json!({ "from": from.to_string(), "to": to.to_string(), "items": items }))
}
