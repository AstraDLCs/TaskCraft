//! Importación en modo fusión (RF-EXP-03b): lo que no existe se crea; lo
//! que existe (mismo ULID) se actualiza si el archivo es más reciente. Si el
//! destino cambió después, se conserva el destino y se informa del conflicto.

use std::collections::{HashMap, HashSet};

use model::{AgentId, LabelId, ProjectId, TaskId, Timestamp};
use store::{Reader, TaskFilter};

use crate::plan::{Collision, Counts, Mode, Plan};
use crate::{Dump, ExchangeError, Problem};

/// Qué hacer con algo que ya existe en el destino.
enum Fate {
    New,
    Update,
    /// Igual o más antiguo en el archivo: no se toca.
    Skip,
    /// El destino cambió después: se conserva y se informa.
    Conflict,
}

fn fate(local: Option<Timestamp>, incoming: Timestamp) -> Fate {
    match local {
        None => Fate::New,
        Some(l) if incoming > l => Fate::Update,
        Some(l) if incoming == l => Fate::Skip,
        Some(_) => Fate::Conflict,
    }
}

pub(crate) fn plan_merge(target: &dyn Reader, mut dump: Dump) -> Result<Plan, ExchangeError> {
    let mut collisions = Vec::new();
    let mut updates: HashSet<String> = HashSet::new();
    let mut created = Counts::default();
    let mut updated = Counts::default();
    let conflict = |kind: &'static str, name: String| Collision {
        kind,
        name,
        resolution: "kept-local".into(),
    };

    // Proyectos: por ULID; una clave ya usada por otro proyecto no se puede fusionar.
    let mut problems = Vec::new();
    let mut keep_projects = Vec::new();
    for p in dump.projects.drain(..) {
        let local = target.project(p.id)?;
        if local.is_none()
            && let Some(other) = target.project_by_key(&p.key)?
        {
            problems.push(Problem {
                code: "project-key-taken",
                detail: format!("{} ({} ≠ {})", p.key, p.id, other.id),
            });
            continue;
        }
        match fate(local.map(|l| l.updated_at), p.updated_at) {
            Fate::New => {
                created.projects += 1;
                keep_projects.push(p);
            }
            Fate::Update => {
                updated.projects += 1;
                updates.insert(p.id.to_string());
                keep_projects.push(p);
            }
            Fate::Skip => {}
            Fate::Conflict => collisions.push(conflict("project", p.key.clone())),
        }
    }
    if !problems.is_empty() {
        return Err(ExchangeError::Invalid(problems));
    }
    dump.projects = keep_projects;

    // Etiquetas y agentes: por ULID o por nombre; los existentes no se tocan.
    let mut labels: HashMap<LabelId, LabelId> = HashMap::new();
    let mut existing_labels = Vec::new();
    for l in &mut dump.labels {
        let found = match target.label(l.id)? {
            Some(x) => Some(x),
            None => target.label_by_name(&l.name)?,
        };
        if let Some(found) = found {
            existing_labels.push(found.id);
            labels.insert(l.id, found.id);
            l.id = found.id;
        } else {
            created.labels += 1;
        }
    }
    let (interfaces, existing_interfaces) =
        crate::plan::map_interfaces(target, &mut dump, false, &mut collisions)?;
    let mut agents: HashMap<AgentId, AgentId> = HashMap::new();
    let mut existing_agents = Vec::new();
    for a in &mut dump.agents {
        a.interface_id = a
            .interface_id
            .map(|i| interfaces.get(&i).copied().unwrap_or(i));
        let found = match target.agent(a.id)? {
            Some(x) => Some(x),
            None => target.agent_by_name(&a.name)?,
        };
        if let Some(found) = found {
            existing_agents.push(found.id);
            agents.insert(a.id, found.id);
            a.id = found.id;
        } else {
            created.agents += 1;
        }
    }
    let actor = |a: &model::Actor| match a {
        model::Actor::Agent(id) => model::Actor::Agent(agents.get(id).copied().unwrap_or(*id)),
        other => other.clone(),
    };

    // Épicas y plantillas: por ULID y fecha.
    let mut epics = Vec::new();
    for e in dump.epics.drain(..) {
        match fate(target.epic(e.id)?.map(|x| x.updated_at), e.updated_at) {
            Fate::New => {
                created.epics += 1;
                epics.push(e);
            }
            Fate::Update => {
                updated.epics += 1;
                updates.insert(e.id.to_string());
                epics.push(e);
            }
            Fate::Skip => {}
            Fate::Conflict => collisions.push(conflict("epic", e.name.clone())),
        }
    }
    dump.epics = epics;
    let mut cycles = Vec::new();
    for c in dump.cycles.drain(..) {
        match fate(target.cycle(c.id)?.map(|x| x.updated_at), c.updated_at) {
            Fate::New => {
                created.cycles += 1;
                cycles.push(c);
            }
            Fate::Update => {
                updated.cycles += 1;
                updates.insert(c.id.to_string());
                cycles.push(c);
            }
            Fate::Skip => {}
            Fate::Conflict => collisions.push(conflict("cycle", c.name.clone())),
        }
    }
    dump.cycles = cycles;
    let mut diagrams = Vec::new();
    for g in dump.diagrams.drain(..) {
        let local = target.diagram(g.id)?;
        // Otro diagrama con el mismo número en el destino: se conserva el
        // local, porque las citas `[[API-D3]]` ya apuntan a él.
        let taken = local.is_none() && target.diagram_by_number(g.project_id, g.number)?.is_some();
        if taken {
            collisions.push(Collision {
                kind: "diagram_number",
                name: format!("D{} {}", g.number, g.title),
                resolution: "kept-local".into(),
            });
            continue;
        }
        match fate(local.map(|x| x.updated_at), g.updated_at) {
            Fate::New => {
                created.diagrams += 1;
                diagrams.push(g);
            }
            Fate::Update => {
                updated.diagrams += 1;
                updates.insert(g.id.to_string());
                diagrams.push(g);
            }
            Fate::Skip => {}
            Fate::Conflict => collisions.push(conflict("diagram", g.title.clone())),
        }
    }
    dump.diagrams = diagrams;
    let mut skipped_templates = Vec::new();
    let mut templates = Vec::new();
    for t in dump.templates.drain(..) {
        let local = target.template(t.id)?;
        if local.is_none() && target.template_by_name(&t.name)?.is_some() {
            collisions.push(Collision {
                kind: "template",
                name: t.name.clone(),
                resolution: "kept-existing".into(),
            });
            continue;
        }
        match fate(local.map(|x| x.updated_at), t.updated_at) {
            Fate::New => {
                created.templates += 1;
                templates.push(t);
            }
            Fate::Update => {
                updated.templates += 1;
                updates.insert(t.id.to_string());
                templates.push(t);
            }
            Fate::Skip => {}
            Fate::Conflict => {
                skipped_templates.push(t.id);
                collisions.push(conflict("template", t.name.clone()));
            }
        }
    }
    dump.templates = templates;

    // Tareas: por ULID y fecha; una tarea nueva cuyo número ya está ocupado
    // recibe el siguiente libre del proyecto.
    let all_local = target.tasks(&TaskFilter {
        include_archived: true,
        ..TaskFilter::default()
    })?;
    let trash = target.tasks(&TaskFilter {
        deleted: true,
        include_archived: true,
        ..TaskFilter::default()
    })?;
    let mut numbers: HashMap<ProjectId, HashSet<u64>> = HashMap::new();
    for t in all_local.iter().chain(&trash) {
        numbers.entry(t.project_id).or_default().insert(t.number);
    }
    let keys: HashMap<ProjectId, String> = target
        .projects(true)?
        .into_iter()
        .map(|p| (p.id, p.key))
        .chain(dump.projects.iter().map(|p| (p.id, p.key.clone())))
        .collect();
    let key_of = |p: ProjectId, n: u64| format!("{}-{n}", keys.get(&p).map_or("?", String::as_str));
    let mut tasks = Vec::new();
    let mut known: HashSet<TaskId> = HashSet::new();
    for mut t in dump.tasks.drain(..) {
        let local = target.task(t.id)?;
        t.labels = t
            .labels
            .iter()
            .map(|l| labels.get(l).copied().unwrap_or(*l))
            .collect();
        crate::plan::remap_people(&mut t, &actor);
        t.creator = actor(&t.creator);
        t.claim = None;
        match fate(local.as_ref().map(|x| x.updated_at), t.updated_at) {
            Fate::New => {
                let used = numbers.entry(t.project_id).or_default();
                if used.contains(&t.number) {
                    let old = key_of(t.project_id, t.number);
                    let next = used.iter().max().copied().unwrap_or(0).saturating_add(1);
                    t.number = next;
                    collisions.push(Collision {
                        kind: "task_number",
                        name: old,
                        resolution: format!("renumbered:{}", key_of(t.project_id, next)),
                    });
                }
                used.insert(t.number);
                created.tasks += 1;
                known.insert(t.id);
                tasks.push(t);
            }
            Fate::Update => {
                // El número y el proyecto son la clave: se conservan los locales.
                if let Some(l) = &local {
                    t.number = l.number;
                    t.project_id = l.project_id;
                    t.claim = l.claim;
                }
                updated.tasks += 1;
                updates.insert(t.id.to_string());
                known.insert(t.id);
                tasks.push(t);
            }
            Fate::Skip => {}
            Fate::Conflict => {
                let key = local
                    .map(|l| key_of(l.project_id, l.number))
                    .unwrap_or_default();
                collisions.push(conflict("task", key));
            }
        }
    }
    dump.tasks = tasks;

    // Vínculos: solo los que faltan.
    let deps: HashSet<_> = target.dependencies()?.into_iter().collect();
    dump.dependencies.retain(|d| !deps.contains(d));
    created.dependencies = dump.dependencies.len();
    let rels: HashSet<_> = target.relations(None)?.into_iter().collect();
    dump.relations.retain(|r| !rels.contains(r));
    created.relations = dump.relations.len();

    // Lo inmutable (comentarios, adjuntos, historial, bandeja, vistas): lo que falta.
    let mut comment_ids = HashSet::new();
    for t in all_local.iter().chain(&trash) {
        for c in target.comments(t.id)? {
            comment_ids.insert(c.id);
        }
    }
    dump.comments.retain(|c| !comment_ids.contains(&c.id));
    for c in &mut dump.comments {
        c.author = actor(&c.author);
    }
    created.comments = dump.comments.len();
    let mut attachments = Vec::new();
    for a in dump.attachments.drain(..) {
        if target.attachment(a.id)?.is_none() {
            attachments.push(a);
        }
    }
    dump.attachments = attachments;
    created.attachments = dump.attachments.len();
    let costs: HashSet<_> = target
        .cost_entries(&store::CostFilter::default())?
        .iter()
        .map(|c| c.id)
        .collect();
    dump.costs.retain(|c| !costs.contains(&c.id));
    for c in &mut dump.costs {
        c.actor = actor(&c.actor);
    }
    created.costs = dump.costs.len();
    let links: HashSet<_> = target.git_links(None)?.iter().map(|l| l.id).collect();
    dump.git_links.retain(|l| !links.contains(&l.id));
    for l in &mut dump.git_links {
        l.actor = actor(&l.actor);
    }
    created.git_links = dump.git_links.len();
    let mut activity = Vec::new();
    for mut e in dump.activity.drain(..) {
        if target.activity_entry(e.id)?.is_none() {
            e.actor = actor(&e.actor);
            activity.push(e);
        }
    }
    dump.activity = activity;
    created.activity = dump.activity.len();
    let mut inbox = Vec::new();
    for i in dump.inbox.drain(..) {
        if target.inbox_item(i.id)?.is_none() {
            inbox.push(i);
        }
    }
    dump.inbox = inbox;
    created.inbox = dump.inbox.len();
    let views: HashSet<String> = target
        .views()?
        .into_iter()
        .flat_map(|v| [v.id.to_string(), v.name.to_lowercase()])
        .collect();
    dump.views
        .retain(|v| !views.contains(&v.id.to_string()) && !views.contains(&v.name.to_lowercase()));

    Ok(Plan {
        mode: Mode::Merge,
        dump,
        existing_labels,
        existing_agents,
        existing_interfaces,
        skipped_templates,
        collisions,
        creates: created,
        updates: updated,
        updated_ids: updates,
    })
}
