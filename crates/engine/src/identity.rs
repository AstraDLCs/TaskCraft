//! Quién actúa (D15): el token identifica a una interfaz; la interfaz actúa
//! como uno de sus agentes, que puede cambiar en cualquier momento. Lo usan
//! el daemon, la CLI, el MCP y la API web, así que la regla es una sola.

use i18n::Msg;
use model::{Actor, Agent, AgentStatus, Interface, ProjectId};
use store::Reader;

use crate::{EngineError, lookup};

/// La interfaz dueña de un token; solo si está activa.
pub fn interface_by_token(r: &dyn Reader, token: &str) -> Result<Interface, EngineError> {
    let hash = foundation::secret::hash_token(token);
    match r.interface_by_token(&hash)? {
        Some(i) if i.status == AgentStatus::Active => Ok(i),
        _ => Err(EngineError::forbidden(Msg::new("agent-bad-token"))),
    }
}

/// Sus agentes activos, por nombre.
pub fn agents_of(r: &dyn Reader, interface: &Interface) -> Result<Vec<Agent>, EngineError> {
    Ok(r.agents()?
        .into_iter()
        .filter(|a| a.interface_id == Some(interface.id) && a.status == AgentStatus::Active)
        .collect())
}

/// Con un solo agente activo se actúa como él; si no, como la interfaz,
/// que debe elegir uno (`agent_use`) antes de escribir.
pub fn default_actor(r: &dyn Reader, interface: &Interface) -> Result<Actor, EngineError> {
    let agents = agents_of(r, interface)?;
    Ok(match agents.as_slice() {
        [only] => Actor::Agent(only.id),
        _ => Actor::Interface(interface.id),
    })
}

/// Actor de un token: la interfaz o su único agente.
pub fn from_token(r: &dyn Reader, token: &str) -> Result<Actor, EngineError> {
    default_actor(r, &interface_by_token(r, token)?)
}

/// Interfaz a la que pertenece quien actúa, si la tiene.
pub fn interface_of(r: &dyn Reader, actor: &Actor) -> Result<Option<Interface>, EngineError> {
    let id = match actor {
        Actor::Interface(id) => Some(*id),
        Actor::Agent(a) => r.agent(*a)?.and_then(|a| a.interface_id),
        _ => None,
    };
    Ok(id.map(|id| r.interface(id)).transpose()?.flatten())
}

/// Cambia de identidad (RF-MCP-11): `base` pasa a actuar como el agente
/// `name`. Una interfaz (o uno de sus agentes) solo puede usar agentes
/// propios; el supervisor, cualquiera (confianza local).
pub fn act_as(r: &dyn Reader, base: &Actor, name: &str) -> Result<Actor, EngineError> {
    let agent = lookup::agent(r, name.trim())?;
    if agent.status != AgentStatus::Active {
        return Err(EngineError::forbidden(
            Msg::new("agent-disabled").arg("agent", &agent.name),
        ));
    }
    let allowed = match base {
        Actor::Supervisor => true,
        Actor::Agent(id) if *id == agent.id => true,
        other => match interface_of(r, other)? {
            Some(i) => agent.interface_id == Some(i.id),
            None => false,
        },
    };
    if allowed {
        Ok(Actor::Agent(agent.id))
    } else {
        Err(EngineError::forbidden(
            Msg::new("agent-other-interface").arg("agent", &agent.name),
        ))
    }
}

/// Alcance efectivo de un agente (D11 + D15): la intersección del suyo con
/// el de su interfaz. `None` = todos los proyectos.
pub fn scope(r: &dyn Reader, agent: &Agent) -> Result<Option<Vec<ProjectId>>, EngineError> {
    let own = (!agent.scope.is_empty()).then(|| agent.scope.clone());
    let theirs = agent
        .interface_id
        .map(|i| r.interface(i))
        .transpose()?
        .flatten()
        .filter(|i| !i.scope.is_empty())
        .map(|i| i.scope);
    Ok(match (own, theirs) {
        (None, None) => None,
        (Some(s), None) | (None, Some(s)) => Some(s),
        (Some(a), Some(b)) => Some(a.into_iter().filter(|p| b.contains(p)).collect()),
    })
}
