//! Planificación y aplicación de una importación (RF-EXP-03a, RF-EXP-05):
//! qué se crea, qué colisiona y cómo se resuelve, antes de tocar nada.

use std::collections::{HashMap, HashSet};

use foundation::Ulid;
use model::{
    ActivityEntry, Actor, AgentId, CommentId, InboxId, LabelId, ProjectId, TaskId, WorkspaceInfo,
};
use serde::Serialize;
use store::{Reader, StoreError, TaskFilter, Writer};

use crate::{Dump, ExchangeError, validate};

/// Dónde va lo importado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Workspace vacío: se conservan todos los IDs (ida y vuelta, RF-EXP-02).
    Workspace,
    /// Como proyectos nuevos de un workspace con datos: IDs nuevos y claves
    /// renombradas si colisionan.
    Projects,
    /// Sobre el mismo workspace (o una copia): actualiza por ULID y crea lo
    /// que falta, con informe de conflictos (RF-EXP-03b).
    Merge,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "workspace" => Ok(Self::Workspace),
            "project" | "projects" | "proyecto" => Ok(Self::Projects),
            "merge" | "fusion" | "fusión" => Ok(Self::Merge),
            other => Err(other.to_owned()),
        }
    }
}

/// Algo que ya existía y cómo se resolvió.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Collision {
    /// `project_key`, `label`, `agent`, `template`.
    pub kind: &'static str,
    pub name: String,
    /// `renamed:API2`, `merged`…
    pub resolution: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Counts {
    pub projects: usize,
    pub epics: usize,
    pub templates: usize,
    pub attachments: usize,
    pub relations: usize,
    pub costs: usize,
    pub git_links: usize,
    pub cycles: usize,
    pub diagrams: usize,
    pub tasks: usize,
    pub labels: usize,
    pub agents: usize,
    pub dependencies: usize,
    pub comments: usize,
    pub activity: usize,
    pub inbox: usize,
}

/// Lo que se va a escribir, ya remapeado.
#[derive(Debug, Clone)]
pub struct Plan {
    pub mode: Mode,
    pub dump: Dump,
    /// Etiquetas, agentes y plantillas que ya existían: no se crean.
    pub(crate) existing_labels: Vec<LabelId>,
    pub(crate) existing_agents: Vec<AgentId>,
    /// Interfaces que ya existían (por ID o nombre): no se crean.
    pub(crate) existing_interfaces: Vec<model::InterfaceId>,
    pub(crate) skipped_templates: Vec<model::TemplateId>,
    pub collisions: Vec<Collision>,
    pub creates: Counts,
    /// En fusión: lo que ya existía y se actualiza.
    pub updates: Counts,
    /// IDs (como texto) que se actualizan en lugar de crearse.
    pub(crate) updated_ids: HashSet<String>,
}

/// Valida el archivo contra el workspace de destino y decide cómo encaja.
pub fn plan(target: &dyn Reader, dump: Dump, mode: Mode) -> Result<Plan, ExchangeError> {
    let problems = validate(&dump);
    if !problems.is_empty() {
        return Err(ExchangeError::Invalid(problems));
    }
    match mode {
        Mode::Workspace => plan_workspace(target, dump),
        Mode::Projects => plan_projects(target, dump),
        Mode::Merge => crate::merge::plan_merge(target, dump),
    }
}

fn plan_workspace(target: &dyn Reader, dump: Dump) -> Result<Plan, ExchangeError> {
    let any_task = !target
        .tasks(&TaskFilter {
            include_archived: true,
            ..TaskFilter::default()
        })?
        .is_empty();
    if !target.projects(true)?.is_empty()
        || any_task
        || !target.agents()?.is_empty()
        || !target.interfaces()?.is_empty()
        || !target.labels()?.is_empty()
        || !target.inbox_items(None)?.is_empty()
        || !target.templates()?.is_empty()
    {
        return Err(ExchangeError::TargetNotEmpty);
    }
    let creates = counts(&dump, 0, 0, 0);
    Ok(Plan {
        mode: Mode::Workspace,
        dump,
        existing_labels: Vec::new(),
        existing_agents: Vec::new(),
        existing_interfaces: Vec::new(),
        skipped_templates: Vec::new(),
        collisions: Vec::new(),
        creates,
        updates: Counts::default(),
        updated_ids: HashSet::new(),
    })
}

