use serde::{Deserialize, Serialize};

use crate::LabelId;

/// Etiqueta transversal, compartida entre proyectos del workspace (RF-MOD-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: LabelId,
    pub name: String,
    /// `#rrggbb`
    pub color: String,
}
