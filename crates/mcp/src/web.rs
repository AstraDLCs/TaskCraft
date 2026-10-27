//! API JSON para la app web (D14), en el mismo servidor HTTP que el MCP:
//! `POST /api/command`, `GET /api/changes?since=N` y los archivos de la app.
//! Un token de interfaz actúa como su agente (o elige uno con `as_agent` en
//! el cuerpo, D15); el token web del supervisor (`workspace.web_token`),
//! como supervisor.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use engine::{Args, Context, SharedEngine};
use model::Actor;
use serde_json::{Value, json};
use tokio::sync::broadcast;

/// Lo que necesita la API web del daemon.
#[derive(Clone)]
pub struct Web {
    pub engine: SharedEngine,
    /// Escrituras confirmadas (el `change_token` nuevo de cada una).
    pub changes: broadcast::Sender<u64>,
    /// Carpeta del workspace: ahí está `web-token.sha256`.
    pub home: Option<PathBuf>,
    /// Archivos de la app web (`cargo xtask web`); `None` = solo la API.
    pub dir: Option<PathBuf>,
}

/// Cuánto espera `GET /api/changes` antes de responder sin cambios.
const LONG_POLL: Duration = Duration::from_secs(25);

pub(crate) fn router(web: Web) -> axum::Router {
    let state = Arc::new(web);
    let api = axum::Router::new()
        .route("/api/command", axum::routing::post(command))
        .route("/api/changes", axum::routing::get(changes))
        .route("/api/whoami", axum::routing::get(whoami))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authenticate,
        ))
        .with_state(state.clone());
    // CalDAV y suscripciones `.ics` (D18), con su propia autenticación.
    api.merge(crate::caldav::router(state.clone()))
        .fallback(axum::routing::get(file).with_state(state))
}

/// ¿Quién hace la petición? Lo deja `authenticate` en las extensiones.
#[derive(Debug, Clone)]
struct WebActor(Actor);

async fn authenticate(State(web): State<Arc<Web>>, mut request: Request, next: Next) -> Response {
    let Some(token) = crate::http::bearer(request.headers()) else {
        return crate::http::unauthorized("missing Authorization: Bearer <token>");
    };
    if supervisor_token(web.home.as_deref(), &token) {
        request.extensions_mut().insert(WebActor(Actor::Supervisor));
        return next.run(request).await;
    }
    let engine = web.engine.clone();
    let found = tokio::task::spawn_blocking(move || {
        engine.read(|r| engine::identity::from_token(r, &token))
    })
    .await;
    match found {
        Ok(Ok(actor)) => {
            request.extensions_mut().insert(WebActor(actor));
            next.run(request).await
        }
        _ => crate::http::unauthorized("invalid token"),
    }
}

/// Compara con el hash que guardó `workspace.web_token` (D14).
pub(crate) fn supervisor_token(home: Option<&Path>, token: &str) -> bool {
    let Some(home) = home else { return false };
    let Ok(stored) = std::fs::read_to_string(home.join(engine::WEB_TOKEN_FILE)) else {
        return false;
    };
    let stored = stored.trim();
    !stored.is_empty() && stored == foundation::secret::hash_token(token)
}

fn actor(request: &Request) -> Actor {
    request
        .extensions()
        .get::<WebActor>()
        .map_or(Actor::Supervisor, |a| a.0.clone())
}

async fn whoami(State(web): State<Arc<Web>>, request: Request) -> Response {
    let who = actor(&request);
    let engine = web.engine.clone();
    let name =
        tokio::task::spawn_blocking(move || engine.read(|r| engine::views::actor_name(r, &who)))
            .await;
    match name {
        Ok(Ok(name)) => axum::Json(json!({ "actor": name })).into_response(),
        _ => crate::http::unauthorized("unknown actor"),
    }
}

/// `{"command": "task.list", "args": {"project": "API"}}` → `{"ok": …}` o `{"error": …}`.
async fn command(State(web): State<Arc<Web>>, request: Request) -> Response {
    let who = actor(&request);
    let lang = i18n::Lang::from_tag(
        request
            .headers()
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("en"),
    )
    .unwrap_or(i18n::Lang::En);
    let Ok(body) = axum::body::to_bytes(request.into_body(), 1 << 20).await else {
        return bad_request("body too large");
    };
    let Ok(parsed) = serde_json::from_slice::<Value>(&body) else {
        return bad_request("body must be JSON: {\"command\": …, \"args\": {…}}");
    };
    let Some(id) = parsed
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return bad_request("missing `command`");
    };
    let args = parsed
        .get("args")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter().fold(Args::none(), |a, (k, v)| {
                let text = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                a.with(k.as_str(), text)
            })
        })
        .unwrap_or_else(Args::none);
    let engine = web.engine.clone();
    let ran =
        tokio::task::spawn_blocking(move || engine.dispatch(&Context::new(who), &id, &args)).await;
    match ran {
        Ok(Ok(value)) => axum::Json(json!({ "ok": value })).into_response(),
        Ok(Err(e)) => {
            let wire = e.to_wire(lang);
            let status = match wire.code.as_str() {
                "NOT_FOUND" => StatusCode::NOT_FOUND,
                "FORBIDDEN" => StatusCode::FORBIDDEN,
                "CONFLICT" | "LOCKED_BY_HUMAN" | "CLAIMED_BY_OTHER" => StatusCode::CONFLICT,
                "INTERNAL" => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::UNPROCESSABLE_ENTITY,
            };
            (status, axum::Json(json!({ "error": wire }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": { "code": "INTERNAL", "message": e.to_string() } })),
        )
            .into_response(),
    }
}

/// Sondeo largo: responde en cuanto cambia el `change_token` o a los 25 s.
async fn changes(State(web): State<Arc<Web>>, uri: Uri) -> Response {
    let since: Option<u64> = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("since=")))
        .and_then(|v| v.parse().ok());
    let mut events = web.changes.subscribe();
    let engine = web.engine.clone();
    let current = tokio::task::spawn_blocking(move || engine.change_token())
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(0);
    if since != Some(current) {
        return axum::Json(json!({ "token": current })).into_response();
    }
    let token = tokio::select! {
        ev = events.recv() => ev.unwrap_or(current),
        _ = tokio::time::sleep(LONG_POLL) => current,
    };
    axum::Json(json!({ "token": token })).into_response()
}

/// Archivos de la app web; `/` es `index.html`. Nada fuera de la carpeta.
async fn file(State(web): State<Arc<Web>>, uri: Uri) -> Response {
    let Some(dir) = &web.dir else {
        return (StatusCode::NOT_FOUND, "no web app (serve --web <dir>)").into_response();
    };
    let rel = uri.path().trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let path = Path::new(rel);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    let full = dir.join(path);
    match tokio::task::spawn_blocking(move || std::fs::read(full)).await {
        Ok(Ok(bytes)) => ([(header::CONTENT_TYPE, mime(path))], Bytes::from(bytes)).into_response(),
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({ "error": { "code": "INVALID", "message": message } })),
    )
        .into_response()
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
