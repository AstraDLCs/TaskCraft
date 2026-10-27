//! Métricas de flujo (TC-5): `metrics.flow` y `task.timeline`, sobre el
//! recorrido guardado de las tareas (D24). Solo trabajo: nada de costes.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

use model::{Actor, CycleId, EpicId, Project, StatusDef, Transition};
use rules::metrics::{self, AgentFlow};
use serde_json::{Value, json};
use store::{Reader, TransitionFilter};

use super::time::{bounds, day_cuts, day_of, range, start_of};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(Flow, "metrics.flow", "cmd-metrics-flow", flow, read, [opt "project": K::Text, opt "epic": K::Text, opt "cycle": K::Text, opt "days": K::Int, opt "from": K::Date, opt "to": K::Date]);
command!(Timeline, "task.timeline", "cmd-task-timeline", timeline, read, [req "task": K::Text]);

/// Proyectos que mide una métrica: el indicado o, sin él, todos los activos
/// del alcance de quien pregunta (D11, D15).
pub(super) struct Scope {
    pub project: Option<Project>,
    pub projects: Vec<Project>,
}

impl Scope {
    pub fn load(cx: &Cx<'_>, args: &Args) -> Result<Self, EngineError> {
        let r: &dyn Reader = &*cx.tx;
        if let Some(p) = args.get("project") {
            let project = lookup::project(r, p)?;
            return Ok(Self {
                projects: vec![project.clone()],
                project: Some(project),
            });
        }
        let mut projects = r.projects(false)?;
        let agent = crate::guard::agent_id(cx)
            .map(|id| r.agent(id))
            .transpose()?
            .flatten();
        if let Some(a) = agent
            && let Some(scope) = crate::identity::scope(r, &a)?
        {
            projects.retain(|p| scope.contains(&p.id));
        }
        Ok(Self {
            project: None,
            projects,
        })
    }

    /// Clave del proyecto indicado; `null` = todos.
    pub fn key(&self) -> Value {
        json!(self.project.as_ref().map(|p| &p.key))
    }
}

/// Recorrido de las tareas de esos proyectos que siguen fuera de la
/// papelera; con `epic` o `cycle`, solo de las suyas. Se filtra en la
/// consulta: no se cargan tareas ni proyectos fuera del alcance.
pub(super) fn journey(
    r: &dyn Reader,
    projects: &[Project],
    epic: Option<EpicId>,
    cycle: Option<CycleId>,
) -> Result<Vec<Transition>, EngineError> {
    // Sin proyectos en el alcance no hay nada (vacío en el filtro = todos).
    if projects.is_empty() {
        return Ok(Vec::new());
    }
    Ok(r.transitions(&TransitionFilter {
        projects: projects.iter().map(|p| p.id).collect(),
        live: true,
        group: epic,
        cycle,
        ..TransitionFilter::default()
    })?)
}

fn round(x: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (x * f).round() / f
}

/// Nombre de una columna: el suyo o, si es un estado base sin renombrar, su
/// clave (la interfaz la traduce).
fn column_name(d: &StatusDef) -> String {
    if d.name.is_empty() {
        d.key.clone()
    } else {
        d.name.clone()
    }
}

fn owner_name(r: &dyn Reader, d: &StatusDef) -> Result<Option<String>, EngineError> {
    d.owner
        .map(|a| views::actor_name(r, &Actor::Agent(a)))
        .transpose()
}

