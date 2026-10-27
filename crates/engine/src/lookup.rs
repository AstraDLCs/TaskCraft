//! Resolución de referencias (`API`, `API-42` o ULID) y recorridos del árbol.

use std::collections::HashMap;

use i18n::Msg;
use model::{Project, ProjectId, Task, TaskId, TaskKey};
use store::Reader;

use crate::EngineError;

pub fn project(r: &dyn Reader, reference: &str) -> Result<Project, EngineError> {
    let found = match reference.parse::<ProjectId>() {
        Ok(id) => r.project(id)?,
        Err(_) => r.project_by_key(reference)?,
    };
    found.ok_or_else(|| {
        EngineError::not_found(Msg::new("notfound-project").arg("project", reference))
    })
}

/// Agente dueño de un token (D09). Un token desconocido o de un agente que no
/// está activo es `FORBIDDEN`, sin decir cuál de las dos cosas pasó.
/// Épica o hito por ULID o por nombre (sin distinguir mayúsculas), dentro
/// de un proyecto si se indica.
pub fn epic(
    r: &dyn Reader,
    reference: &str,
    project: Option<ProjectId>,
    kind: Option<model::EpicKind>,
) -> Result<model::Epic, EngineError> {
    let not_found = || EngineError::not_found(Msg::new("notfound-epic").arg("epic", reference));
    if let Ok(id) = reference.parse::<model::EpicId>() {
        return r
            .epic(id)?
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .ok_or_else(not_found);
    }
    let wanted = reference.trim().to_lowercase();
    let mut found: Vec<model::Epic> = r
        .epics(project)?
        .into_iter()
        .filter(|e| e.name.to_lowercase() == wanted && kind.is_none_or(|k| e.kind == k))
        .collect();
    match found.len() {
        0 => Err(not_found()),
        1 => Ok(found.remove(0)),
        _ => Err(EngineError::invalid(
            Msg::new("epic-ambiguous").arg("epic", reference),
        )),
    }
}

/// Diagrama por ULID o clave (`API-D3`). Con `project`, también por número
/// (`D3` o `3`) dentro de ese proyecto (RF-MOD-19).
pub fn diagram(
    r: &dyn Reader,
    reference: &str,
    project: Option<ProjectId>,
) -> Result<model::Diagram, EngineError> {
    let not_found =
        || EngineError::not_found(Msg::new("notfound-diagram").arg("diagram", reference));
    let reference = reference.trim();
    if let Ok(id) = reference.parse::<model::DiagramId>() {
        return r.diagram(id)?.ok_or_else(not_found);
    }
    if let Some(key) = model::DiagramKey::parse(reference) {
        return match r.project_by_key(&key.project)? {
            Some(p) => r.diagram_by_number(p.id, key.number)?.ok_or_else(not_found),
            None => Err(not_found()),
        };
    }
    let number = reference
        .strip_prefix(['D', 'd'])
        .unwrap_or(reference)
        .parse::<u64>();
    match (project, number) {
        (Some(p), Ok(n)) => r.diagram_by_number(p, n)?.ok_or_else(not_found),
        _ => Err(EngineError::arg(
            "diagram",
            Msg::new("arg-bad-diagram-ref").arg("value", reference),
        )),
    }
}

/// Tarea por clave o ULID, incluida la papelera.
pub fn task_any(r: &dyn Reader, reference: &str) -> Result<Task, EngineError> {
    let found = if let Ok(id) = reference.parse::<TaskId>() {
        r.task(id)?
    } else if let Some(key) = TaskKey::parse(reference) {
        match r.project_by_key(&key.project)? {
            Some(p) => r.task_by_number(p.id, key.number)?,
            None => None,
        }
    } else {
        return Err(EngineError::arg(
            "task",
            Msg::new("arg-bad-task-ref").arg("value", reference),
        ));
    };
    found.ok_or_else(|| EngineError::not_found(Msg::new("notfound-task").arg("task", reference)))
}

/// Tarea por clave o ULID, excluyendo la papelera.
pub fn task(r: &dyn Reader, reference: &str) -> Result<Task, EngineError> {
    let task = task_any(r, reference)?;
    if task.deleted_at.is_some() {
        return Err(EngineError::not_found(
            Msg::new("task-in-trash").arg("task", reference),
        ));
    }
    Ok(task)
}

/// Tarea que se va a modificar: ni borrada ni archivada.
pub fn editable_task(r: &dyn Reader, reference: &str) -> Result<Task, EngineError> {
    let task = task(r, reference)?;
    if task.archived_at.is_some() {
        return Err(EngineError::invalid(
            Msg::new("task-archived").arg("task", reference),
        ));
    }
    Ok(task)
}

/// La tarea y todos sus descendientes (preorden), incluidos archivados y borrados.
pub fn subtree(r: &dyn Reader, root: Task) -> Result<Vec<Task>, EngineError> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(task) = stack.pop() {
        let mut children = r.children(task.id)?;
        children.reverse();
        stack.extend(children);
        out.push(task);
    }
    Ok(out)
}

/// Ancestros de la tarea, del padre a la raíz.
pub fn ancestors(r: &dyn Reader, task: &Task) -> Result<Vec<Task>, EngineError> {
    let mut out: Vec<Task> = Vec::new();
    let mut next = task.parent_id;
    while let Some(id) = next {
        if out.iter().any(|t| t.id == id) {
            return Err(EngineError::invalid(Msg::new("hierarchy-corrupt")));
        }
        let parent = r.task(id)?.ok_or_else(|| {
            EngineError::not_found(Msg::new("notfound-parent").arg("id", id.to_string()))
        })?;
        next = parent.parent_id;
        out.push(parent);
    }
    Ok(out)
}

/// Claves de proyecto por ID, para construir claves de tarea.
pub struct Keys(HashMap<ProjectId, String>);

impl Keys {
    pub fn load(r: &dyn Reader) -> Result<Self, EngineError> {
        Ok(Self(
            r.projects(true)?
                .into_iter()
                .map(|p| (p.id, p.key))
                .collect(),
        ))
    }

    pub fn of(&self, task: &Task) -> String {
        let project = self
            .0
            .get(&task.project_id)
            .map(String::as_str)
            .unwrap_or("?");
        TaskKey {
            project: project.to_owned(),
            number: task.number,
        }
        .to_string()
    }

    pub fn of_id(&self, r: &dyn Reader, id: TaskId) -> Result<String, EngineError> {
        Ok(match r.task(id)? {
            Some(t) => self.of(&t),
            None => id.to_string(),
        })
    }
}

/// Agente por nombre o ULID, en cualquier estado.
pub fn agent(r: &dyn Reader, reference: &str) -> Result<model::Agent, EngineError> {
    let found = match reference.parse::<model::AgentId>() {
        Ok(id) => r.agent(id)?,
        Err(_) => r.agent_by_name(reference)?,
    };
    found.ok_or_else(|| EngineError::not_found(Msg::new("notfound-agent").arg("agent", reference)))
}
