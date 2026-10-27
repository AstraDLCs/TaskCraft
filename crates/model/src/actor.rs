use std::fmt;
use std::str::FromStr;

use crate::serde_str::serde_as_str;

foundation::ulid_id! {
    /// Identidad estable de un agente registrado (RF-ACT-01).
    AgentId
}

foundation::ulid_id! {
    /// Cliente LLM que agrupa agentes y tiene el token (D15, RF-ACT-07).
    InterfaceId
}

serde_as_str!(Actor);

/// Quién origina un cambio. Todo cambio en los datos queda atribuido a uno.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Actor {
    /// El humano dueño del workspace.
    Supervisor,
    Agent(AgentId),
    /// Una interfaz que aún no eligió con qué agente actúa (D15): puede leer
    /// y crear sus agentes, nada más.
    Interface(InterfaceId),
    /// Script o servicio externo, identificado por nombre.
    Integration(String),
    /// TaskCraft mismo: expiración de leases, automatizaciones, recurrencias.
    System,
}

impl Actor {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent(_))
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Supervisor => f.write_str("supervisor"),
            Self::Agent(id) => write!(f, "agent:{id}"),
            Self::Interface(id) => write!(f, "interface:{id}"),
            Self::Integration(name) => write!(f, "integration:{name}"),
            Self::System => f.write_str("system"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseActorError(pub String);

impl i18n::Localize for ParseActorError {
    fn msg(&self) -> i18n::Msg {
        i18n::Msg::new("error-parse-actor").arg("value", &self.0)
    }
}

impl fmt::Display for ParseActorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use i18n::Localize;
        self.msg().fmt(f)
    }
}

impl std::error::Error for ParseActorError {}

impl FromStr for Actor {
    type Err = ParseActorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || ParseActorError(s.to_owned());
        match s.split_once(':') {
            None if s == "supervisor" => Ok(Self::Supervisor),
            None if s == "system" => Ok(Self::System),
            Some(("agent", id)) => id.parse().map(Self::Agent).map_err(|_| invalid()),
            Some(("interface", id)) => id.parse().map(Self::Interface).map_err(|_| invalid()),
            Some(("integration", name)) if !name.is_empty() => {
                Ok(Self::Integration(name.to_owned()))
            }
            _ => Err(invalid()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_text() {
        for actor in [
            Actor::Supervisor,
            Actor::System,
            Actor::Agent(AgentId::generate()),
            Actor::Interface(InterfaceId::generate()),
            Actor::Integration("n8n".into()),
        ] {
            assert_eq!(actor.to_string().parse::<Actor>().unwrap(), actor);
        }
    }

    #[test]
    fn rejects_unknown() {
        assert!("robot".parse::<Actor>().is_err());
        assert!("agent:xyz".parse::<Actor>().is_err());
        assert!("integration:".parse::<Actor>().is_err());
    }
}