/// Lead y cycle time, throughput, relevos, devoluciones, WIP, y lo de cada
/// agente y cada columna en el periodo.
fn flow(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let scope = Scope::load(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    let pid = scope.project.as_ref().map(|p| p.id);
    let epic = args
        .get("epic")
        .map(|e| lookup::epic(r, e, pid, None))
        .transpose()?;
    let cycle = args
        .get("cycle")
        .map(|c| super::cycle::find(r, c, scope.project.as_ref(), day_of(cx.now)))
        .transpose()?;
    // Con un ciclo y sin fechas, el periodo es el del ciclo.
    let (from, to) = match &cycle {
        Some(c) if ["from", "to", "days"].iter().all(|k| args.get(k).is_none()) => (c.start, c.end),
        _ => range(args, cx.now, 30)?,
    };
    let (lo, hi) = bounds(from, to, cx.now);
    let transitions = journey(r, &scope.projects, epic.map(|e| e.id), cycle.map(|c| c.id))?;
    let f = metrics::flow(&transitions, &scope.projects, lo, hi, cx.now);

    let days = day_cuts(from, to, cx.now);
    let mut per_day: BTreeMap<jiff::civil::Date, usize> = BTreeMap::new();
    for d in &f.done {
        *per_day.entry(day_of(d.at)).or_default() += 1;
    }
    let throughput: Vec<Value> = days
        .iter()
        .map(
            |(d, _)| json!({ "date": d.to_string(), "done": per_day.get(d).copied().unwrap_or(0) }),
        )
        .collect();
    // Solo los días ya empezados: los futuros no rebajan el ritmo.
    let elapsed = days.iter().filter(|(d, _)| start_of(*d) <= cx.now).count();
    let weeks = elapsed.max(1) as f64 / 7.0;

    let mut rows: Vec<(&Actor, &AgentFlow)> = f
        .agents
        .iter()
        .filter(|(_, a)| **a != AgentFlow::default())
        .collect();
    rows.sort_by_key(|(actor, a)| {
        (
            Reverse(a.done),
            Reverse(a.worked_minutes),
            actor.to_string(),
        )
    });
    let agents = rows
        .into_iter()
        .map(|(actor, a)| {
            Ok(json!({
                "actor": actor.to_string(),
                "actor_name": views::actor_name(r, actor)?,
                "done": a.done,
                "worked_minutes": a.worked_minutes,
                "queued_minutes": a.queued_minutes,
                "returns_received": a.returns_received,
                "returns_sent": a.returns_sent,
            }))
        })
        .collect::<Result<Vec<_>, EngineError>>()?;

    // Las columnas de todos los flujos, una vez cada clave, en su orden.
    let mut seen: HashSet<String> = HashSet::new();
    let mut columns = Vec::new();
    for p in &scope.projects {
        for d in rules::workflow(p) {
            if !seen.insert(d.key.clone()) {
                continue;
            }
            let stats = f.columns.get(&d.key).cloned().unwrap_or_default();
            columns.push(json!({
                "key": d.key,
                "name": column_name(&d),
                "base": d.base,
                "owner": owner_name(r, &d)?,
                "tasks_now": stats.tasks_now,
                "avg_minutes": stats.avg_minutes(),
            }));
        }
    }

    Ok(json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "summary": {
            "done": f.done.len(),
            "throughput_per_week": round(f.done.len() as f64 / weeks, 1),
            "lead_p50_minutes": f.lead(50),
            "lead_p85_minutes": f.lead(85),
            "cycle_p50_minutes": f.cycle(50),
            "cycle_p85_minutes": f.cycle(85),
            "handoffs": f.handoffs,
            "returns": f.returns,
            "returned_pct": round(f.returned_pct(), 1),
            "wip_avg": round(f.wip_avg, 2),
        },
        "throughput": throughput,
        "agents": agents,
        "columns": columns,
    }))
}

/// Recorrido de una tarea tramo a tramo: en qué columna, de quién, quién la
/// llevó y cuánto estuvo. Abierta, el tramo actual cuenta hasta ahora;
/// cerrada (hecha o cancelada), el reloj se para en `closed_at`.
fn timeline(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let task = lookup::task_any(r, args.require("task")?)?;
    let project = r.project(task.project_id)?;
    let flow = project.as_ref().map(rules::workflow).unwrap_or_default();
    let segs = metrics::segments(&r.transitions(&TransitionFilter {
        task: Some(task.id),
        ..TransitionFilter::default()
    })?);
    let created_at = segs.first().map_or(task.created_at, |s| s.entered);
    let done_at = metrics::done_at(&segs);
    let closed_at = metrics::closed_at(&segs);
    let end = closed_at.unwrap_or(cx.now);
    // Los mismos actores y dueños se repiten tramo a tramo: un nombre por actor.
    let mut names: HashMap<Actor, String> = HashMap::new();
    let mut name = |a: &Actor| -> Result<String, EngineError> {
        if let Some(n) = names.get(a) {
            return Ok(n.clone());
        }
        let n = views::actor_name(r, a)?;
        names.insert(a.clone(), n.clone());
        Ok(n)
    };
    let mut steps = Vec::with_capacity(segs.len());
    for s in &segs {
        let def = flow.iter().find(|d| d.key == s.column());
        let owner = def
            .and_then(|d| d.owner)
            .map(|a| name(&Actor::Agent(a)))
            .transpose()?;
        steps.push(json!({
            "status": s.status,
            "stage": s.stage,
            "column": def.map_or_else(|| s.column().to_owned(), column_name),
            "owner": owner,
            "entered_at": s.entered,
            "left_at": s.left,
            "minutes": s.minutes(end.max(s.entered)),
            "actor": s.actor.to_string(),
            "actor_name": name(&s.actor)?,
        }));
    }
    Ok(json!({
        "task": Keys::load(r)?.of(&task),
        "created_at": created_at,
        "done_at": done_at,
        "closed_at": closed_at,
        "total_minutes": end.duration_since(created_at).as_secs() / 60,
        "steps": steps,
    }))
}