/// ID nuevo con el mismo instante que el original: el orden por tiempo
/// (historial, comentarios) se conserva.
fn fresh(old: Ulid) -> Ulid {
    Ulid::from_parts(old.timestamp_ms(), Ulid::generate().random())
}

fn plan_projects(target: &dyn Reader, mut dump: Dump) -> Result<Plan, ExchangeError> {
    let mut collisions = Vec::new();

    let mut projects: HashMap<ProjectId, ProjectId> = HashMap::new();
    let mut renamed: Vec<(String, String)> = Vec::new();
    for p in &mut dump.projects {
        let new = ProjectId::from_ulid(fresh(p.id.ulid()));
        projects.insert(p.id, new);
        p.id = new;
        let original = p.key.clone();
        let mut n = 2;
        while target.project_by_key(&p.key)?.is_some() {
            p.key = format!("{original}{n}");
            n += 1;
        }
        if p.key != original {
            renamed.push((original.clone(), p.key.clone()));
            collisions.push(Collision {
                kind: "project_key",
                name: original,
                resolution: format!("renamed:{}", p.key),
            });
        }
    }

    let mut labels: HashMap<LabelId, LabelId> = HashMap::new();
    let mut existing_labels = Vec::new();
    for l in &mut dump.labels {
        let new = match target.label_by_name(&l.name)? {
            Some(found) => {
                existing_labels.push(found.id);
                collisions.push(Collision {
                    kind: "label",
                    name: l.name.clone(),
                    resolution: "merged".into(),
                });
                found.id
            }
            None => LabelId::from_ulid(fresh(l.id.ulid())),
        };
        labels.insert(l.id, new);
        l.id = new;
    }

    let (interfaces, existing_interfaces) =
        map_interfaces(target, &mut dump, true, &mut collisions)?;
    let mut agents: HashMap<AgentId, AgentId> = HashMap::new();
    let mut existing_agents = Vec::new();
    for a in &mut dump.agents {
        a.interface_id = a
            .interface_id
            .map(|i| interfaces.get(&i).copied().unwrap_or(i));
        let new = match target.agent_by_name(&a.name)? {
            Some(found) => {
                existing_agents.push(found.id);
                collisions.push(Collision {
                    kind: "agent",
                    name: a.name.clone(),
                    resolution: "merged".into(),
                });
                found.id
            }
            None => AgentId::from_ulid(fresh(a.id.ulid())),
        };
        agents.insert(a.id, new);
        a.id = new;
    }
    let actor = |a: &Actor| match a {
        Actor::Agent(id) => Actor::Agent(agents.get(id).copied().unwrap_or(*id)),
        other => other.clone(),
    };

    let mut epics: HashMap<model::EpicId, model::EpicId> = HashMap::new();
    for e in &mut dump.epics {
        let new = model::EpicId::from_ulid(fresh(e.id.ulid()));
        epics.insert(e.id, new);
        e.id = new;
        e.project_id = projects.get(&e.project_id).copied().unwrap_or(e.project_id);
    }
    let group = |g: &Option<model::EpicId>| g.map(|g| epics.get(&g).copied().unwrap_or(g));
    let mut cycles: HashMap<model::CycleId, model::CycleId> = HashMap::new();
    for c in &mut dump.cycles {
        let new = model::CycleId::from_ulid(fresh(c.id.ulid()));
        cycles.insert(c.id, new);
        c.id = new;
        c.project_id = projects.get(&c.project_id).copied().unwrap_or(c.project_id);
    }
    // Con los proyectos nuevos, los diagramas conservan su número: las citas
    // `[[API-D3]]` siguen la clave del proyecto, como las menciones.
    for g in &mut dump.diagrams {
        g.id = model::DiagramId::from_ulid(fresh(g.id.ulid()));
        g.project_id = projects.get(&g.project_id).copied().unwrap_or(g.project_id);
    }

    // Las vistas son del workspace: un proyecto importado no las trae.
    dump.views.clear();
    let mut skipped_templates = Vec::new();
    for tpl in &mut dump.templates {
        tpl.id = model::TemplateId::from_ulid(fresh(tpl.id.ulid()));
        if target.template_by_name(&tpl.name)?.is_some() {
            skipped_templates.push(tpl.id);
            collisions.push(Collision {
                kind: "template",
                name: tpl.name.clone(),
                resolution: "kept-existing".into(),
            });
        }
    }

    let mut tasks: HashMap<TaskId, TaskId> = HashMap::new();
    for t in &dump.tasks {
        tasks.insert(t.id, TaskId::from_ulid(fresh(t.id.ulid())));
    }
    let task = |id: &TaskId| tasks.get(id).copied().unwrap_or(*id);
    let project = |id: &ProjectId| projects.get(id).copied().unwrap_or(*id);
    for t in &mut dump.tasks {
        t.id = task(&t.id);
        t.project_id = project(&t.project_id);
        t.parent_id = t.parent_id.as_ref().map(task);
        t.labels = t
            .labels
            .iter()
            .map(|l| labels.get(l).copied().unwrap_or(*l))
            .collect();
        remap_people(t, &actor);
        t.creator = actor(&t.creator);
        for item in t.checklist.iter_mut().chain(t.criteria.iter_mut()) {
            item.id = model::ItemId::from_ulid(fresh(item.id.ulid()));
        }
        // Un lease no tiene sentido en otro workspace: la tarea llega libre.
        t.claim = None;
        t.epic_id = group(&t.epic_id);
        t.milestone_id = group(&t.milestone_id);
        for c in &mut t.cycles {
            *c = cycles.get(c).copied().unwrap_or(*c);
        }
    }
    for rel in &mut dump.relations {
        rel.a = task(&rel.a);
        rel.b = task(&rel.b);
    }
    for a in &mut dump.attachments {
        a.id = model::AttachmentId::from_ulid(fresh(a.id.ulid()));
        a.task_id = task(&a.task_id);
        a.author = actor(&a.author);
    }
    for l in &mut dump.git_links {
        l.id = model::GitLinkId::from_ulid(fresh(l.id.ulid()));
        l.task_id = task(&l.task_id);
        l.actor = actor(&l.actor);
    }
    for c in &mut dump.costs {
        c.id = model::CostEntryId::from_ulid(fresh(c.id.ulid()));
        c.task_id = task(&c.task_id);
        c.actor = actor(&c.actor);
    }
    for d in &mut dump.dependencies {
        d.blocker = task(&d.blocker);
        d.blocked = task(&d.blocked);
    }
    // Las menciones del texto siguen a la clave renombrada (`[[API-1]]` → `[[API2-1]]`).
    for t in &mut dump.tasks {
        t.description = rename_mentions(&t.description, &renamed);
    }
    for c in &mut dump.comments {
        c.body = rename_mentions(&c.body, &renamed);
        c.id = CommentId::from_ulid(fresh(c.id.ulid()));
        c.task_id = task(&c.task_id);
        c.author = actor(&c.author);
    }
    for e in &mut dump.activity {
        remap_entry(e, &tasks, &projects, &actor);
    }
    dump.inbox.clear();

    let creates = counts(
        &dump,
        existing_labels.len(),
        existing_agents.len(),
        skipped_templates.len(),
    );
    Ok(Plan {
        mode: Mode::Projects,
        dump,
        existing_labels,
        existing_agents,
        existing_interfaces,
        skipped_templates,
        collisions,
        creates,
        updates: Counts::default(),
        updated_ids: HashSet::new(),
    })
}

