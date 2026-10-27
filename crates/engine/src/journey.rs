//! Recorrido de las tareas (D24): lo que aún no lo tiene se reconstruye del
//! historial una sola vez, y lo que una importación dejó a medias se cierra.

use std::collections::{BTreeSet, HashMap};

use model::{Actor, EntityKind, Status, Task, TaskId, Timestamp, Transition};
use rules::metrics::Step;
use store::{ActivityFilter, StoreResult, TaskFilter, TransitionFilter, Writer};

/// Todas las tareas, también las archivadas y las de la papelera, en dos
/// lecturas.
fn all_tasks(tx: &dyn Writer) -> StoreResult<HashMap<TaskId, Task>> {
    let live = TaskFilter {
        include_archived: true,
        ..TaskFilter::default()
    };
    let trash = TaskFilter {
        include_archived: true,
        deleted: true,
        ..TaskFilter::default()
    };
    Ok(tx
        .tasks(&live)?
        .into_iter()
        .chain(tx.tasks(&trash)?)
        .map(|t| (t.id, t))
        .collect())
}

fn now_of(task: &Task) -> (Status, Option<String>) {
    (task.status, task.stage.clone())
}

/// Recorrido de las tareas que no lo tienen: las de antes de D24 o
/// importadas de un archivo anterior. Sale del historial; si ya no está
/// (podado), un solo paso con su estado actual desde que se creó. Si el
/// historial no llega al estado actual, un último paso lo cierra. Devuelve
/// cuántos pasos escribió.
pub(crate) fn backfill(tx: &mut dyn Writer) -> StoreResult<usize> {
    let missing: BTreeSet<TaskId> = tx.tasks_without_transitions()?.into_iter().collect();
    if missing.is_empty() {
        return Ok(0);
    }
    let tasks = all_tasks(tx)?;
    let entries: Vec<_> = tx
        .activity(&ActivityFilter {
            entity: Some(EntityKind::Task),
            limit: usize::MAX >> 1,
            ..ActivityFilter::default()
        })?
        .into_iter()
        .filter(|e| e.task_id.is_some_and(|t| missing.contains(&t)))
        .collect();
    let mut by_task: HashMap<TaskId, Vec<Step>> = HashMap::new();
    for step in rules::metrics::journey(&entries) {
        by_task.entry(step.task).or_default().push(step);
    }
    let mut written = 0;
    for id in missing {
        let Some(task) = tasks.get(&id) else { continue };
        let steps = by_task.remove(&id).unwrap_or_default();
        for transition in rebuild(task, steps) {
            tx.insert_transition(&transition)?;
            written += 1;
        }
    }
    Ok(written)
}

/// Tras importar o fusionar: el recorrido de una tarea cuyo estado cambió
/// sin pasar por el engine (la fusión actualiza tareas que ya existían) se
/// cierra con un paso hasta su estado actual.
pub(crate) fn reconcile(tx: &mut dyn Writer) -> StoreResult<usize> {
    let mut last: HashMap<TaskId, ((Status, Option<String>), Timestamp)> = HashMap::new();
    // Del más antiguo al más reciente: queda el último de cada tarea.
    for t in tx.transitions(&TransitionFilter::default())? {
        last.insert(t.task_id, ((t.to_status, t.to_stage), t.at));
    }
    let mut written = 0;
    for task in all_tasks(tx)?.values() {
        if let Some((reached, at)) = last.get(&task.id)
            && *reached != now_of(task)
        {
            tx.insert_transition(&closing(task, reached.clone(), *at))?;
            written += 1;
        }
    }
    Ok(written)
}

/// El paso que lleva a la tarea de `reached` a su estado actual, sin saber
/// quién ni cuándo exactamente: el sistema, en su última modificación. Nunca
/// antes que `after` (el último paso): con relojes desfasados, el cierre
/// quedaría detrás y cada importación añadiría otro.
fn closing(task: &Task, reached: (Status, Option<String>), after: Timestamp) -> Transition {
    Transition::new(
        task.id,
        task.project_id,
        Some(reached),
        now_of(task),
        Actor::System,
        task.updated_at.max(after),
    )
}

fn rebuild(task: &Task, steps: Vec<Step>) -> Vec<Transition> {
    let reached = steps.last().map(|s| (s.to.clone(), s.at));
    let mut out: Vec<Transition> = steps
        .into_iter()
        .map(|s| Transition::new(task.id, task.project_id, s.from, s.to, s.actor, s.at))
        .collect();
    match reached {
        None => out.push(Transition::new(
            task.id,
            task.project_id,
            None,
            now_of(task),
            task.creator.clone(),
            task.created_at,
        )),
        Some((last, at)) if last != now_of(task) => out.push(closing(task, last, at)),
        Some(_) => {}
    }
    out
}
