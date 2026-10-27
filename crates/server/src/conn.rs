//! Una conexión al daemon: saludo e identidad (D09), sesión de agente
//! (RF-ACT-03), peticiones y, si se suscribe, eventos (RNF-08).

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use engine::{Args, Context, EngineError, identity};
use i18n::Lang;
use model::Actor;
use protocol::{ClientMsg, Event, ServerMsg, WireError};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::Shared;

/// Cada cuánto se renueva la sesión de un agente conectado aunque no haga nada.
const SESSION_HEARTBEAT: Duration = Duration::from_secs(30);

/// Cuenta la conexión mientras vive (para el cierre por inactividad).
struct ClientGuard(Shared);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.clients.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) async fn handle<S>(stream: S, shared: Shared)
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    shared.clients.fetch_add(1, Ordering::SeqCst);
    let _guard = ClientGuard(shared.clone());
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();

    let Ok(Some(first)) = lines.next_line().await else {
        return;
    };
    let (client, lang, token, agent) = match protocol::decode::<ClientMsg>(&first) {
        Ok(ClientMsg::Hello {
            client,
            version,
            lang,
            token,
            agent,
        }) if version == protocol::VERSION => (client, lang_of(lang.as_deref()), token, agent),
        Ok(ClientMsg::Hello { version, .. }) => {
            let err = WireError::new(
                "INVALID",
                "protocol-version",
                format!(
                    "protocol version {version} not supported; this daemon speaks {}",
                    protocol::VERSION
                ),
            );
            let _ = send(&mut write, &ServerMsg::Rejected { err }).await;
            return;
        }
        // Un `Ping` sin saludo sirve para comprobar si el daemon está vivo.
        Ok(ClientMsg::Ping) => {
            let _ = send(&mut write, &pong()).await;
            return;
        }
        _ => {
            let err = WireError::new("INVALID", "protocol-hello", "expected a hello message");
            let _ = send(&mut write, &ServerMsg::Rejected { err }).await;
            return;
        }
    };

    let actor = match identify(&shared, token, agent).await {
        Ok(actor) => actor,
        Err(e) => {
            let err = e.to_wire(lang);
            let _ = send(&mut write, &ServerMsg::Rejected { err }).await;
            return;
        }
    };
    // Una sesión por cada agente que use la conexión (D15: puede cambiar).
    let mut sessions = Sessions::default();
    let session = sessions.ensure(&shared, &actor, &client).await;
    let welcome = ServerMsg::Welcome {
        actor: actor.to_string(),
        session,
        version: protocol::VERSION,
    };
    if send(&mut write, &welcome).await.is_err() {
        sessions.close_all(&shared).await;
        return;
    }

    let mut shutdown = shared.shutdown.subscribe();
    let mut beat = tokio::time::interval(SESSION_HEARTBEAT);
    beat.tick().await;
    let mut last_beat = Instant::now();
    loop {
        let line = tokio::select! {
            line = lines.next_line() => line,
            _ = beat.tick() => {
                if last_beat.elapsed() >= SESSION_HEARTBEAT {
                    sessions.heartbeat_all(&shared).await;
                    last_beat = Instant::now();
                }
                continue;
            }
            _ = shutdown.changed() => break,
        };
        let Ok(Some(line)) = line else { break };
        let reply = match protocol::decode::<ClientMsg>(&line) {
            Ok(ClientMsg::Request {
                id,
                command,
                args,
                lang: req_lang,
            }) => {
                if last_beat.elapsed() >= SESSION_HEARTBEAT {
                    sessions.heartbeat_all(&shared).await;
                    last_beat = Instant::now();
                }
                // Con `as_agent`, ese agente también queda conectado.
                if let Some(name) = args.get(engine::AS_AGENT).cloned()
                    && let Ok(other) = switched(&shared, &actor, name).await
                {
                    sessions.ensure(&shared, &other, &client).await;
                }
                let lang = req_lang.as_deref().map_or(lang, |l| lang_of(Some(l)));
                let result = run(&shared, &actor, command, Args::from_map(args)).await;
                ServerMsg::response(id, result.map_err(|e| e.to_wire(lang)))
            }
            Ok(ClientMsg::Ping) => pong(),
            Ok(ClientMsg::Subscribe) => {
                subscribe(&shared, &mut write).await;
                break;
            }
            Ok(ClientMsg::Shutdown) if actor == Actor::Supervisor => {
                let _ = send(&mut write, &ServerMsg::Bye).await;
                let _ = shared.shutdown.send(true);
                break;
            }
            Ok(ClientMsg::Shutdown) => ServerMsg::Rejected {
                err: WireError::new(
                    "FORBIDDEN",
                    "supervisor-only",
                    i18n::tr(lang, &i18n::Msg::new("supervisor-only")),
                ),
            },
            Ok(ClientMsg::Hello { .. }) | Err(_) => ServerMsg::Rejected {
                err: WireError::new("INVALID", "protocol-message", "malformed message"),
            },
        };
        if send(&mut write, &reply).await.is_err() {
            break;
        }
    }
    sessions.close_all(&shared).await;
}