/// Interfaces del archivo frente a las del destino (D15): por ULID o por
/// nombre se reutiliza la existente; si no, se crea (con ULID nuevo si
/// `fresh_ids`). Ninguna trae token.
pub(crate) fn map_interfaces(
    target: &dyn Reader,
    dump: &mut Dump,
    fresh_ids: bool,
    collisions: &mut Vec<Collision>,
) -> Result<
    (
        HashMap<model::InterfaceId, model::InterfaceId>,
        Vec<model::InterfaceId>,
    ),
    StoreError,
> {
    let mut map = HashMap::new();
    let mut existing = Vec::new();
    for i in &mut dump.interfaces {
        i.token_hash = None;
        let found = match target.interface(i.id)? {
            Some(x) => Some(x),
            None => target.interface_by_name(&i.name)?,
        };
        let new = match found {
            Some(found) => {
                existing.push(found.id);
                // Al fusionar, que ya exista es lo normal: no es una colisión.
                if fresh_ids {
                    collisions.push(Collision {
                        kind: "interface",
                        name: i.name.clone(),
                        resolution: "merged".into(),
                    });
                }
                found.id
            }
            None if fresh_ids => model::InterfaceId::from_ulid(fresh(i.id.ulid())),
            None => i.id,
        };
        map.insert(i.id, new);
        i.id = new;
    }
    Ok((map, existing))
}

