use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{TemplateId, Timestamp};

/// Plantilla de tarea o de proyecto (RF-MOD-16). `body` es JSON: para una
/// tarea, los elementos de un `plan.create`; para un proyecto, además su
/// descripción, flujo, campos y unidad de estimación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub id: TemplateId,
    /// Único; un agente la instancia por nombre.
    pub name: String,
    pub kind: TemplateKind,
    pub description: String,
    pub body: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

text_enum! {
    pub enum TemplateKind ("error-parse-template-kind", "template-kind") {
        Task = "task" | "tarea",
        Project = "project" | "proyecto",
    }
}
