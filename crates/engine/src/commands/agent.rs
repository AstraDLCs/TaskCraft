//! Agentes (RF-ACT-01) y su ciclo de vida (RF-ACT-06). Siempre pertenecen a
//! una interfaz, que es la única con token (D15, D22): la interfaz crea los
//! suyos (`agent.create`) y el supervisor los añade (`agent.register
//! interface=`) y administra. Desactivar o revocar no borra su historial.

use i18n::Msg;
use model::{Actor, Agent, AgentId, AgentLevel, AgentStatus, Interface, InterfaceId};
use serde_json::{Value, json};
use store::Reader;

use super::paged;
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup, views};

command!(Register, "agent.register", "cmd-agent-register", register, write, [req "name": K::Text, req "interface": K::Text, opt "model": K::Text, opt "description": K::Text, opt "color": K::Text, opt "capabilities": K::List, opt "level": K::Enum(AgentLevel::NAMES), opt "scope": K::List]);
command!(Create, "agent.create", "cmd-agent-create", create, write, [req "name": K::Text, opt "model": K::Text, opt "description": K::Text, opt "color": K::Text, opt "capabilities": K::List, opt "level": K::Enum(AgentLevel::NAMES), opt "scope": K::List, opt "interface": K::Text]);
command!(Mine, "agent.mine", "cmd-agent-mine", mine, read, []);
command!(List, "agent.list", "cmd-agent-list", list_agents, read, [opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Get, "agent.get", "cmd-agent-get", get, read, [req "agent": K::Text]);
command!(Update, "agent.update", "cmd-agent-update", update, write, [req "agent": K::Text, opt "interface": K::Text, opt "model": K::Text, opt "description": K::Text, opt "color": K::Text, opt "level": K::Enum(model::AgentLevel::NAMES), opt "scope": K::List, opt "capabilities": K::List]);
command!(Pause, "agent.pause", "cmd-agent-pause", pause, write, [req "agent": K::Text]);
command!(Resume, "agent.resume", "cmd-agent-resume", resume, write, [req "agent": K::Text]);
command!(Disable, "agent.disable", "cmd-agent-disable", disable, write, [req "agent": K::Text]);
command!(Enable, "agent.enable", "cmd-agent-enable", enable, write, [req "agent": K::Text]);
command!(Revoke, "agent.revoke", "cmd-agent-revoke", revoke, write, [req "agent": K::Text]);

pub(super) fn supervisor_only(cx: &Cx<'_>) -> Result<(), EngineError> {
    match cx.ctx.actor {
        Actor::Supervisor | Actor::System => Ok(()),
        _ => Err(EngineError::forbidden(Msg::new("supervisor-only"))),
    }
}

fn view(cx: &Cx<'_>, agent: &Agent) -> Result<Value, EngineError> {
    views::agent(&*cx.tx, agent, cx.now)
}

/// Un agente nuevo, activo, en `interface`.
fn new_agent(
    cx: &Cx<'_>,
    args: &Args,
    interface: Option<InterfaceId>,
) -> Result<Agent, EngineError> {
    let name = rules::agent_name(args.require("name")?)?;
    if cx.tx.agent_by_name(&name)?.is_some() {
        return Err(EngineError::invalid(
            Msg::new("agent-name-taken").arg("name", &name),
        ));
    }
    let color = match args.get("color") {
        Some(c) => rules::label_color(c)?,
        None => rules::default_label_color(&name),
    };
    Ok(Agent {
        id: AgentId::generate(),
        model: args.get("model").unwrap_or_default().trim().to_owned(),
        description: args.get("description").unwrap_or_default().to_owned(),
        color,
        name,
        status: AgentStatus::Active,
        created_at: cx.now,
        updated_at: cx.now,
        interface_id: interface,
        level: AgentLevel::Plan,
        scope: Vec::new(),
        capabilities: Vec::new(),
        paused: false,
    })
}

/// Supervisor: un agente más de `interface` (sin token; el token es de la
/// interfaz, D22). Mismos límites que `agent.create`.
fn register(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let interface = super::interface::find(&*cx.tx, args.require("interface")?)?;
    create_in(cx, args, &interface)
}

/// La interfaz de quien llama: la propia o, para el supervisor, `interface=`.
fn caller_interface(cx: &Cx<'_>, args: &Args) -> Result<Interface, EngineError> {
    if let Some(i) = crate::identity::interface_of(&*cx.tx, &cx.ctx.actor)? {
        return Ok(i);
    }
    match (&cx.ctx.actor, args.get("interface")) {
        (Actor::Supervisor | Actor::System, Some(reference)) => {
            super::interface::find(&*cx.tx, reference)
        }
        (Actor::Supervisor | Actor::System, None) => {
            Err(EngineError::MissingArg("interface".into()))
        }
        _ => Err(EngineError::forbidden(Msg::new("interface-only"))),
    }
}

/// Una interfaz crea sus agentes (D15, RF-ACT-07): sin token, con como
/// mucho su nivel y hasta su máximo de agentes.
fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let interface = caller_interface(cx, args)?;
    create_in(cx, args, &interface)
}

