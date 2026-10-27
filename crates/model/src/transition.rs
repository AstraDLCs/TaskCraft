use serde::{Deserialize, Serialize};

use crate::{Actor, ProjectId, Status, TaskId, Timestamp, TransitionId};

/// Un paso del recorrido de una tarea (RF-HIS-09, D24): de qué estado y
/// columna a cuáles, quién la movió y cuándo. Se guarda aparte del historial
/// para que las métricas sobrevivan a `activity.prune` (D23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub id: TransitionId,
    pub task_id: TaskId,
    pub project_id: ProjectId,
    /// `None` en el paso en que se creó. Si se reconstruyó de un historial
    /// podado (D24), el primer paso conocido puede venir de otro estado.
    pub from_status: Option<Status>,
    pub from_stage: Option<String>,
    pub to_status: Status,
    /// Columna propia del flujo (D10), si la hay.
    pub to_stage: Option<String>,
    pub actor: Actor,
    pub at: Timestamp,
}

impl Transition {
    /// Un paso nuevo de `task`: de `from` (`None` = se creó así) a `to`,
    /// estado y columna.
    pub fn new(
        task: TaskId,
        project: ProjectId,
        from: Option<(Status, Option<String>)>,
        to: (Status, Option<String>),
        actor: Actor,
        at: Timestamp,
    ) -> Self {
        let (from_status, from_stage) = match from {
            Some((status, stage)) => (Some(status), stage),
            None => (None, None),
        };
        Self {
            id: TransitionId::generate(),
            task_id: task,
            project_id: project,
            from_status,
            from_stage,
            to_status: to.0,
            to_stage: to.1,
            actor,
            at,
        }
    }
}
