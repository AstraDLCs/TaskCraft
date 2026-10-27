use foundation::CorrelationId;
use model::Actor;

/// Quién ejecuta un comando y con qué ID de correlación (RNF-13).
///
/// Cada frontend crea uno por petición; los logs, eventos e historial del
/// comando quedan atribuidos a este actor y agrupados por la correlación.
#[derive(Debug, Clone)]
pub struct Context {
    pub actor: Actor,
    pub correlation: CorrelationId,
}

impl Context {
    pub fn new(actor: Actor) -> Self {
        Self {
            actor,
            correlation: CorrelationId::generate(),
        }
    }

    pub fn supervisor() -> Self {
        Self::new(Actor::Supervisor)
    }
}