/// ¿Le cabe otro agente a la interfaz?
fn check_room(cx: &Cx<'_>, interface: &Interface) -> Result<(), EngineError> {
    let mine = crate::identity::agents_of(&*cx.tx, interface)?.len();
    if mine >= usize::try_from(interface.max_agents).unwrap_or(usize::MAX) {
        return Err(EngineError::invalid(
            Msg::new("interface-max-agents")
                .arg("interface", &interface.name)
                .arg("max", interface.max_agents),
        ));
    }
    Ok(())
}

/// Como mucho el nivel de la interfaz.
fn capped(wanted: AgentLevel, interface: &Interface) -> AgentLevel {
    if wanted.rank() > interface.level.rank() {
        interface.level
    } else {
        wanted
    }
}

fn create_in(cx: &mut Cx<'_>, args: &Args, interface: &Interface) -> Result<Value, EngineError> {
    check_room(cx, interface)?;
    let mut agent = new_agent(cx, args, Some(interface.id))?;
    let wanted: AgentLevel = args.parse("level")?.unwrap_or(AgentLevel::Plan);
    agent.level = capped(wanted, interface);
    for key in args.list("scope") {
        let id = lookup::project(&*cx.tx, &key)?.id;
        if !agent.scope.contains(&id) {
            agent.scope.push(id);
        }
    }
    for c in args.list("capabilities") {
        let name = rules::label_name(&c)?.to_lowercase();
        if !agent.capabilities.contains(&name) {
            agent.capabilities.push(name);
        }
    }
    cx.tx.insert_agent(&agent)?;
    view(cx, &agent)
}

/// Quién soy y qué agentes puedo usar (D15): la base de `agent_use`.
fn mine(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let current = views::actor_name(r, &cx.ctx.actor)?;
    let acting_as = match &cx.ctx.actor {
        Actor::Agent(_) => Some(current.clone()),
        _ => None,
    };
    let Some(interface) = crate::identity::interface_of(r, &cx.ctx.actor)? else {
        return Ok(
            json!({ "actor": current, "acting_as": acting_as, "interface": null, "agents": [] }),
        );
    };
    let agents = crate::identity::agents_of(r, &interface)?
        .iter()
        .map(|a| views::agent(r, a, cx.now))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "actor": current,
        "acting_as": acting_as,
        "interface": interface.name,
        "agents": agents,
    }))
}

fn list_agents(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    paged(r.agents()?, args, |a| views::agent(r, a, cx.now))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let agent = lookup::agent(&*cx.tx, args.require("agent")?)?;
    view(cx, &agent)
}

