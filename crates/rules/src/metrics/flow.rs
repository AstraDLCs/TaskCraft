//! Métricas de flujo (TC-5) sobre el recorrido guardado de las tareas (D24):
//! tiempos de entrega y de ciclo, relevos entre dueños, devoluciones y WIP.
//! Solo trabajo: nada de costes ni tokens.

use std::borrow::Borrow;
use std::collections::HashMap;

use model::{Actor, Category, Project, ProjectId, Status, TaskId, Timestamp, Transition};

use crate::column_owner;

/// Un tramo del recorrido: la tarea estuvo en una columna desde `entered`
/// hasta `left` (`None` = sigue ahí).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub task: TaskId,
    /// Proyecto de la tarea en ese tramo: da los dueños de las columnas.
    pub project: ProjectId,
    pub status: Status,
    pub stage: Option<String>,
    /// Quién la llevó a este tramo.
    pub actor: Actor,
    pub entered: Timestamp,
    pub left: Option<Timestamp>,
}

impl Segment {
    /// Clave de la columna: su etapa o, sin ella, su estado base.
    pub fn column(&self) -> &str {
        self.stage.as_deref().unwrap_or(self.status.as_str())
    }

    /// Minutos en el tramo; el actual cuenta hasta `now`.
    pub fn minutes(&self, now: Timestamp) -> i64 {
        minutes(self.entered, self.left.unwrap_or(now))
    }

    /// Minutos del tramo dentro de `[lo, hi)`.
    fn overlap(&self, lo: Timestamp, hi: Timestamp, now: Timestamp) -> i64 {
        let start = self.entered.max(lo);
        let end = self.left.unwrap_or(now).min(hi);
        if end > start { minutes(start, end) } else { 0 }
    }
}

fn minutes(from: Timestamp, to: Timestamp) -> i64 {
    to.duration_since(from).as_secs() / 60
}

/// Transiciones → tramos: cada una abre el suyo y cierra el anterior de su
/// tarea. Salen por orden de entrada (el store ya las da así; ordenar una
/// lista ya ordenada es lineal).
pub fn segments(transitions: &[Transition]) -> Vec<Segment> {
    let mut sorted: Vec<&Transition> = transitions.iter().collect();
    sorted.sort_by_key(|t| (t.at, t.id));
    let mut open: HashMap<TaskId, usize> = HashMap::new();
    let mut out: Vec<Segment> = Vec::with_capacity(sorted.len());
    for t in sorted {
        if let Some(prev) = open.get(&t.task_id).and_then(|i| out.get_mut(*i)) {
            prev.left = Some(t.at);
        }
        open.insert(t.task_id, out.len());
        out.push(Segment {
            task: t.task_id,
            project: t.project_id,
            status: t.to_status,
            stage: t.to_stage.clone(),
            actor: t.actor.clone(),
            entered: t.at,
            left: None,
        });
    }
    out
}

/// Índices de los tramos en que la tarea entra en hecho (moverse entre
/// etapas de hecho no es volver a entrar). `path` es el de una sola tarea.
fn done_entries<S: Borrow<Segment>>(path: &[S]) -> impl Iterator<Item = usize> + '_ {
    let mut was_done = false;
    path.iter().enumerate().filter_map(move |(i, s)| {
        let done = s.borrow().status == Status::Done;
        let entered = done && !was_done;
        was_done = done;
        entered.then_some(i)
    })
}

/// Última vez que entró en hecho, si sigue hecha. `segments` es el recorrido
/// de una sola tarea, en orden.
pub fn done_at(segments: &[Segment]) -> Option<Timestamp> {
    if segments.last()?.status != Status::Done {
        return None;
    }
    done_entries(segments)
        .last()
        .and_then(|i| segments.get(i))
        .map(|s| s.entered)
}

/// Desde cuándo está cerrada (hecha o cancelada), si lo está: el inicio de
/// la racha final de tramos cerrados. `segments` es el de una sola tarea.
pub fn closed_at(segments: &[Segment]) -> Option<Timestamp> {
    segments
        .iter()
        .rev()
        .take_while(|s| matches!(s.status.category(), Category::Done | Category::Cancelled))
        .last()
        .map(|s| s.entered)
}

