use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{AgentId, InterfaceId, ProjectId, SessionId, Timestamp};

/// Agente de IA (RF-ACT-01). No tiene token: actúa a través de su interfaz (D15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    /// Único, en minúsculas: con él se identifica (`--agent claude-1`).
    pub name: String,
    /// Texto libre: `claude-opus-5-5`, `cursor`, `script propio`…
    pub model: String,
    pub description: String,
    /// `#rrggbb`; también es la semilla del avatar.
    pub color: String,
    pub status: AgentStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// Interfaz a la que pertenece (D15); su token es el que lo autentica.
    #[serde(default)]
    pub interface_id: Option<InterfaceId>,
    /// Hasta dónde puede actuar (D11, RF-ACT-05).
    #[serde(default = "default_level")]
    pub level: AgentLevel,
    /// Proyectos en los que puede escribir; vacío = todos.
    #[serde(default)]
    pub scope: Vec<ProjectId>,
    /// Etiquetas de lo que sabe hacer; filtran `task.next` (RF-ACT-04).
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Pausado por el supervisor: puede leer pero no escribir (RF-SUP-08).
    #[serde(default)]
    pub paused: bool,
}

fn default_level() -> AgentLevel {
    AgentLevel::Plan
}

text_enum! {
    /// Nivel de permisos (D11), de menos a más.
    pub enum AgentLevel ("error-parse-agent-level", "agent-level") {
        Read = "read" | "lectura",
        Execute = "execute" | "ejecutar",
        Plan = "plan" | "planificar",
        Admin = "admin" | "administrar",
    }
}

impl AgentLevel {
    /// 0 = solo lectura; para comparar niveles.
    pub fn rank(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Execute => 1,
            Self::Plan => 2,
            Self::Admin => 3,
        }
    }
}

text_enum! {
    /// RF-ACT-06: desactivar es reversible; revocar es definitivo. Ninguno
    /// borra el historial del agente.
    pub enum AgentStatus ("error-parse-agent-status", "agent-status") {
        Active = "active" | "activo",
        Disabled = "disabled" | "desactivado",
        Revoked = "revoked" | "revocado",
    }
}

/// Conexión de un agente (RF-ACT-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: SessionId,
    pub agent_id: AgentId,
    /// `cli`, `stdio`, `http`…
    pub transport: String,
    pub started_at: Timestamp,
    pub last_seen: Timestamp,
    pub ended_at: Option<Timestamp>,
}

/// Cliente LLM (Claude Code, Cursor…) registrado por el supervisor (D15,
/// RF-ACT-07). Tiene el único token; sus agentes actúan con él.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interface {
    pub id: InterfaceId,
    /// Único, en minúsculas.
    pub name: String,
    /// Texto libre: `claude-code`, `cursor`, `antigravity`…
    pub kind: String,
    pub description: String,
    pub color: String,
    pub status: AgentStatus,
    /// Nivel máximo de sus agentes (D11).
    #[serde(default = "default_level")]
    pub level: AgentLevel,
    /// Proyectos a los que pueden llegar sus agentes; vacío = todos.
    #[serde(default)]
    pub scope: Vec<ProjectId>,
    /// Cuántos agentes puede crear.
    #[serde(default = "default_max_agents")]
    pub max_agents: u32,
    /// Pausa a todos sus agentes (RF-SUP-08).
    #[serde(default)]
    pub paused: bool,
    /// SHA-256 de su token (D09). Nunca sale en vistas, historial ni exportaciones.
    #[serde(skip, default)]
    pub token_hash: Option<String>,
    /// El token en claro, para copiarlo desde la app (D22). Como el hash,
    /// nunca sale en vistas, historial ni exportaciones. `None` en las
    /// interfaces creadas antes de D22: hay que generar uno nuevo.
    #[serde(skip, default)]
    pub token: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

fn default_max_agents() -> u32 {
    50
}
