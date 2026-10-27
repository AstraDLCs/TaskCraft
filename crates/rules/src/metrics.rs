//! Tiempo y métricas a partir del historial (RF-HIS-03, RF-INT-06): se
//! reproduce, tarea a tarea, cómo cambiaron su estado, su claim y su
//! asignado. Sin columnas nuevas: el historial ya lo tiene todo.

use std::collections::{BTreeMap, HashMap};

use model::{Action, ActivityEntry, Actor, AgentId, EntityKind, Status, TaskId, Timestamp};
use serde_json::Value;

// Métricas de flujo sobre el recorrido guardado (TC-5).
mod flow;
pub use flow::{
    AgentFlow, ColumnFlow, Done, Flow, Segment, closed_at, done_at, flow, percentile, segments,
    status_counts,
};

/// Un tramo en el que una tarea estuvo en curso a cargo de `actor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval {
    pub actor: Actor,
    pub task: TaskId,
    pub start: Timestamp,
    pub end: Timestamp,
}

impl Interval {
    pub fn minutes(&self) -> i64 {
        self.end.duration_since(self.start).as_secs() / 60
    }
}

#[derive(Debug, Clone, Default)]
struct State {
    status: Option<Status>,
    claim: Option<AgentId>,
    assignee: Option<Actor>,
    since: Option<Timestamp>,
}

impl State {
    /// Quién trabaja en la tarea ahora: quien la tiene tomada o, si nadie,
    /// su asignado; solo mientras está en curso.
    fn owner(&self) -> Option<Actor> {
        if self.status.map(|s| s.category()) != Some(model::Category::Active) {
            return None;
        }
        self.claim
            .map(Actor::Agent)
            .or_else(|| self.assignee.clone())
    }

    fn apply(&mut self, e: &ActivityEntry) {
        for c in &e.changes {
            match c.field.as_str() {
                "status" => self.status = c.after.as_str().and_then(|s| s.parse().ok()),
                "claim" => {
                    self.claim = c
                        .after
                        .get("agent")
                        .and_then(Value::as_str)
                        .and_then(|a| a.parse().ok());
                }
                // Historial anterior a D15: un solo asignado.
                "assignee" => self.assignee = c.after.as_str().and_then(|a| a.parse().ok()),
                // El primero de la lista: un actor (D20) o, en historial de
                // D15, el primer `{actor, role}` que era ejecutor.
                "assignees" => {
                    self.assignee = c.after.as_array().and_then(|list| {
                        list.iter()
                            .find_map(|a| match a {
                                Value::String(s) => Some(s.as_str()),
                                _ if a.get("role").and_then(Value::as_str) == Some("executor") => {
                                    a.get("actor").and_then(Value::as_str)
                                }
                                _ => None,
                            })
                            .and_then(|a| a.parse().ok())
                    });
                }
                // En la papelera no se trabaja.
                "deleted_at" if !c.after.is_null() => self.status = None,
                _ => {}
            }
        }
    }
}

fn is_task_change(e: &ActivityEntry) -> bool {
    e.entity == EntityKind::Task && e.task_id.is_some()
}

/// Tramos de trabajo de todas las tareas del historial, hasta `now`.
/// `entries` puede venir en cualquier orden.
pub fn worked(entries: &[ActivityEntry], now: Timestamp) -> Vec<Interval> {
    let mut sorted: Vec<&ActivityEntry> = entries.iter().filter(|e| is_task_change(e)).collect();
    sorted.sort_by_key(|e| (e.at, e.id));
    let mut states: HashMap<TaskId, State> = HashMap::new();
    let mut out = Vec::new();
    for e in sorted {
        let Some(task) = e.task_id else { continue };
        let state = states.entry(task).or_default();
        if let (Some(actor), Some(start)) = (state.owner(), state.since)
            && e.at > start
        {
            out.push(Interval {
                actor,
                task,
                start,
                end: e.at,
            });
        }
        if e.action == Action::Deleted && e.changes.is_empty() {
            state.status = None;
        }
        state.apply(e);
        state.since = Some(e.at);
    }
    for (task, state) in states {
        if let (Some(actor), Some(start)) = (state.owner(), state.since)
            && now > start
        {
            out.push(Interval {
                actor,
                task,
                start,
                end: now,
            });
        }
    }
    out
}

/// Un paso del recorrido de una tarea reconstruido del historial (D24), para
/// las tareas de antes de que el recorrido se guardara aparte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub task: TaskId,
    /// `None` en el primer paso: la tarea se creó así.
    pub from: Option<(Status, Option<String>)>,
    pub to: (Status, Option<String>),
    pub actor: Actor,
    pub at: Timestamp,
}