/// Qué tramos son devoluciones: alguien que tenía la tarea entre manos se la
/// manda a otro, a una columna por la que ya pasó en su camino actual.
/// Cuenta si se cumple todo:
/// - quien la mueve no es el sistema (la caducidad del lease no devuelve);
/// - sale de una columna de trabajo (activa o de revisión) o de una cola
///   suya (el revisor que la rechaza sin tomarla); no de una pendiente ajena
///   (backlog ↔ todo) ni de hecha (reabrir no es devolver);
/// - la columna destino ya está en el camino actual;
/// - va a otro: la columna destino es de otro dueño o, sin dueño, es
///   pendiente o de revisión y sale de revisión (rechazo a la bolsa).
///
/// Así no cuentan: pausar en la propia cola (el dueño es quien la mueve),
/// soltarla con `task.release` (de activa a una pendiente sin dueño) ni
/// entrar en una activa sin dueño (quien la mueve la toma: el revisor que
/// empieza a revisar desde su cola). Al volver a una columna, el camino se
/// recorta hasta ella; avanzar otra vez después no es volver.
/// `owners` son los dueños de las columnas de `path`, tramo a tramo.
fn returns(path: &[&Segment], owners: &[Option<Actor>]) -> Vec<bool> {
    let mut columns: Vec<&str> = Vec::new();
    let mut out = Vec::with_capacity(path.len());
    let mut source: Option<(Category, Option<&Actor>)> = None;
    for (s, owner) in path.iter().zip(owners) {
        let column = s.column();
        let category = s.status.category();
        let from = source.replace((category, owner.as_ref()));
        // Sigue en la misma columna: no se ha movido.
        let visited = if columns.last() == Some(&column) {
            false
        } else if let Some(pos) = columns.iter().position(|c| *c == column) {
            columns.truncate(pos + 1);
            true
        } else {
            columns.push(column);
            false
        };
        let mover = &s.actor;
        let back = visited
            && *mover != Actor::System
            && from.is_some_and(|(from, from_owner)| {
                let working = matches!(from, Category::Active | Category::Review)
                    || from_owner == Some(mover);
                let to_other = match owner {
                    Some(o) => o != mover,
                    None => {
                        from == Category::Review
                            && matches!(category, Category::Pending | Category::Review)
                    }
                };
                working && to_other
            });
        out.push(back);
    }
    out
}

/// Una tarea terminada en el periodo.
#[derive(Debug, Clone, PartialEq)]
pub struct Done {
    pub task: TaskId,
    /// Última vez que entró en hecho dentro del periodo (aunque luego se
    /// reabriera: el periodo no cambia después).
    pub at: Timestamp,
    /// Quién la cerró (la movió a hecho); no necesariamente quien la trabajó.
    pub actor: Actor,
    /// De la creación a hecha. La creación es el primer paso del recorrido
    /// (`from = None`); si se reconstruyó de un historial podado (D24), el
    /// primero que se conoce.
    pub lead_minutes: i64,
    /// De la primera vez activa a hecha; `None` si no estuvo activa antes.
    pub cycle_minutes: Option<i64>,
    /// Tuvo alguna devolución antes de cerrarse.
    pub returned: bool,
}

/// Lo de cada actor en el periodo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFlow {
    /// Tareas que cerró.
    pub done: usize,
    /// En columnas activas: de su dueño o, sin dueño, de quien la llevó.
    pub worked_minutes: i64,
    /// Esperando en sus columnas (pendiente o revisión).
    pub queued_minutes: i64,
    /// Devoluciones a una columna suya.
    pub returns_received: usize,
    /// Devoluciones que hizo él.
    pub returns_sent: usize,
}

/// Lo de cada columna (por clave, sumando proyectos).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnFlow {
    /// Tareas que están en ella ahora.
    pub tasks_now: usize,
    /// Estancias acabadas en el periodo y sus minutos.
    pub stays: usize,
    pub minutes: i64,
}