/// Agente que el supervisor va a modificar; uno revocado ya no se toca.
fn manageable(cx: &Cx<'_>, args: &Args) -> Result<Agent, EngineError> {
    supervisor_only(cx)?;
    let agent = lookup::agent(&*cx.tx, args.require("agent")?)?;
    if agent.status == AgentStatus::Revoked {
        return Err(EngineError::invalid(
            Msg::new("agent-revoked-final").arg("agent", &agent.name),
        ));
    }
    Ok(agent)
}

fn save(cx: &mut Cx<'_>, agent: &mut Agent) -> Result<Value, EngineError> {
    agent.updated_at = cx.now;
    cx.tx.update_agent(agent)?;
    view(cx, agent)
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut agent = manageable(cx, args)?;
    // Mover a otra interfaz (sobre todo los agentes antiguos que no tienen).
    if let Some(reference) = args.get("interface") {
        let interface = super::interface::find(&*cx.tx, reference)?;
        if agent.interface_id != Some(interface.id) {
            check_room(cx, &interface)?;
            agent.interface_id = Some(interface.id);
            agent.level = capped(agent.level, &interface);
        }
    }
    if let Some(model) = args.get("model") {
        agent.model = model.trim().to_owned();
    }
    if let Some(description) = args.get("description") {
        agent.description = description.to_owned();
    }
    if let Some(color) = args.get("color") {
        agent.color = rules::label_color(color)?;
    }
    if let Some(level) = args.parse("level")? {
        agent.level = match agent.interface_id.map(|i| cx.tx.interface(i)).transpose()? {
            Some(Some(interface)) => capped(level, &interface),
            _ => level,
        };
    }
    if args.get("scope").is_some() {
        let mut scope = Vec::new();
        for key in args.list("scope") {
            scope.push(lookup::project(&*cx.tx, &key)?.id);
        }
        agent.scope = scope;
    }
    if args.get("capabilities").is_some() {
        let mut caps: Vec<String> = Vec::new();
        for c in args.list("capabilities") {
            let name = rules::label_name(&c)?.to_lowercase();
            if !caps.contains(&name) {
                caps.push(name);
            }
        }
        agent.capabilities = caps;
    }
    save(cx, &mut agent)
}

/// Pausa: puede leer y sigue conectado, pero no escribe (RF-SUP-08).
fn set_paused(cx: &mut Cx<'_>, args: &Args, paused: bool) -> Result<Value, EngineError> {
    let mut agent = manageable(cx, args)?;
    agent.paused = paused;
    save(cx, &mut agent)
}

fn pause(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_paused(cx, args, true)
}

fn resume(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_paused(cx, args, false)
}

/// Cambia el estado y, si deja de estar activo, cierra sus sesiones abiertas.
fn set_status(cx: &mut Cx<'_>, args: &Args, status: AgentStatus) -> Result<Value, EngineError> {
    let mut agent = manageable(cx, args)?;
    agent.status = status;
    if status != AgentStatus::Active {
        for mut session in cx.tx.open_sessions(Some(agent.id))? {
            session.ended_at = Some(cx.now);
            cx.tx.upsert_session(&session)?;
        }
    }
    save(cx, &mut agent)
}

fn disable(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_status(cx, args, AgentStatus::Disabled)
}

fn enable(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_status(cx, args, AgentStatus::Active)
}

fn revoke(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    set_status(cx, args, AgentStatus::Revoked)
}

/// Token nuevo (D09); el sistema debe poder dar aleatoriedad.
pub(super) fn new_token() -> Result<String, EngineError> {
    foundation::secret::generate_token()
        .ok_or_else(|| EngineError::invalid(Msg::new("agent-token-unavailable")))
}

/// El token solo se muestra en la respuesta que lo emite.
pub(super) fn with_token(mut view: Value, token: String) -> Result<Value, EngineError> {
    if let Some(obj) = view.as_object_mut() {
        obj.insert("token".into(), Value::String(token));
    }
    Ok(view)
}
