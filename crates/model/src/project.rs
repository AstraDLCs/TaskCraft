use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{EstimateUnit, FieldDef, ProjectId, StatusDef, Timestamp};

/// Contenedor de primer nivel (RF-MOD-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    /// Clave corta en mayúsculas (`API`); prefijo de las claves de tarea.
    pub key: String,
    pub name: String,
    /// Markdown (fuente).
    pub description: String,
    pub status: ProjectStatus,
    /// Número que recibirá la próxima tarea (`API-<n>`).
    pub next_number: u64,
    /// Número que recibirá el próximo diagrama (`API-D<n>`, RF-MOD-19).
    #[serde(default = "first_number")]
    pub next_diagram: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// Bloqueo humano de todo el proyecto (RF-SUP-05).
    pub locked: bool,
    /// Columnas del flujo en orden (D10); vacío = el flujo por defecto.
    #[serde(default)]
    pub workflow: Vec<StatusDef>,
    #[serde(default = "default_unit")]
    pub estimate_unit: EstimateUnit,
    /// Campos personalizados (RF-MOD-15).
    #[serde(default)]
    pub fields: Vec<FieldDef>,
}

fn first_number() -> u64 {
    1
}

fn default_unit() -> EstimateUnit {
    EstimateUnit::Hours
}

text_enum! {
    pub enum ProjectStatus ("error-parse-project-status", "project-status") {
        Active = "active" | "activo",
        Archived = "archived" | "archivado",
    }
}