impl ColumnFlow {
    pub fn avg_minutes(&self) -> Option<i64> {
        i64::try_from(self.stays)
            .ok()
            .filter(|n| *n > 0)
            .map(|n| self.minutes / n)
    }
}

/// Métricas de flujo de un periodo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Flow {
    pub done: Vec<Done>,
    /// Cambios de manos: entradas en una columna con dueño distinto del que
    /// la tenía (ver `flow`).
    pub handoffs: usize,
    pub returns: usize,
    /// Tareas activas de media en el periodo (ponderado por tiempo).
    pub wip_avg: f64,
    pub agents: HashMap<Actor, AgentFlow>,
    pub columns: HashMap<String, ColumnFlow>,
}

impl Flow {
    pub fn lead(&self, pct: u32) -> Option<i64> {
        percentile(
            &self.done.iter().map(|d| d.lead_minutes).collect::<Vec<_>>(),
            pct,
        )
    }

    pub fn cycle(&self, pct: u32) -> Option<i64> {
        percentile(
            &self
                .done
                .iter()
                .filter_map(|d| d.cycle_minutes)
                .collect::<Vec<_>>(),
            pct,
        )
    }

    /// % de las hechas con al menos una devolución (0 sin hechas).
    pub fn returned_pct(&self) -> f64 {
        if self.done.is_empty() {
            return 0.0;
        }
        let returned = self.done.iter().filter(|d| d.returned).count();
        returned as f64 * 100.0 / self.done.len() as f64
    }
}

/// Percentil por rango más cercano: el valor que deja `pct` % a su altura o
/// por debajo. `None` sin valores.
pub fn percentile(values: &[i64], pct: u32) -> Option<i64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let rank = (n * pct.min(100) as usize).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// Métricas de flujo entre `from` y `to` (`[from, to)`) a partir del
/// recorrido completo de las tareas (el anterior da el contexto: creación,
/// columnas ya visitadas). `projects` da los dueños de las columnas.
///
/// Un relevo es un cambio de manos: la tarea entra en una columna con dueño
/// y ese dueño no es quien la tenía en el tramo anterior (el dueño de esa
/// columna o, sin dueño, quien la llevó allí; al crearse, quien la creó).
/// Tomarla en su propia columna activa o pasar de su cola a su columna de
/// trabajo no es relevo.
pub fn flow(
    transitions: &[Transition],
    projects: &[Project],
    from: Timestamp,
    to: Timestamp,
    now: Timestamp,
) -> Flow {
    let segs = segments(transitions);
    let mut by_task: HashMap<TaskId, Vec<&Segment>> = HashMap::new();
    for s in &segs {
        by_task.entry(s.task).or_default().push(s);
    }
    let in_range = |at: Timestamp| at >= from && at < to;
    let mut out = Flow::default();
    let mut active_minutes = 0i64;
    for path in by_task.values() {
        let Some(first) = path.first() else { continue };
        let owners: Vec<Option<Actor>> = path
            .iter()
            .map(|s| {
                projects
                    .iter()
                    .find(|p| p.id == s.project)
                    .and_then(|p| column_owner(p, s.status, s.stage.as_deref()))
                    .map(Actor::Agent)
            })
            .collect();
        let back = returns(path, &owners);
        let mut holder = &first.actor;
        for ((s, owner), back) in path.iter().zip(&owners).zip(&back) {
            if in_range(s.entered) {
                if owner.as_ref().is_some_and(|o| o != holder) {
                    out.handoffs += 1;
                }
                if *back {
                    out.returns += 1;
                    out.agents.entry(s.actor.clone()).or_default().returns_sent += 1;
                    if let Some(o) = owner {
                        out.agents.entry(o.clone()).or_default().returns_received += 1;
                    }
                }
            }
            holder = owner.as_ref().unwrap_or(&s.actor);
            let inside = s.overlap(from, to, now);
            match s.status.category() {
                Category::Active => {
                    active_minutes += inside;
                    let who = owner.clone().unwrap_or_else(|| s.actor.clone());
                    out.agents.entry(who).or_default().worked_minutes += inside;
                }
                Category::Pending | Category::Review => {
                    if let Some(o) = owner {
                        out.agents.entry(o.clone()).or_default().queued_minutes += inside;
                    }
                }
                Category::Done | Category::Cancelled => {}
            }
            let column = out.columns.entry(s.column().to_owned()).or_default();
            match s.left {
                None => column.tasks_now += 1,
                Some(left) if in_range(left) => {
                    column.stays += 1;
                    column.minutes += s.minutes(now);
                }
                Some(_) => {}
            }
        }
        // Hecha en el periodo: su última entrada en hecho dentro de él.
        let Some((i, done)) = done_entries(path)
            .filter_map(|i| path.get(i).map(|s| (i, *s)))
            .filter(|(_, s)| in_range(s.entered))
            .last()
        else {
            continue;
        };
        let before = path.get(..i).unwrap_or_default();
        let cycle = before
            .iter()
            .find(|s| s.status.category() == Category::Active)
            .map(|s| minutes(s.entered, done.entered));
        out.agents.entry(done.actor.clone()).or_default().done += 1;
        out.done.push(Done {
            task: done.task,
            at: done.entered,
            actor: done.actor.clone(),
            lead_minutes: minutes(first.entered, done.entered),
            cycle_minutes: cycle,
            returned: back.get(..i).is_some_and(|b| b.contains(&true)),
        });
    }
    out.done.sort_by_key(|d| (d.at, d.task));
    let span = minutes(from, to.min(now));
    if span > 0 {
        out.wip_avg = active_minutes as f64 / span as f64;
    }
    out
}

