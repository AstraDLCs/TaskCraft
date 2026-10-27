//! Representación JSON de las entidades para los clientes.

use model::{Project, Task};
use serde_json::{Value, json};
use store::Reader;

use crate::EngineError;
use crate::lookup::Keys;

pub fn project(p: &Project) -> Value {
    json!({
        "id": p.id,
        "key": p.key,
        "name": p.name,
        "description": p.description,
        "status": p.status,
        "created_at": p.created_at,
        "updated_at": p.updated_at,
    })
}

/// Clave legible de un diagrama: `API-D3` (RF-MOD-19).
pub fn diagram_key(r: &dyn Reader, d: &model::Diagram) -> Result<String, EngineError> {
    let project = r.project(d.project_id)?.map(|p| p.key).unwrap_or_default();
    Ok(model::DiagramKey {
        project,
        number: d.number,
    }
    .to_string())
}

/// Diagrama sin la fuente, para listas y referencias.
pub fn diagram_ref(r: &dyn Reader, d: &model::Diagram) -> Result<Value, EngineError> {
    Ok(json!({ "id": d.id, "key": diagram_key(r, d)?, "title": d.title }))
}

/// Diagramas que cita `task` en su descripción o comentarios, sin repetir.
fn cited_diagrams(r: &dyn Reader, task: model::TaskId) -> Result<Vec<Value>, EngineError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for m in r.diagram_mentions_from(task)? {
        if seen.contains(&m.target) {
            continue;
        }
        seen.push(m.target);
        if let Some(d) = r.diagram(m.target)? {
            out.push(diagram_ref(r, &d)?);
        }
    }
    Ok(out)
}

/// Una tarea está lista si ningún bloqueador vivo sigue abierto.
pub fn is_ready(r: &dyn Reader, task: &Task) -> Result<bool, EngineError> {
    let mut statuses = Vec::new();
    for id in r.blockers_of(task.id)? {
        if let Some(b) = r.task(id)?
            && b.deleted_at.is_none()
        {
            statuses.push(b.status);
        }
    }
    Ok(rules::is_ready(statuses))
}

