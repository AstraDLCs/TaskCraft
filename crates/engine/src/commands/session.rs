//! Sesiones de agente (RF-ACT-03): qué agentes están conectados ahora. El
//! daemon (D01) abrirá una por conexión; hoy la CLI abre una por ejecución.

use i18n::Msg;
use model::{Actor, AgentId, AgentSession, SessionId};
use serde_json::{Value, json};

use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError};

command!(Open, "session.open", "cmd-session-open", open, write, [opt "transport": K::Text]);
command!(Heartbeat, "session.heartbeat", "cmd-session-heartbeat", heartbeat, write, [req "session": K::Text]);
command!(Close, "session.close", "cmd-session-close", close, write, [req "session": K::Text]);

fn caller(cx: &Cx<'_>) -> Result<AgentId, EngineError> {
    match cx.ctx.actor {
        Actor::Agent(id) => Ok(id),
        _ => Err(EngineError::forbidden(Msg::new("session-agent-only"))),
    }
}

/// Sesión del agente que llama, todavía abierta.
fn own(cx: &Cx<'_>, args: &Args) -> Result<AgentSession, EngineError> {
    let agent = caller(cx)?;
    let reference = args.require("session")?;
    let not_found =
        || EngineError::not_found(Msg::new("notfound-session").arg("session", reference));
    let id: SessionId = reference.parse().map_err(|_| not_found())?;
    let session = cx.tx.session(id)?.ok_or_else(not_found)?;
    if session.agent_id != agent {
        return Err(EngineError::forbidden(Msg::new("session-other-agent")));
    }
    if session.ended_at.is_some() {
        return Err(not_found());
    }
    Ok(session)
}

fn view(s: &AgentSession) -> Value {
    json!({
        "session": s.id,
        "transport": s.transport,
        "started_at": s.started_at,
        "last_seen": s.last_seen,
        "ended_at": s.ended_at,
    })
}

fn open(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let session = AgentSession {
        id: SessionId::generate(),
        agent_id: caller(cx)?,
        transport: args.get("transport").unwrap_or("cli").to_owned(),
        started_at: cx.now,
        last_seen: cx.now,
        ended_at: None,
    };
    cx.tx.upsert_session(&session)?;
    Ok(view(&session))
}

fn heartbeat(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut session = own(cx, args)?;
    session.last_seen = cx.now;
    cx.tx.upsert_session(&session)?;
    Ok(view(&session))
}

fn close(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut session = own(cx, args)?;
    session.last_seen = cx.now;
    session.ended_at = Some(cx.now);
    cx.tx.upsert_session(&session)?;
    Ok(view(&session))
}