/// Asignados, con los agentes del destino.
pub(crate) fn remap_people(t: &mut model::Task, actor: &impl Fn(&Actor) -> Actor) {
    for a in &mut t.assignees {
        *a = actor(a);
    }
}

fn rename_mentions(text: &str, renamed: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for (old, new) in renamed {
        for (from, to) in [
            (format!("[[{old}-"), format!("[[{new}-")),
            (format!("[[{}-", old.to_lowercase()), format!("[[{new}-")),
        ] {
            out = out.replace(&from, &to);
        }
    }
    out
}

fn remap_entry(
    e: &mut ActivityEntry,
    tasks: &HashMap<TaskId, TaskId>,
    projects: &HashMap<ProjectId, ProjectId>,
    actor: &impl Fn(&Actor) -> Actor,
) {
    e.id = model::ActivityId::from_ulid(fresh(e.id.ulid()));
    e.actor = actor(&e.actor);
    e.task_id = e.task_id.map(|t| tasks.get(&t).copied().unwrap_or(t));
    e.project_id = e.project_id.map(|p| projects.get(&p).copied().unwrap_or(p));
    if let Ok(old) = e.entity_id.parse::<TaskId>()
        && let Some(new) = tasks.get(&old)
    {
        e.entity_id = new.to_string();
    }
}

fn counts(d: &Dump, merged_labels: usize, merged_agents: usize, kept_templates: usize) -> Counts {
    Counts {
        projects: d.projects.len(),
        epics: d.epics.len(),
        templates: d.templates.len().saturating_sub(kept_templates),
        attachments: d.attachments.len(),
        relations: d.relations.len(),
        costs: d.costs.len(),
        git_links: d.git_links.len(),
        cycles: d.cycles.len(),
        diagrams: d.diagrams.len(),
        tasks: d.tasks.len(),
        labels: d.labels.len().saturating_sub(merged_labels),
        agents: d.agents.len().saturating_sub(merged_agents),
        dependencies: d.dependencies.len(),
        comments: d.comments.len(),
        activity: d.activity.len(),
        inbox: d.inbox.len(),
    }
}

