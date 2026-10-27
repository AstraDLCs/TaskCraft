//! Réplica de lectura que dibujan las vistas. Se reconstruye entera cuando
//! cambia el almacenamiento (`change_token`); las vistas nunca leen del engine.

use std::collections::{BTreeMap, HashMap, HashSet};

use eframe::egui::Color32;
use engine::lookup::Keys;
use engine::{EngineError, views};
use model::{
    Action, Actor, Agent, AgentStatus, EntityKind, InboxItem, InboxStatus, Label, LabelId, Project,
    ProjectId, Status, Task, TaskId, Timestamp, WorkspaceInfo,
};
use store::{ActivityFilter, Reader, TaskFilter};

use crate::conn::Conn;
use crate::theme;

/// Qué proyectos se muestran en lista, tablero y calendario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    Project(ProjectId),
}

/// Quién: nombre visible, semilla del avatar y color.
#[derive(Debug, Clone)]
pub struct Who {
    pub name: String,
    pub seed: String,
    pub color: Color32,
    /// Lo que hace el agente, tal como se declaró al crearlo (D20).
    pub function: String,
}

impl Who {
    /// Nombre y, si la tiene, su función: el texto de su tooltip.
    pub fn hover(&self) -> String {
        if self.function.trim().is_empty() {
            self.name.clone()
        } else {
            format!("{}\n{}", self.name, self.function.trim())
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClaimInfo {
    pub agent: Who,
    pub since: Timestamp,
    pub until: Timestamp,
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub task: Task,
    pub key: String,
    pub project_key: String,
    pub parent_key: Option<String>,
    pub depth: usize,
    pub children_total: usize,
    pub children_done: usize,
    pub ready: bool,
    /// Pendiente sin cambios desde hace semanas (RF-VIS-16).
    pub stale: bool,
    pub labels: Vec<(String, Color32)>,
    /// El primero de `assignees`: carriles y avisos.
    pub assignee: Option<Who>,
    /// Todos los asignados, en orden (D20).
    pub assignees: Vec<Who>,
    pub claim: Option<ClaimInfo>,
    pub comments: usize,
}

impl TaskRow {
    pub fn id(&self) -> TaskId {
        self.task.id
    }

    pub fn is_closed(&self) -> bool {
        self.task.status.category().is_closed()
    }

    pub fn blocked(&self) -> bool {
        !self.ready && !self.is_closed()
    }
}

#[derive(Debug, Clone)]
pub struct AgentRow {
    pub agent: Agent,
    pub who: Who,
    pub connected: bool,
    pub last_seen: Option<Timestamp>,
    /// Tareas que tiene tomadas ahora: clave, título y desde cuándo.
    pub working_on: Vec<(TaskId, String, String, Timestamp)>,
}

#[derive(Debug, Clone)]
pub struct InboxRow {
    pub item: InboxItem,
    pub author: Who,
}

#[derive(Debug, Clone)]
pub struct ActivityLine {
    pub id: model::ActivityId,
    pub at: Timestamp,
    pub actor: Who,
    pub command: String,
    pub action: Action,
    pub entity: EntityKind,
    pub target: String,
    pub task: Option<TaskId>,
    pub fields: Vec<String>,
}

#[derive(Default)]
pub struct Snapshot {
    pub workspace: Option<WorkspaceInfo>,
    pub projects: Vec<Project>,
    pub labels: Vec<Label>,
    /// Clientes LLM con su token (D15).
    pub interfaces: Vec<model::Interface>,
    pub agents: Vec<AgentRow>,
    /// Tareas del alcance, en orden de árbol (cada una seguida de sus subtareas).
    pub rows: Vec<TaskRow>,
    /// Revisión y escalados de todo el workspace (RF-SUP-06).
    pub review: Vec<TaskRow>,
    pub inbox: Vec<InboxRow>,
    pub trash: Vec<TaskRow>,
    pub activity: Vec<ActivityLine>,
    /// Texto buscado y tareas que lo contienen según el índice FTS5 (RF-VIS-11).
    pub search: (String, HashSet<TaskId>),
    /// Épicas e hitos de todos los proyectos, con su progreso (RF-MOD-13).
    pub epics: Vec<EpicLine>,
    /// Ciclos de todos los proyectos, por inicio (RF-INT-05).
    pub cycles: Vec<model::Cycle>,
    /// Diagramas Mermaid de todos los proyectos (RF-MOD-19), por proyecto y número.
    pub diagrams: Vec<DiagramLine>,
    /// (hechas, total) de cada ciclo abierto, sin las canceladas.
    pub cycle_progress: HashMap<model::CycleId, (usize, usize)>,
    /// Tareas abiertas (ni hechas ni canceladas) de cada proyecto.
    pub open_counts: HashMap<ProjectId, usize>,
    pub templates: Vec<model::Template>,
    /// Vistas guardadas (RF-VIS-12).
    pub views: Vec<model::SavedView>,
    /// Nombres para evaluar el lenguaje de filtros (D12) igual que el engine.
    pub names: rules::filter::Names,
    /// Marcadas «para hoy» hoy mismo (RF-VIS-08).
    pub today: HashSet<TaskId>,
    /// Con bloqueo humano propio o heredado (RF-SUP-05).
    pub locked: HashSet<TaskId>,
    /// Bloqueadora → bloqueada, para las flechas del Gantt (RF-VIS-10).
    pub dependencies: Vec<model::Dependency>,
}

/// Un diagrama con su clave (`API-D3`) y las tareas que lo citan.
#[derive(Debug, Clone)]
pub struct DiagramLine {
    pub diagram: model::Diagram,
    pub key: String,
    pub cited_by: Vec<TaskRef>,
}

/// Una épica o un hito con el progreso de sus tareas (sin las canceladas).
#[derive(Debug, Clone)]
pub struct EpicLine {
    pub epic: model::Epic,
    pub project_key: String,
    pub done: usize,
    pub total: usize,
}

impl EpicLine {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.done as f32 / self.total as f32
        }
    }
}

impl Snapshot {
    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn epic(&self, id: model::EpicId) -> Option<&EpicLine> {
        self.epics.iter().find(|e| e.epic.id == id)
    }