/// El recorrido de cada tarea según el historial: un paso al crearse y otro
/// por cada cambio de estado o de columna. Si falta el principio (historial
/// podado), el estado de partida sale del valor anterior de cada cambio.
pub fn journey(entries: &[ActivityEntry]) -> Vec<Step> {
    let mut sorted: Vec<&ActivityEntry> = entries.iter().filter(|e| is_task_change(e)).collect();
    sorted.sort_by_key(|e| (e.at, e.id));
    let mut states: HashMap<TaskId, (Status, Option<String>)> = HashMap::new();
    let mut out = Vec::new();
    for e in sorted {
        let Some(task) = e.task_id else { continue };
        let known = states.get(&task).cloned();
        let field = |name: &str| e.changes.iter().find(|c| c.field == name);
        let status_of = |v: &Value| v.as_str().and_then(|s| s.parse::<Status>().ok());
        let stage_of = |v: &Value| v.as_str().map(str::to_owned);
        let status = field("status");
        let stage = field("stage");
        let from = known.clone().or_else(|| {
            // Primera vez que se ve, sin su creación: lo de antes del cambio.
            let before = status.and_then(|c| status_of(&c.before))?;
            Some((before, stage.and_then(|c| stage_of(&c.before))))
        });
        let to_status = status.and_then(|c| status_of(&c.after));
        let to_stage = stage.map(|c| stage_of(&c.after));
        let to = match (&from, to_status, to_stage) {
            (_, Some(s), Some(st)) => (s, st),
            (_, Some(s), None) => (s, from.as_ref().and_then(|f| f.1.clone())),
            (Some(f), None, Some(st)) => (f.0, st),
            // Sin estado conocido (inicio podado y solo cambió la columna):
            // se espera al primer cambio de estado; el relleno cierra el resto.
            _ => continue,
        };
        let created = e.action == Action::Created && known.is_none();
        if created || from.as_ref() != Some(&to) {
            out.push(Step {
                task,
                from: if created { None } else { from },
                to: to.clone(),
                actor: e.actor.clone(),
                at: e.at,
            });
        }
        states.insert(task, to);
    }
    out
}

/// Rendimiento de un agente en un periodo (RF-INT-06).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct AgentStats {
    /// Entregas a revisión.
    pub submitted: usize,
    /// Entregas aprobadas (pasaron a hecho).
    pub approved: usize,
    pub rejected: usize,
    /// Minutos medios desde que tomó la tarea hasta que la entregó.
    pub avg_cycle_minutes: Option<i64>,
}

impl AgentStats {
    pub fn rejection_rate(&self) -> Option<f64> {
        (self.submitted > 0).then(|| self.rejected as f64 / self.submitted as f64)
    }
}

