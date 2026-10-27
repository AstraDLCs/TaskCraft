use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::{CycleId, ProjectId, Timestamp};

/// Ciclo o sprint de un proyecto (RF-INT-05): un periodo con fecha de inicio
/// y de fin. Al cerrarlo, lo incompleto pasa al siguiente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub id: CycleId,
    pub project_id: ProjectId,
    pub name: String,
    pub start: Date,
    /// Último día del ciclo (incluido).
    pub end: Date,
    pub closed_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// En qué punto está un ciclo respecto a hoy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleState {
    Planned,
    Active,
    /// Ya terminó su fecha, pero aún no se cerró.
    Overdue,
    Closed,
}

impl Cycle {
    pub fn state(&self, today: Date) -> CycleState {
        if self.closed_at.is_some() {
            CycleState::Closed
        } else if today < self.start {
            CycleState::Planned
        } else if today <= self.end {
            CycleState::Active
        } else {
            CycleState::Overdue
        }
    }
}