    /// Épicas (o hitos) abiertos de un proyecto, para los selectores.
    pub fn groups_of(&self, project: ProjectId, kind: model::EpicKind) -> Vec<&EpicLine> {
        self.epics
            .iter()
            .filter(|e| e.epic.project_id == project && e.epic.kind == kind)
            .collect()
    }

    /// (hechas, total) del ciclo; (0, 0) si está cerrado o vacío.
    pub fn cycle_progress(&self, id: model::CycleId) -> (usize, usize) {
        self.cycle_progress.get(&id).copied().unwrap_or_default()
    }

    /// Ciclos sin cerrar de un proyecto, por fecha de inicio.
    pub fn open_cycles(&self, project: ProjectId) -> impl Iterator<Item = &model::Cycle> {
        self.cycles
            .iter()
            .filter(move |c| c.project_id == project && c.closed_at.is_none())
    }

    pub fn diagram(&self, id: model::DiagramId) -> Option<&DiagramLine> {
        self.diagrams.iter().find(|d| d.diagram.id == id)
    }

    /// Por clave, sin distinguir mayúsculas (`api-d3`).
    pub fn diagram_by_key(&self, key: &str) -> Option<&DiagramLine> {
        let key = model::DiagramKey::parse(key.trim())?.to_string();
        self.diagrams.iter().find(|d| d.key == key)
    }

    pub fn diagrams_of(&self, project: ProjectId) -> impl Iterator<Item = &DiagramLine> {
        self.diagrams
            .iter()
            .filter(move |d| d.diagram.project_id == project)
    }

