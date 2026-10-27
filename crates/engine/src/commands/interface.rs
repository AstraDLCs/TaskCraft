//! Interfaces (D15, RF-ACT-07): el cliente LLM que tiene el token y agrupa
//! agentes. Solo el supervisor las registra y administra. El token se guarda
//! para poder copiarlo desde la app (D22); nunca sale en vistas, historial
//! ni exportaciones, solo con `interface.token` o al emitirlo.

use i18n::Msg;
use model::{AgentLevel, AgentStatus, Interface, InterfaceId};
use serde_json::{Value, json};
use store::Reader;

use super::agent::{new_token, supervisor_only, with_token};
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup, views};

command!(Register, "interface.register", "cmd-interface-register", register, write, [req "name": K::Text, opt "kind": K::Text, opt "description": K::Text, opt "color": K::Text, opt "level": K::Enum(AgentLevel::NAMES), opt "scope": K::List, opt "max_agents": K::Int]);
command!(List, "interface.list", "cmd-interface-list", list, read, []);
command!(Get, "interface.get", "cmd-interface-get", get, read, [req "interface": K::Text]);
command!(Update, "interface.update", "cmd-interface-update", update, write, [req "interface": K::Text, opt "name": K::Text, opt "color": K::Text, opt "kind": K::Text, opt "description": K::Text, opt "level": K::Enum(AgentLevel::NAMES), opt "scope": K::List, opt "max_agents": K::Int]);
command!(Pause, "interface.pause", "cmd-interface-pause", pause, write, [req "interface": K::Text]);
command!(Resume, "interface.resume", "cmd-interface-resume", resume, write, [req "interface": K::Text]);
command!(Disable, "interface.disable", "cmd-interface-disable", disable, write, [req "interface": K::Text]);
command!(Enable, "interface.enable", "cmd-interface-enable", enable, write, [req "interface": K::Text]);
command!(Revoke, "interface.revoke", "cmd-interface-revoke", revoke, write, [req "interface": K::Text]);
command!(RotateToken, "interface.rotate_token", "cmd-interface-rotate_token", rotate_token, write, [req "interface": K::Text]);
command!(Token, "interface.token", "cmd-interface-token", token, read, [req "interface": K::Text]);

/// Por nombre o ULID.
pub(super) fn find(r: &dyn Reader, reference: &str) -> Result<Interface, EngineError> {
    let found = match reference.trim().parse::<InterfaceId>() {
        Ok(id) => r.interface(id)?,
        Err(_) => r.interface_by_name(reference)?,
    };
    found.ok_or_else(|| {
        EngineError::not_found(Msg::new("notfound-interface").arg("interface", reference))
    })
}

/// Crea la interfaz con su token; devuelve el token en claro (solo esta vez).
pub(super) fn new_interface(
    cx: &mut Cx<'_>,
    name: &str,
    kind: &str,
    description: &str,
) -> Result<(Interface, String), EngineError> {
    let name = rules::interface_name(name)?;
    name_free(cx, &name, None)?;
    let token = new_token()?;
    let interface = Interface {
        id: InterfaceId::generate(),
        color: rules::default_label_color(&name),
        name,
        kind: kind.trim().to_owned(),
        description: description.to_owned(),
        status: AgentStatus::Active,
        level: AgentLevel::Admin,
        scope: Vec::new(),
        max_agents: 50,
        paused: false,
        token_hash: Some(foundation::secret::hash_token(&token)),
        token: Some(token.clone()),
        created_at: cx.now,
        updated_at: cx.now,
    };
    cx.tx.insert_interface(&interface)?;
    Ok((interface, token))
}

/// Ninguna otra interfaz se llama así (sin distinguir mayúsculas).
fn name_free(cx: &Cx<'_>, name: &str, except: Option<InterfaceId>) -> Result<(), EngineError> {
    match cx.tx.interface_by_name(name)? {
        Some(other) if Some(other.id) != except => Err(EngineError::invalid(
            Msg::new("interface-name-taken").arg("name", name),
        )),
        _ => Ok(()),
    }
}

fn view(cx: &Cx<'_>, i: &Interface) -> Result<Value, EngineError> {
    views::interface(&*cx.tx, i, cx.now)
}

