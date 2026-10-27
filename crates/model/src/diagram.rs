use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{CommentId, DiagramId, ProjectId, TaskId, Timestamp};

/// Diagrama Mermaid de un proyecto (RF-MOD-19, D21). Las tareas lo citan con
/// `[[API-D3]]` (enlace) o `![[API-D3]]` (incrustado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagram {
    pub id: DiagramId,
    pub project_id: ProjectId,
    /// Número dentro del proyecto: `API-D<n>`.
    pub number: u64,
    pub title: String,
    /// Markdown (fuente).
    pub description: String,
    /// Fuente Mermaid.
    pub source: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Clave legible de un diagrama: `API-D3`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiagramKey {
    pub project: String,
    pub number: u64,
}

impl DiagramKey {
    /// `api-d3` → (`API`, 3). Devuelve `None` si no tiene esa forma.
    pub fn parse(text: &str) -> Option<Self> {
        let (project, number) = text.rsplit_once('-')?;
        let number = number
            .strip_prefix('D')
            .or_else(|| number.strip_prefix('d'))?;
        if project.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self {
            project: project.to_uppercase(),
            number: number.parse().ok()?,
        })
    }
}

impl fmt::Display for DiagramKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-D{}", self.project, self.number)
    }
}

/// `[[API-D3]]` o `![[API-D3]]` en la descripción de `source` o en uno de sus
/// comentarios cita el diagrama `target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiagramMention {
    pub source: TaskId,
    pub comment: Option<CommentId>,
    pub target: DiagramId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagram_keys() {
        let key = DiagramKey::parse("api-d3").expect("clave");
        assert_eq!((key.project.as_str(), key.number), ("API", 3));
        assert_eq!(key.to_string(), "API-D3");
        assert_eq!(
            DiagramKey::parse("A-B-D12").map(|k| k.to_string()),
            Some("A-B-D12".into())
        );
        for bad in ["API-42", "API-D", "-D3", "API-D+3", "APID3", "API-Dx"] {
            assert_eq!(DiagramKey::parse(bad), None, "{bad}");
        }
    }
}