/// Que el contador de cada proyecto quede por encima de sus diagramas: en
/// una fusión pueden llegar diagramas nuevos a un proyecto que no cambia.
fn bump_diagram_counters(
    w: &mut dyn Writer,
    diagrams: &[model::Diagram],
) -> Result<(), StoreError> {
    let mut highest: HashMap<ProjectId, u64> = HashMap::new();
    for g in diagrams {
        let n = highest.entry(g.project_id).or_default();
        *n = (*n).max(g.number);
    }
    for (id, number) in highest {
        if let Some(mut p) = w.project(id)?
            && p.next_diagram <= number
        {
            p.next_diagram = number.saturating_add(1);
            w.update_project(&p)?;
        }
    }
    Ok(())
}

/// Escribe lo planificado y devuelve el historial importado, que el engine
/// añade en la misma transacción (solo `Transaction` escribe historial).
pub fn apply(w: &mut dyn Writer, plan: Plan) -> Result<Vec<ActivityEntry>, StoreError> {
    let Plan {
        mode,
        dump,
        existing_labels,
        existing_agents,
        existing_interfaces,
        skipped_templates,
        updated_ids,
        ..
    } = plan;
    let update = |id: &dyn ToString| updated_ids.contains(&id.to_string());
    if mode == Mode::Workspace {
        let current = w.workspace()?;
        w.update_workspace(&WorkspaceInfo {
            id: current.id,
            created_at: current.created_at,
            ..dump.workspace
        })?;
    }
    for l in dump
        .labels
        .iter()
        .filter(|l| !existing_labels.contains(&l.id))
    {
        w.insert_label(l)?;
    }
    for i in dump
        .interfaces
        .iter()
        .filter(|i| !existing_interfaces.contains(&i.id))
    {
        w.insert_interface(i)?;
    }
    for a in dump
        .agents
        .iter()
        .filter(|a| !existing_agents.contains(&a.id))
    {
        w.insert_agent(a)?;
    }
    for p in &dump.projects {
        if update(&p.id) {
            w.update_project(p)?;
        } else {
            w.insert_project(p)?;
        }
    }
    for e in &dump.epics {
        if update(&e.id) {
            w.update_epic(e)?;
        } else {
            w.insert_epic(e)?;
        }
    }
    for c in &dump.cycles {
        if update(&c.id) {
            w.update_cycle(c)?;
        } else {
            w.insert_cycle(c)?;
        }
    }
    for g in &dump.diagrams {
        if update(&g.id) {
            w.update_diagram(g)?;
        } else {
            w.insert_diagram(g)?;
        }
    }
    bump_diagram_counters(w, &dump.diagrams)?;
    for t in &dump.tasks {
        if update(&t.id) {
            w.update_task(t)?;
        } else {
            w.insert_task(t)?;
        }
    }
    for d in &dump.dependencies {
        w.add_dependency(*d)?;
    }
    for rel in &dump.relations {
        w.add_relation(*rel)?;
    }
    for a in &dump.attachments {
        let data = dump
            .blobs
            .get(&a.sha256)
            .map(Vec::as_slice)
            .unwrap_or_default();
        w.insert_attachment(a, data)?;
    }
    for tpl in dump
        .templates
        .iter()
        .filter(|t| !skipped_templates.contains(&t.id))
    {
        if update(&tpl.id) {
            w.update_template(tpl)?;
        } else {
            w.insert_template(tpl)?;
        }
    }
    for v in &dump.views {
        w.save_view(v)?;
    }
    for c in &dump.costs {
        w.insert_cost_entry(c)?;
    }
    for l in &dump.git_links {
        w.insert_git_link(l)?;
    }
    for c in &dump.comments {
        w.insert_comment(c)?;
    }
    for i in &dump.inbox {
        let item = match mode {
            Mode::Workspace | Mode::Merge => i.clone(),
            Mode::Projects => model::InboxItem {
                id: InboxId::from_ulid(fresh(i.id.ulid())),
                ..i.clone()
            },
        };
        w.insert_inbox_item(&item)?;
    }
    Ok(dump.activity)
}