/// Cuántas tareas hay en cada estado al final de cada instante de `cuts`
/// según su recorrido (para burndown y flujo acumulado). Sin entrada antes
/// del corte = no existía todavía. Los conteos se llevan al día paso a paso:
/// cada corte copia solo un estado por clave, no todas las tareas.
pub fn status_counts(
    transitions: &[Transition],
    cuts: &[Timestamp],
) -> Vec<HashMap<Status, usize>> {
    let mut sorted: Vec<&Transition> = transitions.iter().collect();
    sorted.sort_by_key(|t| (t.at, t.id));
    let mut state: HashMap<TaskId, Status> = HashMap::new();
    let mut counts: HashMap<Status, usize> = HashMap::new();
    let mut out = Vec::with_capacity(cuts.len());
    let mut next = 0;
    for &cut in cuts {
        while let Some(t) = sorted.get(next).filter(|t| t.at <= cut) {
            if let Some(old) = state.insert(t.task_id, t.to_status)
                && let Some(n) = counts.get_mut(&old)
            {
                *n = n.saturating_sub(1);
            }
            *counts.entry(t.to_status).or_default() += 1;
            next += 1;
        }
        out.push(counts.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{AgentId, ProjectStatus, StatusDef, TransitionId};

    fn at(min: i64) -> Timestamp {
        Timestamp::UNIX_EPOCH
            .checked_add(jiff::SignedDuration::from_mins(1_000_000 + min))
            .unwrap()
    }

    fn project(columns: &[(&str, Status, AgentId)]) -> Project {
        Project {
            id: ProjectId::generate(),
            key: "API".into(),
            name: "API".into(),
            description: String::new(),
            status: ProjectStatus::Active,
            next_number: 1,
            next_diagram: 1,
            created_at: at(0),
            updated_at: at(0),
            locked: false,
            workflow: columns
                .iter()
                .map(|(key, base, owner)| {
                    let mut d = StatusDef::new(*key, *key, *base);
                    d.owner = Some(*owner);
                    d
                })
                .collect(),
            estimate_unit: model::EstimateUnit::Hours,
            fields: Vec::new(),
        }
    }

    /// Recorrido de una tarea: (minuto, estado, etapa, actor).
    fn walk(
        project: &Project,
        task: TaskId,
        steps: &[(i64, Status, Option<&str>, &Actor)],
    ) -> Vec<Transition> {
        let mut prev: Option<(Status, Option<String>)> = None;
        steps
            .iter()
            .map(|(min, status, stage, actor)| {
                let stage = stage.map(str::to_owned);
                let t = Transition {
                    id: TransitionId::generate(),
                    task_id: task,
                    project_id: project.id,
                    from_status: prev.as_ref().map(|p| p.0),
                    from_stage: prev.as_ref().and_then(|p| p.1.clone()),
                    to_status: *status,
                    to_stage: stage.clone(),
                    actor: (*actor).clone(),
                    at: at(*min),
                };
                prev = Some((*status, stage));
                t
            })
            .collect()
    }

    #[test]
    fn percentiles_by_nearest_rank() {
        assert_eq!(percentile(&[], 50), None);
        assert_eq!(percentile(&[7], 85), Some(7));
        assert_eq!(percentile(&[40, 10, 30, 20], 50), Some(20));
        assert_eq!(percentile(&[40, 10, 30, 20], 85), Some(40));
    }

    /// dev → revisor → dev → revisor → hecho: 4 relevos y 1 devolución; al
    /// volver a avanzar tras la devolución no se cuenta otra.
    #[test]
    fn hand_overs_and_one_return() {
        let (dev, rev) = (AgentId::generate(), AgentId::generate());
        let p = project(&[
            ("cola_dev", Status::Todo, dev),
            ("cola_rev", Status::Todo, rev),
        ]);
        let (a_dev, a_rev) = (Actor::Agent(dev), Actor::Agent(rev));
        let task = TaskId::generate();
        let sup = Actor::Supervisor;
        let ip = Status::InProgress;
        let todo = Status::Todo;
        let steps = walk(
            &p,
            task,
            &[
                (0, todo, Some("cola_dev"), &sup),
                (10, ip, None, &a_dev),
                (40, todo, Some("cola_rev"), &a_dev),
                (55, todo, Some("cola_dev"), &a_rev),
                (60, ip, None, &a_dev),
                (80, todo, Some("cola_rev"), &a_dev),
                // El revisor la toma: pasa otra vez por en curso sin volver.
                (85, ip, None, &a_rev),
                (90, Status::Done, None, &a_rev),
            ],
        );
        // Otra tarea sin terminar, todavía en curso.
        let other = TaskId::generate();
        let mut all = walk(&p, other, &[(0, todo, None, &sup), (30, ip, None, &a_dev)]);
        all.extend(steps.iter().cloned());

        let f = flow(&all, std::slice::from_ref(&p), at(0), at(120), at(120));
        assert_eq!((f.handoffs, f.returns), (4, 1));
        assert_eq!(f.done.len(), 1);
        let d = &f.done[0];
        assert_eq!((d.lead_minutes, d.cycle_minutes), (90, Some(80)));
        assert!(d.returned);
        assert_eq!(d.actor, a_rev);
        assert_eq!(f.returned_pct(), 100.0);
        assert_eq!((f.lead(50), f.cycle(85)), (Some(90), Some(80)));

        let dev_flow = &f.agents[&a_dev];
        // En curso: 10..40 y 60..80, más la otra tarea 30..120.
        assert_eq!(dev_flow.worked_minutes, 30 + 20 + 90);
        // En su cola: 0..10 y 55..60.
        assert_eq!(dev_flow.queued_minutes, 15);
        assert_eq!((dev_flow.returns_received, dev_flow.returns_sent), (1, 0));
        let rev_flow = &f.agents[&a_rev];
        assert_eq!((rev_flow.done, rev_flow.queued_minutes), (1, 15 + 5));
        assert_eq!(rev_flow.worked_minutes, 5);
        assert_eq!((rev_flow.returns_received, rev_flow.returns_sent), (0, 1));

        let rev_col = &f.columns["cola_rev"];
        assert_eq!((rev_col.stays, rev_col.avg_minutes()), (2, Some(10)));
        assert_eq!(f.columns["done"].tasks_now, 1);
        assert_eq!(f.columns["in_progress"].tasks_now, 1);
        // 50 + 5 + 90 minutos activos en 120.
        assert!((f.wip_avg - 145.0 / 120.0).abs() < 1e-9);

        // Fuera del periodo no cuenta nada de lo que pasó dentro.
        let later = flow(&all, std::slice::from_ref(&p), at(200), at(300), at(300));
        assert_eq!((later.handoffs, later.returns, later.done.len()), (0, 0, 0));

        let segs = segments(&steps);
        assert_eq!(segs.len(), 8);
        assert_eq!(segs[1].minutes(at(500)), 30);
        assert_eq!(segs[7].left, None);
        assert_eq!(done_at(&segs), Some(at(90)));
        let counts = status_counts(&all, &[at(5), at(50), at(95)]);
        assert_eq!(counts[0][&Status::Todo], 2);
        assert_eq!(counts[1][&Status::InProgress], 1);
        assert_eq!(counts[1][&Status::Todo], 1);
        assert_eq!(counts[2][&Status::Done], 1);
        assert_eq!(counts[2].get(&Status::Todo).copied().unwrap_or(0), 0);
    }

    /// Con una columna de trabajo propia («revisando»): tomarla en ella o
    /// pasar de la cola a ella es del mismo dueño, no un relevo.
    #[test]
    fn taking_it_into_ones_own_column_is_not_a_handoff() {
        let (dev, rev) = (AgentId::generate(), AgentId::generate());
        let p = project(&[
            ("cola_dev", Status::Todo, dev),
            ("trabajando", Status::InProgress, dev),
            ("cola_rev", Status::Todo, rev),
            ("revisando", Status::InProgress, rev),
        ]);
        let (a_dev, a_rev) = (Actor::Agent(dev), Actor::Agent(rev));
        let (todo, ip) = (Status::Todo, Status::InProgress);
        let steps = walk(
            &p,
            TaskId::generate(),
            &[
                (0, todo, Some("cola_dev"), &Actor::Supervisor),
                (10, ip, Some("trabajando"), &a_dev),
                (20, todo, Some("cola_rev"), &a_dev),
                (30, ip, Some("revisando"), &a_rev),
                (40, todo, Some("cola_dev"), &a_rev),
                (50, ip, Some("trabajando"), &a_dev),
                (60, todo, Some("cola_rev"), &a_dev),
                (70, ip, Some("revisando"), &a_rev),
                (80, Status::Done, None, &a_rev),
            ],
        );
        let f = flow(&steps, std::slice::from_ref(&p), at(0), at(90), at(90));
        assert_eq!((f.handoffs, f.returns), (4, 1));
        // Crearla en la propia cola tampoco es un relevo.
        let own = walk(
            &p,
            TaskId::generate(),
            &[(0, todo, Some("cola_dev"), &a_dev)],
        );
        let f = flow(&own, std::slice::from_ref(&p), at(0), at(90), at(90));
        assert_eq!(f.handoffs, 0);
    }

    /// Ir y venir del backlog, pausar en la propia cola, soltarla
    /// (`task.release`) y la caducidad del lease no son devoluciones.
    #[test]
    fn moves_that_are_not_returns() {
        let dev = AgentId::generate();
        let p = project(&[("cola_dev", Status::Todo, dev)]);
        let a_dev = Actor::Agent(dev);
        let sup = Actor::Supervisor;
        let (todo, ip) = (Status::Todo, Status::InProgress);
        let count = |steps: &[(i64, Status, Option<&str>, &Actor)]| {
            let t = walk(&p, TaskId::generate(), steps);
            flow(&t, std::slice::from_ref(&p), at(0), at(100), at(100)).returns
        };
        let backlog = [
            (0, todo, None, &sup),
            (10, Status::Backlog, None, &sup),
            (20, todo, None, &sup),
        ];
        assert_eq!(count(&backlog), 0, "backlog ↔ todo");
        let pause = [
            (0, todo, Some("cola_dev"), &sup),
            (10, ip, None, &a_dev),
            (20, todo, Some("cola_dev"), &a_dev),
            (30, ip, None, &a_dev),
        ];
        assert_eq!(count(&pause), 0, "pausa en la propia cola");
        let release = [
            (0, todo, None, &sup),
            (10, ip, None, &a_dev),
            (20, todo, None, &a_dev),
        ];
        assert_eq!(count(&release), 0, "task.release");
        let expired = [
            (0, todo, None, &sup),
            (10, ip, None, &a_dev),
            (20, todo, None, &Actor::System),
        ];
        assert_eq!(count(&expired), 0, "lease caducado");
        let expired_to_queue = [
            (0, todo, Some("cola_dev"), &sup),
            (10, ip, None, &a_dev),
            (20, todo, Some("cola_dev"), &Actor::System),
        ];
        assert_eq!(count(&expired_to_queue), 0, "lease caducado a su cola");
        let reopened = [
            (0, todo, Some("cola_dev"), &sup),
            (10, ip, None, &a_dev),
            (20, Status::Done, None, &a_dev),
            (30, todo, Some("cola_dev"), &sup),
            (40, ip, None, &a_dev),
        ];
        assert_eq!(count(&reopened), 0, "reabrir");
    }

    /// Cola de revisor con base «en revisión» y el revisor trabajando en el
    /// «en curso» genérico: tomarla no es devolverla; mandarla a la cola de
    /// dev, sí.
    #[test]
    fn reviewer_queue_in_review_with_generic_in_progress() {
        let (dev, rev) = (AgentId::generate(), AgentId::generate());
        let p = project(&[
            ("cola_dev", Status::Todo, dev),
            ("cola_rev", Status::InReview, rev),
        ]);
        let (a_dev, a_rev) = (Actor::Agent(dev), Actor::Agent(rev));
        let (todo, ip, rv) = (Status::Todo, Status::InProgress, Status::InReview);
        let steps = walk(
            &p,
            TaskId::generate(),
            &[
                (0, todo, Some("cola_dev"), &Actor::Supervisor),
                (10, ip, None, &a_dev),
                (20, rv, Some("cola_rev"), &a_dev),
                (30, ip, None, &a_rev),
                (40, todo, Some("cola_dev"), &a_rev),
                (50, ip, None, &a_dev),
                (60, rv, Some("cola_rev"), &a_dev),
                (70, ip, None, &a_rev),
                (80, Status::Done, None, &a_rev),
            ],
        );
        let f = flow(&steps, std::slice::from_ref(&p), at(0), at(90), at(90));
        assert_eq!((f.handoffs, f.returns), (4, 1));
        assert_eq!(f.agents[&a_rev].returns_sent, 1);
        assert_eq!(f.agents[&a_dev].returns_received, 1);
    }

    /// Reabierta después: el periodo en que se cerró la sigue contando; el
    /// periodo de la reapertura no, hasta que se vuelva a cerrar en él.
    #[test]
    fn done_does_not_change_after_the_period() {
        let p = project(&[]);
        let bot = Actor::Agent(AgentId::generate());
        let sup = Actor::Supervisor;
        let steps = walk(
            &p,
            TaskId::generate(),
            &[
                (0, Status::Todo, None, &sup),
                (10, Status::InProgress, None, &bot),
                (20, Status::Done, None, &bot),
                (200, Status::Todo, None, &sup),
                (210, Status::InProgress, None, &bot),
            ],
        );
        let before = flow(&steps, std::slice::from_ref(&p), at(0), at(100), at(300));
        assert_eq!(before.done.len(), 1);
        assert_eq!(before.done[0].at, at(20));
        assert_eq!(before.done[0].lead_minutes, 20);
        assert_eq!(before.agents[&bot].done, 1);
        let after = flow(&steps, std::slice::from_ref(&p), at(100), at(300), at(300));
        assert!(after.done.is_empty());

        // Cerrada otra vez dentro del mismo periodo: cuenta la última entrada.
        let mut again = steps.clone();
        again.extend(walk(
            &p,
            steps[0].task_id,
            &[(250, Status::Done, None, &bot)],
        ));
        let all = flow(&again, std::slice::from_ref(&p), at(0), at(300), at(300));
        assert_eq!(all.done.len(), 1);
        assert_eq!(
            (
                all.done[0].at,
                all.done[0].lead_minutes,
                all.done[0].cycle_minutes
            ),
            (at(250), 250, Some(240))
        );
        let segs = segments(&again);
        assert_eq!(
            (done_at(&segs), closed_at(&segs)),
            (Some(at(250)), Some(at(250)))
        );
    }

    /// Creada antes del periodo: el lead cuenta desde la creación; los
    /// relevos y minutos, solo lo de dentro.
    #[test]
    fn tasks_created_before_the_period() {
        let dev = AgentId::generate();
        let p = project(&[("cola_dev", Status::Todo, dev)]);
        let a_dev = Actor::Agent(dev);
        let steps = walk(
            &p,
            TaskId::generate(),
            &[
                (0, Status::Todo, Some("cola_dev"), &Actor::Supervisor),
                (100, Status::InProgress, None, &a_dev),
                (130, Status::Done, None, &a_dev),
            ],
        );
        let f = flow(&steps, std::slice::from_ref(&p), at(50), at(150), at(150));
        assert_eq!(f.handoffs, 0, "la entrada en la cola fue antes");
        assert_eq!(
            (f.done[0].lead_minutes, f.done[0].cycle_minutes),
            (130, Some(30))
        );
        assert_eq!(f.agents[&a_dev].queued_minutes, 50);
        assert_eq!(f.agents[&a_dev].worked_minutes, 30);
    }

    /// Sin datos: nada hecho, percentiles vacíos, WIP 0.
    #[test]
    fn empty_flow() {
        let f = flow(&[], &[], at(0), at(100), at(100));
        assert!(f.done.is_empty());
        assert_eq!((f.handoffs, f.returns, f.returned_pct()), (0, 0, 0.0));
        assert_eq!((f.lead(50), f.cycle(85)), (None, None));
        assert_eq!(f.wip_avg, 0.0);
        assert!(status_counts(&[], &[at(10)])[0].is_empty());
    }

    /// Cancelada: cerrada desde la cancelación, sin fecha de hecha.
    #[test]
    fn cancelled_is_closed_but_not_done() {
        let p = project(&[]);
        let sup = Actor::Supervisor;
        let steps = walk(
            &p,
            TaskId::generate(),
            &[
                (0, Status::Todo, None, &sup),
                (10, Status::Cancelled, None, &sup),
            ],
        );
        let segs = segments(&steps);
        assert_eq!((done_at(&segs), closed_at(&segs)), (None, Some(at(10))));
        assert_eq!(closed_at(&segs[..1]), None);
    }

    /// Sin columnas con dueño: rechazar desde revisión (`task.reject`, a
    /// pendiente) es una devolución; volver a revisión después, no.
    #[test]
    fn rejecting_from_review_is_a_return() {
        let p = project(&[]);
        let task = TaskId::generate();
        let (sup, bot) = (Actor::Supervisor, Actor::Agent(AgentId::generate()));
        let steps = walk(
            &p,
            task,
            &[
                (0, Status::Todo, None, &sup),
                (10, Status::InProgress, None, &bot),
                (20, Status::InReview, None, &bot),
                (30, Status::Todo, None, &sup),
                (35, Status::InProgress, None, &bot),
                (40, Status::InReview, None, &bot),
                (50, Status::Done, None, &sup),
            ],
        );
        let f = flow(&steps, std::slice::from_ref(&p), at(0), at(60), at(60));
        assert_eq!((f.handoffs, f.returns), (0, 1));
        assert_eq!(f.agents[&sup].returns_sent, 1);
        assert_eq!(f.done[0].cycle_minutes, Some(40));
    }
}
