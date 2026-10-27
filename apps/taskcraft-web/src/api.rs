//! Cliente de la API web del daemon (D14): `POST /api/command` y
//! `GET /api/changes`. Las respuestas llegan por un buzón que la UI vacía en
//! cada frame; así la UI no depende de cómo se hace la petición.

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;

/// Para qué era la petición: decide dónde va la respuesta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    WhoAmI,
    Projects,
    Tasks,
    Review,
    Agents,
    Activity,
    /// Un cambio (aprobar, mover…): al terminar se recarga.
    Action,
    Changes,
}

/// Respuesta de una petición: el `ok` o el mensaje de error.
#[derive(Debug, Clone)]
pub struct Reply {
    pub purpose: Purpose,
    pub result: Result<Value, String>,
}

pub type Inbox = Arc<Mutex<Vec<Reply>>>;

pub fn take(inbox: &Inbox) -> Vec<Reply> {
    std::mem::take(&mut *inbox.lock().unwrap_or_else(PoisonError::into_inner))
}

pub fn push(inbox: &Inbox, reply: Reply) {
    inbox
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(reply);
}

/// Hace las peticiones; en el navegador, con `fetch` (ver `net`).
pub trait Transport {
    /// `POST /api/command`.
    fn command(&self, purpose: Purpose, command: &str, args: Value);
    /// `GET /api/changes?since=…` (sondeo largo).
    fn changes(&self, since: Option<u64>);
    fn whoami(&self);
}

/// Interpreta el cuerpo de `/api/command`: `{"ok": …}` o `{"error": {message}}`.
pub fn parse(status: u16, body: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|_| format!("HTTP {status}: {}", body.trim()))?;
    if let Some(ok) = value.get("ok") {
        return Ok(ok.clone());
    }
    let error = value.get("error").unwrap_or(&value);
    Err(error
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| format!("HTTP {status}"), str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_ok_or_a_message() {
        assert_eq!(parse(200, r#"{"ok": {"count": 1}}"#).unwrap()["count"], 1);
        assert_eq!(
            parse(
                403,
                r#"{"error": {"code": "FORBIDDEN", "message": "solo el supervisor"}}"#
            ),
            Err("solo el supervisor".to_owned())
        );
        assert_eq!(
            parse(401, r#"{"code": "FORBIDDEN", "message": "invalid token"}"#),
            Err("invalid token".to_owned())
        );
        assert!(parse(502, "Bad Gateway").unwrap_err().contains("502"));
    }
}