fn apply(cx: &Cx<'_>, i: &mut Interface, args: &Args) -> Result<(), EngineError> {
    if let Some(name) = args.get("name") {
        let name = rules::interface_name(name)?;
        name_free(cx, &name, Some(i.id))?;
        i.name = name;
    }
    if let Some(color) = args.get("color") {
        i.color = rules::label_color(color)?;
    }
    if let Some(kind) = args.get("kind") {
        i.kind = kind.trim().to_owned();
    }
    if let Some(description) = args.get("description") {
        i.description = description.to_owned();
    }
    if let Some(level) = args.parse("level")? {
        i.level = level;
    }
    if args.get("scope").is_some() {
        let mut scope = Vec::new();
        for key in args.list("scope") {
            scope.push(lookup::project(&*cx.tx, &key)?.id);
        }
        i.scope = scope;
    }
    if let Some(max) = args.parse::<u32>("max_agents")? {
        if !(1..=1000).contains(&max) {
            return Err(EngineError::arg(
                "max_agents",
                Msg::new("arg-range").arg("min", 1u32).arg("max", 1000u32),
            ));
        }
        i.max_agents = max;
    }
    Ok(())
}

fn register(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let (mut interface, token) = new_interface(
        cx,
        args.require("name")?,
        args.get("kind").unwrap_or("custom"),
        args.get("description").unwrap_or_default(),
    )?;
    apply(cx, &mut interface, args)?;
    cx.tx.update_interface(&interface)?;
    with_token(view(cx, &interface)?, token)
}

fn list(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let items = r
        .interfaces()?
        .iter()
        .map(|i| views::interface(r, i, cx.now))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "count": items.len(), "items": items }))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let i = find(&*cx.tx, args.require("interface")?)?;
    view(cx, &i)
}

/// La que el supervisor va a modificar; una revocada ya no se toca.
fn manageable(cx: &Cx<'_>, args: &Args) -> Result<Interface, EngineError> {
    supervisor_only(cx)?;
    let i = find(&*cx.tx, args.require("interface")?)?;
    if i.status == AgentStatus::Revoked {
        return Err(EngineError::invalid(
            Msg::new("agent-revoked-final").arg("agent", &i.name),
        ));
    }
    Ok(i)
}

fn save(cx: &mut Cx<'_>, i: &mut Interface) -> Result<Value, EngineError> {
    i.updated_at = cx.now;
    cx.tx.update_interface(i)?;
    view(cx, i)
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut i = manageable(cx, args)?;
    apply(cx, &mut i, args)?;
    save(cx, &mut i)
}

/// Pausa a todos sus agentes: leen, pero no escriben (RF-SUP-08).
fn pause(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut i = manageable(cx, args)?;
    i.paused = true;
    save(cx, &mut i)
}

fn resume(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut i = manageable(cx, args)?;
    i.paused = false;
    save(cx, &mut i)
}

/// Fuera de servicio, su token deja de valer y se cierran las sesiones de
/// sus agentes.
fn set_status(cx: &mut Cx<'_>, args: &Args, status: AgentStatus) -> Result<Value, EngineError> {
    let mut i = manageable(cx, args)?;
    i.status = status;
    if status != AgentStatus::Active {
        for agent in cx
            .tx
            .agents()?
            .iter()
            .filter(|a| a.interface_id == Some(i.id))
        {
            for mut session in cx.tx.open_sessions(Some(agent.id))? {
                session.ended_at = Some(cx.now);
                cx.tx.upsert_session(&session)?;
            }
        }
    }
    save(cx, &mut i)
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

fn rotate_token(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut i = manageable(cx, args)?;
    let token = new_token()?;
    i.token_hash = Some(foundation::secret::hash_token(&token));
    i.token = Some(token.clone());
    let view = save(cx, &mut i)?;
    with_token(view, token)
}

/// El token guardado, para copiarlo (D22). Las interfaces de antes de D22
/// no lo tienen: hay que generar uno nuevo con `interface.rotate_token`.
fn token(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let i = find(&*cx.tx, args.require("interface")?)?;
    match i.token {
        Some(token) => Ok(json!({ "interface": i.name, "token": token })),
        None => Err(EngineError::invalid(
            Msg::new("interface-token-unknown").arg("interface", &i.name),
        )),
    }
}