/// Entregas, aprobaciones, rechazos y ciclo medio por agente, a partir de
/// las entradas entre `from` y `to` (las anteriores dan el contexto).
pub fn agent_stats(
    entries: &[ActivityEntry],
    from: Timestamp,
    to: Timestamp,
) -> BTreeMap<AgentId, AgentStats> {
    let mut sorted: Vec<&ActivityEntry> = entries.iter().filter(|e| is_task_change(e)).collect();
    sorted.sort_by_key(|e| (e.at, e.id));
    let mut claimed_at: HashMap<(TaskId, AgentId), Timestamp> = HashMap::new();
    let mut submitter: HashMap<TaskId, AgentId> = HashMap::new();
    let mut cycles: HashMap<AgentId, Vec<i64>> = HashMap::new();
    let mut out: BTreeMap<AgentId, AgentStats> = BTreeMap::new();
    for e in sorted {
        let Some(task) = e.task_id else { continue };
        let in_range = e.at >= from && e.at <= to;
        let changed_to = |field: &str, value: &str| {
            e.changes
                .iter()
                .any(|c| c.field == field && c.after.as_str() == Some(value))
        };
        for c in e.changes.iter().filter(|c| c.field == "claim") {
            if let Some(agent) = c
                .after
                .get("agent")
                .and_then(Value::as_str)
                .and_then(|a| a.parse().ok())
            {
                claimed_at.insert((task, agent), e.at);
            }
        }
        if let Actor::Agent(agent) = e.actor
            && changed_to("status", "in_review")
        {
            submitter.insert(task, agent);
            if in_range {
                let stats = out.entry(agent).or_default();
                stats.submitted += 1;
                if let Some(start) = claimed_at.get(&(task, agent)) {
                    cycles
                        .entry(agent)
                        .or_default()
                        .push(e.at.duration_since(*start).as_secs() / 60);
                }
            }
        }
        if !in_range {
            continue;
        }
        if let Some(agent) = submitter.get(&task).copied() {
            if e.command == "task.reject" {
                out.entry(agent).or_default().rejected += 1;
                submitter.remove(&task);
            } else if changed_to("status", "done") {
                out.entry(agent).or_default().approved += 1;
                submitter.remove(&task);
            }
        }
    }
    for (agent, list) in cycles {
        if let Some(stats) = out.get_mut(&agent)
            && !list.is_empty()
        {
            stats.avg_cycle_minutes = Some(list.iter().sum::<i64>() / list.len() as i64);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{ActivityId, CorrelationId, FieldChange};
    use serde_json::json;

    fn at(min: i64) -> Timestamp {
        Timestamp::UNIX_EPOCH
            .checked_add(jiff::SignedDuration::from_mins(1_000_000 + min))
            .unwrap()
    }

    fn entry(
        min: i64,
        task: TaskId,
        actor: Actor,
        command: &str,
        changes: &[(&str, Value)],
    ) -> ActivityEntry {
        let ts = at(min);
        ActivityEntry {
            id: ActivityId::from_ulid(foundation::Ulid::from_parts(
                u64::try_from(ts.as_millisecond()).unwrap(),
                1,
            )),
            at: ts,
            actor,
            correlation: CorrelationId::generate(),
            command: command.into(),
            action: Action::Updated,
            entity: EntityKind::Task,
            entity_id: task.to_string(),
            project_id: None,
            task_id: Some(task),
            changes: changes
                .iter()
                .map(|(f, v)| FieldChange {
                    field: (*f).into(),
                    before: Value::Null,
                    after: v.clone(),
                })
                .collect(),
        }
    }

    /// D24: sin la creación (historial podado), el punto de partida sale de
    /// lo que había antes del primer cambio; lo que no es estado ni columna
    /// no es un paso.
    #[test]
    fn the_journey_survives_a_pruned_beginning() {
        let task = TaskId::generate();
        let bot = Actor::Agent(AgentId::generate());
        let mut moved = entry(10, task, bot.clone(), "task.claim", &[]);
        moved.changes.push(FieldChange {
            field: "status".into(),
            before: json!("todo"),
            after: json!("in_progress"),
        });
        let mut queued = entry(20, task, bot.clone(), "task.move", &[]);
        queued.changes = vec![
            FieldChange {
                field: "status".into(),
                before: json!("in_progress"),
                after: json!("todo"),
            },
            FieldChange {
                field: "stage".into(),
                before: Value::Null,
                after: json!("cola_de_rev"),
            },
        ];
        let renamed = entry(
            30,
            task,
            bot.clone(),
            "task.update",
            &[("title", json!("x"))],
        );
        let steps = journey(&[renamed, queued, moved]);
        let path: Vec<_> = steps
            .iter()
            .map(|s| (s.from.clone(), s.to.clone()))
            .collect();
        assert_eq!(
            path,
            vec![
                (Some((Status::Todo, None)), (Status::InProgress, None)),
                (
                    Some((Status::InProgress, None)),
                    (Status::Todo, Some("cola_de_rev".into()))
                ),
            ]
        );
        assert!(steps.iter().all(|s| s.actor == bot));
        assert_eq!(steps[1].at, at(20));
    }

    #[test]
    fn time_goes_to_whoever_has_the_task_in_progress() {
        let task = TaskId::generate();
        let bot = AgentId::generate();
        let claim = json!({ "agent": bot.to_string(), "since": "x", "until": "y", "revision": 1 });
        let entries = vec![
            entry(
                0,
                task,
                Actor::Supervisor,
                "task.create",
                &[("status", json!("todo"))],
            ),
            entry(
                10,
                task,
                Actor::Agent(bot),
                "task.claim",
                &[("status", json!("in_progress")), ("claim", claim)],
            ),
            entry(
                40,
                task,
                Actor::Agent(bot),
                "task.submit_review",
                &[("status", json!("in_review")), ("claim", Value::Null)],
            ),
            entry(
                50,
                task,
                Actor::Supervisor,
                "task.reject",
                &[("status", json!("in_progress"))],
            ),
        ];
        let worked = worked(&entries, at(65));
        let by_bot: i64 = worked
            .iter()
            .filter(|i| i.actor == Actor::Agent(bot))
            .map(Interval::minutes)
            .sum();
        assert_eq!(by_bot, 30, "de la toma a la entrega");
        // Rechazada sin claim ni asignado: nadie la tiene, no cuenta.
        assert_eq!(worked.len(), 1);

        let stats = agent_stats(&entries, at(0), at(100));
        let s = &stats[&bot];
        assert_eq!((s.submitted, s.rejected, s.approved), (1, 1, 0));
        assert_eq!(s.avg_cycle_minutes, Some(30));
        assert_eq!(s.rejection_rate(), Some(1.0));
    }
}
