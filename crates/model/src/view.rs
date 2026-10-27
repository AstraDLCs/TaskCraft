use serde::{Deserialize, Serialize};

use crate::{ProjectId, Timestamp, ViewId};

/// Vista guardada (RF-VIS-12): un filtro con nombre (D12), el modo en que
/// se muestra y, para la tabla, sus columnas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedView {
    pub id: ViewId,
    /// Único, sin distinguir mayúsculas.
    pub name: String,
    pub filter: String,
    /// `list`, `board`, `table`, `today` o `gantt`; libre para la app.
    pub layout: String,
    pub columns: Vec<String>,
    /// Proyecto al que se limita; `None` = todo el workspace.
    pub project_id: Option<ProjectId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
