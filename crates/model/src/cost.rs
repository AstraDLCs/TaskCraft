use serde::{Deserialize, Serialize};

use crate::{Actor, CostEntryId, TaskId, Timestamp};

/// Consumo que un agente reporta por su trabajo en una tarea (RF-HIS-06):
/// tokens y lo que costaron.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostEntry {
    pub id: CostEntryId,
    pub task_id: TaskId,
    pub actor: Actor,
    /// Modelo que hizo el trabajo (`claude-opus-5-5`…); vacío si no se dice.
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// Importe en millonésimas de `currency`, para sumar sin redondeos.
    pub amount_micros: u64,
    /// Código ISO 4217 en mayúsculas (`USD`, `EUR`…).
    pub currency: String,
    pub note: String,
    pub at: Timestamp,
}
