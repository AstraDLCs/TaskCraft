use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{Actor, TaskId, TimeEntryId, Timestamp};

/// Tiempo registrado a mano o con el cronómetro (RF-HIS-04). El tiempo que
/// los agentes pasan con una tarea tomada se calcula del historial (RF-HIS-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeEntry {
    pub id: TimeEntryId,
    pub task_id: TaskId,
    pub actor: Actor,
    pub source: TimeSource,
    /// Inicio del cronómetro, o el día al que se imputa un registro manual.
    pub started_at: Timestamp,
    /// Duración; 0 mientras el cronómetro sigue en marcha.
    pub minutes: u32,
    pub running: bool,
    pub note: String,
    pub created_at: Timestamp,
}

text_enum! {
    pub enum TimeSource ("error-parse-time-source", "time-source") {
        Timer = "timer" | "cronometro" | "cronómetro",
        Manual = "manual",
    }
}