/// A quién pasa a ser `actor` con `as_agent` (para su sesión; el engine lo
/// vuelve a comprobar al ejecutar).
async fn switched(shared: &Shared, actor: &Actor, name: String) -> Result<Actor, EngineError> {
    let engine = shared.engine.clone();
    let actor = actor.clone();
    blocking(move || engine.read(|r| identity::act_as(r, &actor, &name))).await
}

/// Sesiones abiertas por la conexión, una por agente (RF-ACT-03).
#[derive(Default)]
struct Sessions(BTreeMap<String, (Actor, String)>);

impl Sessions {
    /// Abre la sesión del agente si aún no la tiene; devuelve su ID.
    async fn ensure(&mut self, shared: &Shared, actor: &Actor, client: &str) -> Option<String> {
        if !matches!(actor, Actor::Agent(_)) {
            return None;
        }
        let key = actor.to_string();
        if let Some((_, id)) = self.0.get(&key) {
            return Some(id.clone());
        }
        let id = open_session(shared, actor, client).await?;
        self.0.insert(key, (actor.clone(), id.clone()));
        Some(id)
    }

    async fn heartbeat_all(&self, shared: &Shared) {
        for (actor, id) in self.0.values() {
            heartbeat(shared, actor, Some(id)).await;
        }
    }

    async fn close_all(&mut self, shared: &Shared) {
        for (actor, id) in std::mem::take(&mut self.0).into_values() {
            close_session(shared, &actor, Some(id)).await;
        }
    }
}

fn lang_of(code: Option<&str>) -> Lang {
    code.and_then(Lang::from_tag).unwrap_or(Lang::En)
}

fn pong() -> ServerMsg {
    ServerMsg::Pong {
        version: protocol::VERSION,
        pid: std::process::id(),
    }
}

async fn send(write: &mut (impl AsyncWrite + Unpin), msg: &ServerMsg) -> std::io::Result<()> {
    write.write_all(protocol::encode(msg).as_bytes()).await
}

/// Token → su agente; nombre de agente → ese agente (salvo que se exija
/// token); nada → supervisor (D09).
async fn identify(
    shared: &Shared,
    token: Option<String>,
    agent: Option<String>,
) -> Result<Actor, EngineError> {
    let engine = shared.engine.clone();
    let require_token = shared.require_token;
    blocking(move || {
        let agent = agent.filter(|a| !a.trim().is_empty());
        // D15: el token es de una interfaz; `agent` elige uno de los suyos.
        if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
            return engine.read(|r| {
                let interface = identity::interface_by_token(r, &token)?;
                match &agent {
                    Some(name) => identity::act_as(r, &Actor::Interface(interface.id), name),
                    None => identity::default_actor(r, &interface),
                }
            });
        }
        match agent {
            Some(_) if require_token => Err(EngineError::Forbidden(i18n::Msg::new(
                "agent-token-required",
            ))),
            Some(name) => engine.read(|r| identity::act_as(r, &Actor::Supervisor, &name)),
            None => Ok(Actor::Supervisor),
        }
    })
    .await
}

async fn run(
    shared: &Shared,
    actor: &Actor,
    command: String,
    args: Args,
) -> Result<serde_json::Value, EngineError> {
    let engine = shared.engine.clone();
    let ctx = Context::new(actor.clone());
    blocking(move || engine.dispatch(&ctx, &command, &args)).await
}

/// El engine es síncrono: cada comando corre en el pool de bloqueo (D08).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, EngineError> + Send + 'static,
) -> Result<T, EngineError> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(EngineError::Panicked(e.to_string())))
}

async fn open_session(shared: &Shared, actor: &Actor, client: &str) -> Option<String> {
    let args = Args::none().with("transport", client);
    match run(shared, actor, "session.open".into(), args).await {
        Ok(v) => v.get("session").and_then(|s| s.as_str()).map(str::to_owned),
        Err(e) => {
            tracing::warn!(error = %e, "could not open agent session");
            None
        }
    }
}

async fn heartbeat(shared: &Shared, actor: &Actor, session: Option<&str>) {
    if let Some(session) = session {
        let args = Args::none().with("session", session);
        let _ = run(shared, actor, "session.heartbeat".into(), args).await;
    }
}

async fn close_session(shared: &Shared, actor: &Actor, session: Option<String>) {
    if let Some(session) = session {
        let args = Args::none().with("session", session);
        // Si el agente fue desactivado mientras tanto, su sesión ya está cerrada.
        let _ = run(shared, actor, "session.close".into(), args).await;
    }
}

/// Reenvía cada escritura confirmada hasta que el cliente se vaya.
async fn subscribe(shared: &Shared, write: &mut (impl AsyncWrite + Unpin)) {
    let mut events = shared.events.subscribe();
    let mut shutdown = shared.shutdown.subscribe();
    loop {
        let token = tokio::select! {
            ev = events.recv() => match ev {
                Ok(token) => token,
                // Si el cliente se quedó atrás, basta con avisarle del último cambio.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let engine = shared.engine.clone();
                    blocking(move || engine.change_token()).await.unwrap_or_default()
                }
                Err(_) => return,
            },
            _ = shutdown.changed() => return,
        };
        let msg = ServerMsg::Event(Event::Changed { token });
        if send(write, &msg).await.is_err() {
            return;
        }
    }
}