    /// Diagramas que cita una tarea en su descripción o comentarios.
    pub fn diagrams_cited_by(&self, task: TaskId) -> impl Iterator<Item = &DiagramLine> {
        self.diagrams
            .iter()
            .filter(move |d| d.cited_by.iter().any(|t| t.id == task))
    }

    pub fn agents_connected(&self) -> usize {
        self.agents.iter().filter(|a| a.connected).count()
    }

    pub fn open_inbox(&self) -> usize {
        self.inbox.len()
    }
}

fn task_time(
    r: &dyn Reader,
    keys: &Keys,
    task: TaskId,
    now: Timestamp,
) -> Result<TaskTime, EngineError> {
    let entries = r.activity(&ActivityFilter {
        task: Some(task),
        limit: 100_000,
        ..ActivityFilter::default()
    })?;
    let mut auto: Vec<(String, i64)> = Vec::new();
    for i in rules::metrics::worked(&entries, now) {
        let name = views::actor_name(r, &i.actor)?;
        match auto.iter_mut().find(|(n, _)| *n == name) {
            Some((_, m)) => *m += i.minutes(),
            None => auto.push((name, i.minutes())),
        }
    }
    let manual = r
        .time_entries(&store::TimeFilter {
            task: Some(task),
            ..store::TimeFilter::default()
        })?
        .iter()
        .map(|e| {
            if e.running {
                now.duration_since(e.started_at).as_secs() / 60
            } else {
                i64::from(e.minutes)
            }
        })
        .sum();
    let running = r
        .time_entries(&store::TimeFilter {
            actor: Some(Actor::Supervisor),
            running: true,
            ..store::TimeFilter::default()
        })?
        .into_iter()
        .next();
    let running_here = running.as_ref().is_some_and(|e| e.task_id == task);
    let running_elsewhere = match running {
        Some(e) if e.task_id != task => Some(keys.of_id(r, e.task_id)?),
        _ => None,
    };
    Ok(TaskTime {
        auto,
        manual,
        running_here,
        running_elsewhere,
    })
}

fn epic_lines(r: &dyn Reader, projects: &[Project]) -> Result<Vec<EpicLine>, EngineError> {
    let mut out = Vec::new();
    for epic in r.epics(None)? {
        let tasks = r.tasks(&TaskFilter {
            include_archived: true,
            group: Some(epic.id),
            ..TaskFilter::default()
        })?;
        let counted: Vec<&Task> = tasks
            .iter()
            .filter(|t| t.status != Status::Cancelled)
            .collect();
        out.push(EpicLine {
            project_key: projects
                .iter()
                .find(|p| p.id == epic.project_id)
                .map(|p| p.key.clone())
                .unwrap_or_default(),
            done: counted.iter().filter(|t| t.status == Status::Done).count(),
            total: counted.len(),
            epic,
        });
    }
    Ok(out)
}

/// Diagramas con su clave y las tareas vivas que los citan (RF-MOD-19).
fn diagram_lines(
    r: &dyn Reader,
    keys: &Keys,
    projects: &[Project],
) -> Result<Vec<DiagramLine>, EngineError> {
    let mut out = Vec::new();
    for diagram in r.diagrams(None)? {
        let project = projects
            .iter()
            .find(|p| p.id == diagram.project_id)
            .map(|p| p.key.clone())
            .unwrap_or_default();
        let mut cited_by: Vec<TaskRef> = Vec::new();
        for m in r.diagram_mentions_of(diagram.id)? {
            if cited_by.iter().any(|t| t.id == m.source) {
                continue;
            }
            if let Some(t) = r.task(m.source)?
                && t.deleted_at.is_none()
            {
                cited_by.push(TaskRef {
                    id: t.id,
                    key: keys.of(&t),
                    title: t.title.clone(),
                    status: t.status,
                });
            }
        }
        out.push(DiagramLine {
            key: model::DiagramKey {
                project,
                number: diagram.number,
            }
            .to_string(),
            diagram,
            cited_by,
        });
    }
    Ok(out)
}

/// Progreso de los ciclos abiertos: una consulta por ciclo, como las épicas.
fn cycle_progress(
    r: &dyn Reader,
    cycles: &[model::Cycle],
) -> Result<HashMap<model::CycleId, (usize, usize)>, EngineError> {
    let mut out = HashMap::new();
    for c in cycles.iter().filter(|c| c.closed_at.is_none()) {
        let tasks = r.tasks(&TaskFilter {
            include_archived: true,
            cycle: Some(c.id),
            ..TaskFilter::default()
        })?;
        let counted = tasks.iter().filter(|t| t.status != Status::Cancelled);
        let total = counted.clone().count();
        let done = counted.filter(|t| t.status == Status::Done).count();
        out.insert(c.id, (done, total));
    }
    Ok(out)
}

/// Tareas cuyo título, descripción o comentarios contienen `text`.
pub fn search(conn: &mut Conn, text: &str) -> HashSet<TaskId> {
    if text.trim().is_empty() {
        return HashSet::new();
    }
    conn.read(|r| Ok(r.search_tasks(text, None, 1000)?))
        .map(|ids| ids.into_iter().collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct Load {
    pub scope: Scope,
    pub include_archived: bool,
    pub activity_limit: usize,
    pub activity_filter: crate::state::ActivityFilter,
}

pub fn load(conn: &mut Conn, opts: &Load) -> Result<Snapshot, EngineError> {
    conn.read(|r| build(r, opts))
}

/// Construye la réplica que dibujan las vistas a partir de una lectura.
pub(crate) fn build(r: &dyn Reader, opts: &Load) -> Result<Snapshot, EngineError> {
    let now = Timestamp::now();
    {
        let keys = Keys::load(r)?;
        let projects = r.projects(true)?;
        let labels = r.labels()?;
        let label_map: HashMap<LabelId, (String, Color32)> = labels
            .iter()
            .map(|l| (l.id, (l.name.clone(), theme::parse_hex(&l.color))))
            .collect();
        let agents_raw = r.agents()?;
        let bulk = Bulk::load(r)?;
        let ctx = RowCtx {
            r,
            keys: &keys,
            labels: &label_map,
            agents: &agents_raw,
            projects: &projects,
            now,
            bulk: &bulk,
        };

        let filter = TaskFilter {
            project: match opts.scope {
                Scope::All => None,
                Scope::Project(id) => Some(id),
            },
            include_archived: opts.include_archived,
            ..TaskFilter::default()
        };
        let tasks = r.tasks(&filter)?;
        let rows = tree_rows(&ctx, tasks)?;

        // En revisión o esperando respuesta: dos consultas acotadas, no todo el workspace.
        let mut review = Vec::new();
        let in_review = r.tasks(&TaskFilter {
            statuses: vec![Status::InReview],
            ..TaskFilter::default()
        })?;
        let asking = r.tasks(&TaskFilter {
            needs_human: true,
            ..TaskFilter::default()
        })?;
        let mut seen = HashSet::new();
        for t in in_review.into_iter().chain(asking) {
            if seen.insert(t.id) {
                review.push(ctx.row(t, 0)?);
            }
        }

        let inbox = r
            .inbox_items(Some(InboxStatus::Open))?
            .into_iter()
            .map(|item| {
                let author = ctx.who(&item.author);
                InboxRow { item, author }
            })
            .collect();

        let trash_filter = TaskFilter {
            deleted: true,
            include_archived: true,
            ..TaskFilter::default()
        };
        let trash = r
            .tasks(&trash_filter)?
            .into_iter()
            .map(|t| ctx.row(t, 0))
            .collect::<Result<_, _>>()?;

        let mut agents = Vec::new();
        let claimed = r.claimed_tasks()?;
        for a in &agents_raw {
            let sessions = r.open_sessions(Some(a.id))?;
            let last_seen = sessions.iter().map(|s| s.last_seen).max();
            let connected = a.status == AgentStatus::Active
                && last_seen.is_some_and(|s| now.duration_since(s) <= views::CONNECTED_WINDOW);
            let working_on = claimed
                .iter()
                .filter_map(|t| {
                    let c = t.active_claim(now)?;
                    (c.agent == a.id).then(|| (t.id, keys.of(t), t.title.clone(), c.since))
                })
                .collect();
            agents.push(AgentRow {
                who: ctx.who(&Actor::Agent(a.id)),
                agent: a.clone(),
                connected,
                last_seen,
                working_on,
            });
        }

        let activity = activity_lines(&ctx, &opts.activity_filter.to_store(opts.activity_limit))?;

        let epics = epic_lines(r, &projects)?;
        let diagrams = diagram_lines(r, &keys, &projects)?;
        let cycles = r.cycles(None)?;
        let today = rules::filter::today();
        let names = rules::filter::Names {
            projects: projects.iter().map(|p| (p.id, p.key.clone())).collect(),
            labels: labels.iter().map(|l| (l.id, l.name.clone())).collect(),
            agents: agents_raw.iter().map(|a| (a.id, a.name.clone())).collect(),
            epics: epics
                .iter()
                .map(|e| (e.epic.id, e.epic.name.clone()))
                .collect(),
            cycles: cycles
                .iter()
                .map(|c| {
                    let current = c.closed_at.is_none() && c.start <= today;
                    (c.id, (c.name.clone(), current))
                })
                .collect(),
        };
        Ok(Snapshot {
            workspace: Some(r.workspace()?),
            projects,
            labels,
            interfaces: r.interfaces()?,
            agents,
            rows,
            review,
            inbox,
            trash,
            activity,
            search: Default::default(),
            epics,
            diagrams,
            cycle_progress: cycle_progress(r, &cycles)?,
            open_counts: r.open_counts(&[Status::Done, Status::Cancelled])?,
            cycles,
            templates: r.templates()?,
            views: r.views()?,
            names,
            today: r
                .today_marks()?
                .into_iter()
                .filter(|(_, d)| *d == today)
                .map(|(t, _)| t)
                .collect(),
            locked: r.locked_subtrees()?,
            dependencies: r.dependencies()?,
        })
    }
}

/// Datos de todo el workspace leídos con una consulta cada uno, para no
/// preguntar tarea a tarea (RNF-06).
struct Bulk {
    children: HashMap<TaskId, (usize, usize)>,
    blocked: HashSet<TaskId>,
    comments: HashMap<TaskId, usize>,
    /// Umbral de las tareas envejecidas (RF-VIS-16).
    stale_weeks: u32,
}

impl Bulk {
    fn load(r: &dyn Reader) -> Result<Self, EngineError> {
        let closed = rules::closed_statuses();
        Ok(Self {
            children: r.children_summary(&closed)?,
            blocked: r.blocked_tasks(&closed)?,
            comments: r.comment_counts()?,
            stale_weeks: r.workspace()?.settings.stale_weeks,
        })
    }
}

/// Lo que hace falta para convertir tareas del modelo en filas visibles.
struct RowCtx<'a> {
    bulk: &'a Bulk,
    r: &'a dyn Reader,
    keys: &'a Keys,
    labels: &'a HashMap<LabelId, (String, Color32)>,
    agents: &'a [Agent],
    projects: &'a [Project],
    now: Timestamp,
}

impl RowCtx<'_> {
    fn who(&self, actor: &Actor) -> Who {
        let mut function = String::new();
        let (name, seed, color) = match actor {
            Actor::Agent(id) => match self.agents.iter().find(|a| a.id == *id) {
                Some(a) => {
                    function.clone_from(&a.description);
                    (a.name.clone(), a.id.to_string(), theme::parse_hex(&a.color))
                }
                None => (id.to_string(), id.to_string(), theme::GRAY),
            },
            Actor::Supervisor => ("supervisor".into(), "supervisor".into(), theme::TEAL),
            Actor::Interface(id) => (id.to_string(), id.to_string(), theme::GRAY),
            Actor::System => ("system".into(), "system".into(), theme::GRAY),
            Actor::Integration(n) => (n.clone(), n.clone(), theme::GRAY),
        };
        Who {
            name,
            seed,
            color,
            function,
        }
    }

    fn row(&self, task: Task, depth: usize) -> Result<TaskRow, EngineError> {
        let (children_total, children_done) =
            self.bulk.children.get(&task.id).copied().unwrap_or((0, 0));
        let project_key = self
            .projects
            .iter()
            .find(|p| p.id == task.project_id)
            .map(|p| p.key.clone())
            .unwrap_or_default();
        Ok(TaskRow {
            key: self.keys.of(&task),
            project_key,
            parent_key: task
                .parent_id
                .map(|id| self.keys.of_id(self.r, id))
                .transpose()?,
            depth,
            children_total,
            children_done,
            ready: !self.bulk.blocked.contains(&task.id),
            stale: rules::is_stale(&task, self.bulk.stale_weeks, self.now),
            labels: task
                .labels
                .iter()
                .filter_map(|id| self.labels.get(id).cloned())
                .collect(),
            assignee: task.assignee().map(|a| self.who(a)),
            assignees: task.assignees.iter().map(|a| self.who(a)).collect(),
            claim: task.active_claim(self.now).map(|c| ClaimInfo {
                agent: self.who(&Actor::Agent(c.agent)),
                since: c.since,
                until: c.until,
            }),
            comments: self.bulk.comments.get(&task.id).copied().unwrap_or(0),
            task,
        })
    }
}

fn tree_rows(ctx: &RowCtx<'_>, tasks: Vec<Task>) -> Result<Vec<TaskRow>, EngineError> {
    let present: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
    let mut children: HashMap<Option<TaskId>, Vec<Task>> = HashMap::new();
    for t in tasks {
        // Si el padre no está en la lista (archivado, otro filtro) se muestra como raíz.
        let parent = t.parent_id.filter(|p| present.contains(p));
        children.entry(parent).or_default().push(t);
    }
    let mut out = Vec::new();
    let mut stack: Vec<(usize, Task)> = children
        .remove(&None)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .map(|t| (0, t))
        .collect();
    while let Some((depth, task)) = stack.pop() {
        if let Some(kids) = children.remove(&Some(task.id)) {
            stack.extend(kids.into_iter().rev().map(|k| (depth + 1, k)));
        }
        out.push(ctx.row(task, depth)?);
    }
    Ok(out)
}

fn activity_lines(
    ctx: &RowCtx<'_>,
    filter: &ActivityFilter,
) -> Result<Vec<ActivityLine>, EngineError> {
    let mut lines = Vec::new();
    for e in ctx.r.activity(filter)? {
        lines.push(ActivityLine {
            id: e.id,
            at: e.at,
            actor: ctx.who(&e.actor),
            target: views::activity_target(ctx.r, ctx.keys, &e)?,
            command: e.command.clone(),
            action: e.action,
            entity: e.entity,
            task: e.task_id,
            fields: e.changes.into_iter().map(|c| c.field).collect(),
        });
    }
    Ok(lines)
}

/// Referencia corta a otra tarea (relaciones, subtareas, ancestros).
#[derive(Debug, Clone)]
pub struct TaskRef {
    pub id: TaskId,
    pub key: String,
    pub title: String,
    pub status: Status,
}

#[derive(Debug, Clone)]
pub struct CommentLine {
    pub author: Who,
    pub body: String,
    pub at: Timestamp,
}

/// Todo lo que muestra el panel de detalle de una tarea (RF-VIS-03).
pub struct Detail {
    pub row: TaskRow,
    pub ancestors: Vec<TaskRef>,
    pub subtasks: Vec<TaskRef>,
    pub blocked_by: Vec<TaskRef>,
    pub blocks: Vec<TaskRef>,
    pub comments: Vec<CommentLine>,
    pub history: Vec<ActivityLine>,
    /// Posibles bloqueadores: tareas vivas de su proyecto salvo ella.
    pub blocker_options: Vec<TaskRef>,
    pub attachments: Vec<model::Attachment>,
    pub related: Vec<TaskRef>,
    pub duplicate_of: Option<TaskRef>,
    pub duplicates: Vec<TaskRef>,
    /// Tareas que la mencionan con `[[CLAVE]]` (RF-REL-04).
    pub mentioned_by: Vec<TaskRef>,
    pub time: TaskTime,
    pub cost: TaskCost,
    /// Ramas, commits y PRs vinculados (RF-INT-04).
    pub git: Vec<model::GitLink>,
}

/// Tiempo de una tarea (RF-HIS-03/04): del historial y anotado.
#[derive(Debug, Clone, Default)]
pub struct TaskTime {
    /// Minutos con la tarea en curso, por actor (del historial).
    pub auto: Vec<(String, i64)>,
    /// Minutos anotados o cronometrados.
    pub manual: i64,
    /// El cronómetro del supervisor corre en esta tarea.
    pub running_here: bool,
    /// …o en otra (su clave).
    pub running_elsewhere: Option<String>,
}

/// Coste reportado por los agentes (RF-HIS-06) en la tarea y sus subtareas.
#[derive(Debug, Clone, Default)]
pub struct TaskCost {
    /// Por actor: nombre, tokens (entrada + salida) e importe por moneda.
    pub by_actor: Vec<CostLine>,
    pub total: CostLine,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostLine {
    pub who: String,
    pub tokens: u64,
    /// Millonésimas por moneda.
    pub amounts: BTreeMap<String, u64>,
}

impl CostLine {
    fn add(&mut self, e: &model::CostEntry) {
        self.tokens = self
            .tokens
            .saturating_add(e.tokens_in)
            .saturating_add(e.tokens_out);
        let slot = self.amounts.entry(e.currency.clone()).or_default();
        *slot = slot.saturating_add(e.amount_micros);
    }