fn label_names(r: &dyn Reader, task: &Task) -> Result<Vec<String>, EngineError> {
    let mut names = Vec::new();
    for id in &task.labels {
        if let Some(l) = r.label(*id)? {
            names.push(l.name);
        }
    }
    // Mismo orden que al leer del almacenamiento, recién creada o no.
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

fn progress(items: &[model::CheckItem]) -> String {
    let done = items.iter().filter(|i| i.done).count();
    format!("{done}/{}", items.len())
}

/// Nombre del agente que tiene la tarea tomada (claim registrado, vigente o no).
fn claim_holder(r: &dyn Reader, t: &Task) -> Result<Option<String>, EngineError> {
    t.claim
        .map(|c| actor_name(r, &model::Actor::Agent(c.agent)))
        .transpose()
}

/// Resumen compacto para listados.
pub fn task_summary(r: &dyn Reader, keys: &Keys, t: &Task) -> Result<Value, EngineError> {
    let parent = t.parent_id.map(|id| keys.of_id(r, id)).transpose()?;
    Ok(json!({
        "key": keys.of(t),
        "title": t.title,
        "status": t.status,
        "priority": t.priority,
        "parent": parent,
        "labels": label_names(r, t)?,
        "due": t.due,
        "ready": is_ready(r, t)?,
        "checklist": progress(&t.checklist),
        "claimed_by": claim_holder(r, t)?,
        "needs_human": t.needs_human.is_some(),
        "locked": t.locked,
        "stage": t.stage,
    }))
}

/// Detalle completo.
pub fn task_detail(r: &dyn Reader, keys: &Keys, t: &Task) -> Result<Value, EngineError> {
    let ids_to_keys = |ids: Vec<model::TaskId>| -> Result<Vec<String>, EngineError> {
        ids.into_iter().map(|id| keys.of_id(r, id)).collect()
    };
    let parent = t.parent_id.map(|id| keys.of_id(r, id)).transpose()?;
    let subtasks: Vec<String> = r
        .children(t.id)?
        .iter()
        .filter(|c| c.deleted_at.is_none())
        .map(|c| keys.of(c))
        .collect();
    // Lo que define el proyecto: etapa (D10), estimación y campos.
    let project = r.project(t.project_id)?;
    let stage = match (&t.stage, &project) {
        (Some(key), Some(p)) => rules::workflow(p)
            .into_iter()
            .find(|d| &d.key == key)
            .map(|d| json!({ "key": d.key, "name": d.name })),
        _ => None,
    };
    let estimate = match (t.estimate, &project) {
        (Some(e), Some(p)) => Some(json!({
            "value": rules::format_estimate(p.estimate_unit, e),
            "unit": p.estimate_unit,
        })),
        _ => None,
    };
    let fields: Vec<Value> = project
        .iter()
        .flat_map(|p| p.fields.iter())
        .filter_map(|f| {
            t.fields
                .get(&f.key)
                .map(|v| json!({ "key": f.key, "name": f.name, "kind": f.kind, "value": v }))
        })
        .collect();
    let group = |id: Option<model::EpicId>| -> Result<Option<Value>, EngineError> {
        Ok(match id {
            Some(id) => r
                .epic(id)?
                .map(|e| json!({ "id": e.id, "name": e.name, "kind": e.kind })),
            None => None,
        })
    };
    let attachments: Vec<Value> = r
        .attachments(t.id)?
        .iter()
        .map(|a| json!({ "id": a.id, "name": a.name, "size": a.size, "mime": a.mime }))
        .collect();
    let mut related = Vec::new();
    let mut duplicate_of = None;
    let mut duplicates = Vec::new();
    for rel in r.relations(Some(t.id))? {
        let other = if rel.a == t.id { rel.b } else { rel.a };
        let key = keys.of_id(r, other)?;
        match rel.kind {
            model::RelationKind::Relates => related.push(key),
            model::RelationKind::Duplicates if rel.a == t.id => duplicate_of = Some(key),
            model::RelationKind::Duplicates => duplicates.push(key),
        }
    }
    let git = r
        .git_links(Some(t.id))?
        .iter()
        .map(|l| git_link(r, keys, l))
        .collect::<Result<Vec<_>, _>>()?;
    let mut mentioned_by: Vec<model::TaskId> = Vec::new();
    for m in r.mentions_of(t.id)? {
        if !mentioned_by.contains(&m.source) {
            mentioned_by.push(m.source);
        }
    }
    let mut out = json!({
        "id": t.id,
        "key": keys.of(t),
        "parent": parent,
        "title": t.title,
        "description": t.description,
        "status": t.status,
        "priority": t.priority,
        "start": t.start,
        "due": t.due,
        "labels": label_names(r, t)?,
        "assignee": t.assignee(),
        "assignee_name": t.assignee().map(|a| actor_name(r, a)).transpose()?,
        // Todos los asignados, por nombre (D20).
        "assignees": t
            .assignees
            .iter()
            .map(|a| actor_name(r, a))
            .collect::<Result<Vec<_>, EngineError>>()?,
        "creator": t.creator,
        "creator_name": actor_name(r, &t.creator)?,
        "comments": r.comment_count(t.id)?,
        "checklist": t.checklist,
        "criteria": t.criteria,
        "ready": is_ready(r, t)?,
        "claim": t.claim.map(|c| json!({
            "agent": actor_name(r, &model::Actor::Agent(c.agent)).unwrap_or_default(),
            "since": c.since,
            "lease_until": c.until,
        })),
        "needs_human": t.needs_human,
        "locked": t.locked,
        "blocked_by": ids_to_keys(r.blockers_of(t.id)?)?,
        "blocks": ids_to_keys(r.blocks(t.id)?)?,
        "subtasks": subtasks,
        "stage": stage,
        "estimate": estimate,
        "epic": group(t.epic_id)?,
        "milestone": group(t.milestone_id)?,
        "fields": fields,
        "attachments": attachments,
        "related": related,
        "duplicate_of": duplicate_of,
        "duplicates": duplicates,
        "mentioned_by": ids_to_keys(mentioned_by)?,
        "revision": t.revision,
        "created_at": t.created_at,
        "updated_at": t.updated_at,
        "archived_at": t.archived_at,
        "deleted_at": t.deleted_at,
    });
    if let Some(obj) = out.as_object_mut() {
        obj.insert("git".into(), Value::Array(git));
        obj.insert("diagrams".into(), Value::Array(cited_diagrams(r, t.id)?));
        let mut cycles = Vec::new();
        for id in &t.cycles {
            if let Some(c) = r.cycle(*id)? {
                cycles.push(json!({ "id": c.id, "name": c.name, "closed": c.closed_at.is_some() }));
            }
        }
        obj.insert("cycles".into(), Value::Array(cycles));
        obj.insert(
            "recurrence".into(),
            serde_json::to_value(&t.recurrence).unwrap_or(Value::Null),
        );
    }
    Ok(out)
}

/// Una sesión cuenta como conectada si latió hace menos de esto.
pub const CONNECTED_WINDOW: jiff::SignedDuration = jiff::SignedDuration::from_secs(120);

/// Nombre legible de un actor: el nombre del agente, no su ULID.
pub fn actor_name(r: &dyn Reader, actor: &model::Actor) -> Result<String, EngineError> {
    use model::Actor;
    Ok(match actor {
        Actor::Supervisor => "supervisor".into(),
        Actor::System => "system".into(),
        Actor::Integration(name) => name.clone(),
        Actor::Agent(id) => match r.agent(*id)? {
            Some(a) => a.name,
            None => id.to_string(),
        },
        Actor::Interface(id) => match r.interface(*id)? {
            Some(i) => i.name,
            None => id.to_string(),
        },
    })
}

pub fn agent(
    r: &dyn Reader,
    a: &model::Agent,
    now: model::Timestamp,
) -> Result<Value, EngineError> {
    let sessions = r.open_sessions(Some(a.id))?;
    let last_seen = sessions.iter().map(|s| s.last_seen).max();
    let connected = last_seen.is_some_and(|seen| {
        now.duration_since(seen) <= CONNECTED_WINDOW && a.status == model::AgentStatus::Active
    });
    Ok(json!({
        "id": a.id,
        "name": a.name,
        "actor": model::Actor::Agent(a.id),
        "model": a.model,
        "description": a.description,
        "color": a.color,
        "status": a.status,
        "level": a.level,
        "scope": a
            .scope
            .iter()
            .map(|p| r.project(*p).map(|p| p.map(|p| p.key)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        "capabilities": a.capabilities,
        "paused": a.paused,
        "interface": a
            .interface_id
            .map(|i| r.interface(i))
            .transpose()?
            .flatten()
            .map(|i| i.name),
        "connected": connected,
        "last_seen": last_seen,
        "created_at": a.created_at,
    }))
}

/// Una interfaz (D15) con sus agentes; nunca el token.
pub fn interface(
    r: &dyn Reader,
    i: &model::Interface,
    now: model::Timestamp,
) -> Result<Value, EngineError> {
    let agents = r
        .agents()?
        .into_iter()
        .filter(|a| a.interface_id == Some(i.id))
        .map(|a| agent(r, &a, now))
        .collect::<Result<Vec<_>, _>>()?;
    let connected = agents.iter().any(|a| a["connected"] == true);
    Ok(json!({
        "id": i.id,
        "name": i.name,
        "kind": i.kind,
        "description": i.description,
        "color": i.color,
        "status": i.status,
        "level": i.level,
        "scope": i
            .scope
            .iter()
            .map(|p| r.project(*p).map(|p| p.map(|p| p.key)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        "max_agents": i.max_agents,
        "paused": i.paused,
        "connected": connected,
        "agents": agents,
        "created_at": i.created_at,
    }))
}

pub fn comment(r: &dyn Reader, keys: &Keys, c: &model::Comment) -> Result<Value, EngineError> {
    Ok(json!({
        "id": c.id,
        "task": keys.of_id(r, c.task_id)?,
        "author": c.author,
        "author_name": actor_name(r, &c.author)?,
        "body": c.body,
        "created_at": c.created_at,
    }))
}

/// Qué se cambió, en texto corto: clave de tarea, clave de proyecto, nombre…
pub fn activity_target(
    r: &dyn Reader,
    keys: &Keys,
    e: &model::ActivityEntry,
) -> Result<String, EngineError> {
    use model::EntityKind;
    let task_key = |id: Option<model::TaskId>| -> Result<String, EngineError> {
        id.map(|id| keys.of_id(r, id))
            .transpose()
            .map(Option::unwrap_or_default)
    };
    Ok(match e.entity {
        EntityKind::Task | EntityKind::Comment => task_key(e.task_id)?,
        EntityKind::Project => match e.project_id {
            Some(id) => r.project(id)?.map(|p| p.key).unwrap_or_default(),
            None => e.entity_id.clone(),
        },
        EntityKind::Dependency => {
            let (blocker, blocked) = e.entity_id.split_once('>').unwrap_or_default();
            let key = |s: &str| match s.parse() {
                Ok(id) => keys.of_id(r, id),
                Err(_) => Ok(s.to_owned()),
            };
            format!("{} -> {}", key(blocker)?, key(blocked)?)
        }
        EntityKind::Label => match e.entity_id.parse() {
            Ok(id) => r.label(id)?.map(|l| l.name).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Agent => match e.entity_id.parse() {
            Ok(id) => r.agent(id)?.map(|a| a.name).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Workspace => r.workspace()?.name,
        EntityKind::Inbox => match e.entity_id.parse() {
            Ok(id) => r.inbox_item(id)?.map(|i| i.title).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Epic => match e.entity_id.parse() {
            Ok(id) => r.epic(id)?.map(|x| x.name).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Cycle => match e.entity_id.parse() {
            Ok(id) => r.cycle(id)?.map(|x| x.name).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Diagram => match e.entity_id.parse() {
            Ok(id) => match r.diagram(id)? {
                Some(d) => diagram_key(r, &d)?,
                None => e.entity_id.clone(),
            },
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Template => match e.entity_id.parse() {
            Ok(id) => r.template(id)?.map(|x| x.name).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Attachment | EntityKind::Time | EntityKind::Cost | EntityKind::Git => {
            task_key(e.task_id)?
        }
        EntityKind::Interface => match e.entity_id.parse() {
            Ok(id) => r.interface(id)?.map(|i| i.name).unwrap_or_default(),
            Err(_) => e.entity_id.clone(),
        },
        EntityKind::Automation => {
            let id = e.entity_id.parse().ok();
            r.automations()?
                .into_iter()
                .find(|a| Some(a.id) == id)
                .map_or_else(|| e.entity_id.clone(), |a| a.name)
        }
        EntityKind::Webhook => {
            let id = e.entity_id.parse().ok();
            r.webhooks()?
                .into_iter()
                .find(|w| Some(w.id) == id)
                .map_or_else(|| e.entity_id.clone(), |w| w.url)
        }
        EntityKind::View => {
            let id = e.entity_id.parse().ok();
            r.views()?
                .into_iter()
                .find(|v| Some(v.id) == id)
                .map_or_else(|| e.entity_id.clone(), |v| v.name)
        }
        EntityKind::Relation => {
            let mut parts = e.entity_id.split('~');
            let key = |s: Option<&str>| match s.and_then(|s| s.parse().ok()) {
                Some(id) => keys.of_id(r, id),
                None => Ok(String::new()),
            };
            let a = key(parts.next())?;
            let kind = parts.next().unwrap_or_default().to_owned();
            let b = key(parts.next())?;
            format!("{a} ~{kind}~ {b}")
        }
    })
}

pub fn activity(
    r: &dyn Reader,
    keys: &Keys,
    e: &model::ActivityEntry,
) -> Result<Value, EngineError> {
    Ok(json!({
        "id": e.id,
        "at": e.at,
        "actor": e.actor,
        "actor_name": actor_name(r, &e.actor)?,
        "command": e.command,
        "action": e.action,
        "entity": e.entity,
        "target": activity_target(r, keys, e)?,
        "changes": e.changes,
    }))
}

/// Un vínculo con git (RF-INT-04): los commits llevan también su SHA corto.
pub fn git_link(r: &dyn Reader, keys: &Keys, l: &model::GitLink) -> Result<Value, EngineError> {
    let short = match l.kind {
        model::GitKind::Commit => l.reference.get(..7).unwrap_or(&l.reference),
        _ => l.reference.as_str(),
    };
    Ok(json!({
        "id": l.id,
        "task": keys.of_id(r, l.task_id)?,
        "kind": l.kind,
        "ref": l.reference,
        "short": short,
        "title": l.title,
        "url": l.url,
        "actor": actor_name(r, &l.actor)?,
        "at": l.at,
    }))
}