    /// `0.42 USD`, o `0.42 USD + 1.10 EUR`; vacío si no hay importe.
    pub fn amount_text(&self) -> String {
        self.amounts
            .iter()
            .filter(|(_, m)| **m > 0)
            .map(|(c, m)| format!("{} {c}", rules::cost::format_amount(*m)))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

fn task_cost(r: &dyn Reader, task: &Task) -> Result<TaskCost, EngineError> {
    let mut cost = TaskCost::default();
    for t in engine::lookup::subtree(r, task.clone())? {
        for e in r.cost_entries(&store::CostFilter {
            task: Some(t.id),
            ..store::CostFilter::default()
        })? {
            let who = views::actor_name(r, &e.actor)?;
            match cost.by_actor.iter_mut().find(|l| l.who == who) {
                Some(line) => line.add(&e),
                None => {
                    let mut line = CostLine {
                        who,
                        ..CostLine::default()
                    };
                    line.add(&e);
                    cost.by_actor.push(line);
                }
            }
            cost.total.add(&e);
        }
    }
    Ok(cost)
}

pub fn load_detail(conn: &mut Conn, id: TaskId) -> Result<Option<Detail>, EngineError> {
    let now = Timestamp::now();
    conn.read(|r| {
        let Some(task) = r.task(id)? else {
            return Ok(None);
        };
        let keys = Keys::load(r)?;
        let projects = r.projects(true)?;
        let label_map: HashMap<LabelId, (String, Color32)> = r
            .labels()?
            .iter()
            .map(|l| (l.id, (l.name.clone(), theme::parse_hex(&l.color))))
            .collect();
        let agents = r.agents()?;
        let bulk = Bulk::load(r)?;
        let ctx = RowCtx {
            r,
            keys: &keys,
            labels: &label_map,
            agents: &agents,
            projects: &projects,
            now,
            bulk: &bulk,
        };
        let to_ref = |t: &Task| TaskRef {
            id: t.id,
            key: keys.of(t),
            title: t.title.clone(),
            status: t.status,
        };
        let refs = |ids: Vec<TaskId>| -> Result<Vec<TaskRef>, EngineError> {
            let mut out = Vec::new();
            for id in ids {
                if let Some(t) = r.task(id)?
                    && t.deleted_at.is_none()
                {
                    out.push(to_ref(&t));
                }
            }
            Ok(out)
        };

        let ancestors = engine::lookup::ancestors(r, &task)?
            .iter()
            .rev()
            .map(to_ref)
            .collect();
        let subtasks = r
            .children(task.id)?
            .iter()
            .filter(|c| c.deleted_at.is_none())
            .map(to_ref)
            .collect();
        // Bloqueadores posibles: las tareas vivas de su proyecto (con 50 000
        // tareas, todo el workspace sería una lista inmanejable y lenta).
        let blocker_options = r
            .tasks(&TaskFilter::project(task.project_id))?
            .iter()
            .filter(|t| t.id != task.id)
            .map(to_ref)
            .collect();
        let comments = r
            .comments(task.id)?
            .into_iter()
            .map(|c| CommentLine {
                author: ctx.who(&c.author),
                body: c.body,
                at: c.created_at,
            })
            .collect();
        let history = activity_lines(
            &ctx,
            &ActivityFilter {
                task: Some(task.id),
                limit: 100,
                ..ActivityFilter::default()
            },
        )?;

        let mut related = Vec::new();
        let mut duplicate_of = None;
        let mut duplicates = Vec::new();
        for rel in r.relations(Some(task.id))? {
            let other = if rel.a == task.id { rel.b } else { rel.a };
            let Some(other) = refs(vec![other])?.into_iter().next() else {
                continue;
            };
            match rel.kind {
                model::RelationKind::Relates => related.push(other),
                model::RelationKind::Duplicates if rel.a == task.id => duplicate_of = Some(other),
                model::RelationKind::Duplicates => duplicates.push(other),
            }
        }
        let mut sources: Vec<TaskId> = Vec::new();
        for m in r.mentions_of(task.id)? {
            if !sources.contains(&m.source) {
                sources.push(m.source);
            }
        }
        let time = task_time(r, &keys, task.id, now)?;
        let cost = task_cost(r, &task)?;
        Ok(Some(Detail {
            time,
            cost,
            git: r.git_links(Some(task.id))?,
            blocked_by: refs(r.blockers_of(task.id)?)?,
            blocks: refs(r.blocks(task.id)?)?,
            attachments: r.attachments(task.id)?,
            related,
            duplicate_of,
            duplicates,
            mentioned_by: refs(sources)?,
            row: ctx.row(task, 0)?,
            ancestors,
            subtasks,
            comments,
            history,
            blocker_options,
        }))
    })
}

/// Proyecto mínimo para los tests de vistas.
#[cfg(test)]
pub(crate) fn test_project(key: &str) -> Project {
    Project {
        id: ProjectId::generate(),
        key: key.into(),
        name: key.into(),
        description: String::new(),
        status: model::ProjectStatus::Active,
        next_number: 1,
        next_diagram: 1,
        created_at: Timestamp::UNIX_EPOCH,
        updated_at: Timestamp::UNIX_EPOCH,
        locked: false,
        workflow: Vec::new(),
        estimate_unit: model::EstimateUnit::Hours,
        fields: Vec::new(),
    }
}
