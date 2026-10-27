//! L6 — Servidor MCP (RF-MCP-01…04/07/08), generado desde el registro de
//! comandos del engine con `rmcp` (D08).
//!
//! Dos backends con la misma interfaz:
//! - [`RemoteBackend`]: `taskcraft-cli mcp --stdio` hace de proxy hacia el daemon;
//! - [`LocalBackend`]: el daemon sirve MCP por HTTP sobre su propio engine.

mod caldav;
pub mod http;
mod ical;
mod resources;
mod tools;
mod watch;
pub mod web;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use engine::{Args, Context, Registry, SharedEngine};
use i18n::Lang;
use model::{Actor, AgentId};
use protocol::{ArgMap, WireError};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, GetPromptRequestParams,
    GetPromptResponse, Implementation, ListPromptsResult, ListResourceTemplatesResult,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ResourceUpdatedNotificationParam, ServerCapabilities, ServerConfig,
    SubscribeRequestParams, SubscriptionFilter, Tool, UnsubscribeRequestParams,
};
use rmcp::service::{RequestContext, SubscriptionContext};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::{Value, json};
use tokio::sync::broadcast;

pub use tools::{hint, tool_name};

/// Lo que se explica a cada agente al conectarse.
const INSTRUCTIONS: &str = "TaskCraft coordinates several AI agents on shared projects while a \
human supervises. Typical loop: call task_next with claim=true to get the highest-priority task \
that is ready, read it with task_get, renew the lease with task_heartbeat while you work (any \
write on the task also renews it), tick acceptance criteria with task_check, leave progress notes \
with comment_add and finish by moving it with task_move (to done, or to the column of whoever \
comes next, with a `note` for them) or with task_submit_review (summary + met criteria). Nobody \
has to approve your work. If you are stuck, use task_escalate with a question or task_release with \
a reason. Pass `revision` on writes to \
avoid overwriting someone else's changes; on CONFLICT, re-read and retry. Lists are paginated: \
use limit, cursor (next_cursor) and fields. Your token belongs to an interface (your LLM client): \
create your agents with agent_create, pick one with agent_use (or pass as_agent on any call). \
A task can have several assigned agents, with no fixed roles: what each agent does is its \
function, the description given when it was created. Any agent can create workflow columns with \
status_create, also owned by an agent (owner=): its queue or its workbench. Moving a task into a \
column owned by an agent assigns it to that agent, and task_next offers what is in your columns \
first. If the task says who comes next (\"then pass it to the reviewer\"), create or use that \
agent's column and move the task there with a note. Read the notes returned by \
task_claim/task_next: they are what previous agents left for you. Put the task key in your commit messages \
(\"API-42: ...\") so TaskCraft links the commits to the task, and record pull requests with \
git_link kind=pr. To explain a design or a flow, write it as a Mermaid diagram with \
diagram_create (it gets a key like API-D3; check render_error and fix it with diagram_update) and \
cite it in a task description or comment: [[API-D3]] links to it and ![[API-D3]] alone on a line \
shows it drawn to the human.";

/// Quién ejecuta los comandos de las tools.
pub trait Backend: Send + Sync + 'static {
    /// `caller`: agente autenticado por la petición HTTP (D09); en stdio es
    /// `None` y manda la identidad de la conexión.
    fn call(&self, caller: Option<Actor>, command: &str, args: &ArgMap)
    -> Result<Value, WireError>;

    /// Cada escritura confirmada (su `change_token`), para avisar a los
    /// suscriptores (RF-MCP-10). `None`: este backend no puede avisar.
    fn changes(&self) -> Option<broadcast::Receiver<u64>> {
        None
    }
}

/// Interfaz (o su único agente) autenticada por `Authorization: Bearer` (D15;
/// lo inserta [`http`]).
#[derive(Debug, Clone)]
pub struct Caller(pub Actor);

/// Manejador MCP: lista las tools y las ejecuta en el backend.
#[derive(Clone)]
pub struct TaskcraftMcp {
    backend: Arc<dyn Backend>,
    registry: Arc<Registry>,
    tools: Arc<Vec<Tool>>,
    /// Agente elegido con `agent_use` en esta sesión (D15).
    acting: Arc<Mutex<Option<String>>>,
    /// Recursos suscritos con `resources/subscribe` (RF-MCP-10).
    watched: watch::Shared,
    /// Ya corre el bucle que avisa de esas suscripciones.
    watching: Arc<AtomicBool>,
}

impl TaskcraftMcp {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        let registry = Registry::standard();
        let tools = tools::all(&registry);
        Self {
            backend,
            registry: Arc::new(registry),
            tools: Arc::new(tools),
            acting: Arc::new(Mutex::new(None)),
            watched: watch::Shared::default(),
            watching: Arc::new(AtomicBool::new(false)),
        }
    }

    fn acting(&self) -> Option<String> {
        self.acting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn run(
        &self,
        caller: Option<Actor>,
        id: String,
        args: ArgMap,
    ) -> Result<Value, WireError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.call(caller, &id, &args))
            .await
            .unwrap_or_else(|e| Err(WireError::new("INTERNAL", "error-panicked", e.to_string())))
    }

    /// `agent_use`: con `agent`, lo comprueba y lo deja activo; sin él, dice
    /// quién es y qué agentes tiene la interfaz.
    async fn agent_use(&self, caller: Option<Actor>, args: ArgMap) -> CallToolResult {
        let wanted = args
            .get("agent")
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty());
        let mut probe = ArgMap::new();
        if let Some(name) = wanted.clone().or_else(|| self.acting()) {
            probe.insert(engine::AS_AGENT.into(), name);
        }
        match self.run(caller, "agent.mine".into(), probe).await {
            Ok(v) => {
                if let Some(name) = wanted {
                    *self.acting.lock().unwrap_or_else(PoisonError::into_inner) = Some(name);
                }
                CallToolResult::structured(v)
            }
            Err(e) => error_result(&e),
        }
    }

    /// Ejecuta una tool y la convierte en resultado MCP: los errores del
    /// engine son resultados `isError` con código, mensaje y pista (RF-MCP-07).
    pub async fn call(
        &self,
        caller: Option<Actor>,
        request: CallToolRequestParams,
    ) -> CallToolResult {
        if request.name == tools::AGENT_USE {
            return self
                .agent_use(caller, tools::to_args(request.arguments))
                .await;
        }
        let Some(command) = tools::command_for(&self.registry, &request.name) else {
            return error_result(&WireError::new(
                "NOT_FOUND",
                "mcp-unknown-tool",
                format!("Unknown tool `{}`", request.name),
            ));
        };
        let id = command.id().to_owned();
        let mut args = tools::to_args(request.arguments);
        // El agente elegido con `agent_use`, salvo que la llamada diga otro.
        if let Some(name) = self.acting() {
            args.entry(engine::AS_AGENT.into()).or_insert(name);
        }
        match self.run(caller, id, args).await {
            Ok(Value::Object(map)) => CallToolResult::structured(Value::Object(map)),
            Ok(other) => CallToolResult::structured(json!({ "result": other })),
            Err(e) => error_result(&e),
        }
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }
}

fn error_result(e: &WireError) -> CallToolResult {
    let mut body = json!({
        "code": e.code,
        "message": e.message,
        "hint": hint(&e.code),
    });
    if let (Some(current), Some(obj)) = (&e.current, body.as_object_mut()) {
        obj.insert("current".into(), (**current).clone());
    }
    CallToolResult::structured_error(body)
}

impl ServerHandler for TaskcraftMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_prompts()
                .enable_resources()
                .enable_resources_subscribe()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("taskcraft", env!("CARGO_PKG_VERSION")))
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.to_vec()))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        resources::list(&self.backend, caller_of(&context))
            .await
            .map(ListResourcesResult::with_all_items)
            .map_err(protocol_error)
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resources::templates(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        resources::read(&self.backend, caller_of(&context), &request.uri)
            .await
            .map(Into::into)
            .map_err(protocol_error)
    }

    /// `resources/subscribe` (RF-MCP-10): avisa con
    /// `notifications/resources/updated` cuando el recurso cambie.
    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let caller = caller_of(&context);
        let Some(digest) = watch::digest(&self.backend, caller.clone(), &request.uri).await else {
            return Err(ErrorData::new(
                rmcp::model::ErrorCode::RESOURCE_NOT_FOUND,
                format!("Unknown resource `{}`", request.uri),
                None,
            ));
        };
        self.watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request.uri, digest);
        if !self.watching.swap(true, Ordering::SeqCst)
            && let Some(changes) = self.backend.changes()
        {
            let peer = context.peer.clone();
            tokio::spawn(watch::run(
                self.backend.clone(),
                caller,
                self.watched.clone(),
                changes,
                move |uri| {
                    let peer = peer.clone();
                    async move {
                        peer.notify_resource_updated(ResourceUpdatedNotificationParam::new(uri))
                            .await
                            .is_ok()
                    }
                },
            ));
        }
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        self.watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&request.uri);
        Ok(())
    }

    /// `subscriptions/listen` (2026-07-28): se aceptan las suscripciones a
    /// recursos `taskcraft://`; las listas de tools y prompts no cambian.
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let uris: Vec<String> = requested
            .resource_subscriptions
            .iter()
            .flatten()
            .filter(|u| resources::is_known(u))
            .cloned()
            .collect();
        let mut accepted = SubscriptionFilter::new();
        accepted.resource_subscriptions = Some(uris);
        Some(accepted)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        let Some(changes) = self.backend.changes() else {
            context.cancelled().await;
            return Ok(());
        };
        let caller = caller_of(context.request_context());
        let watched = watch::Shared::default();
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            if let Some(d) = watch::digest(&self.backend, caller.clone(), uri).await {
                watched
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(uri.clone(), d);
            }
        }
        let sink = context.sink().clone();
        let run = watch::run(self.backend.clone(), caller, watched, changes, move |uri| {
            let sink = sink.clone();
            async move { sink.notify_resource_updated(uri).await.is_ok() }
        });
        tokio::select! {
            () = run => {}
            () = context.cancelled() => {}
        }
        Ok(())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(ListPromptsResult::with_all_items(resources::prompts()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        let arguments = request.arguments.unwrap_or_default();
        resources::prompt(&request.name, &arguments)
            .map(Into::into)
            .map_err(protocol_error)
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name == name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        Ok(self.call(caller_of(&context), request).await.into())
    }
}

/// Quién autenticó la petición HTTP, si lo hay (en stdio, ninguno).
fn caller_of(context: &RequestContext<RoleServer>) -> Option<Actor> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<Caller>())
        .map(|c| c.0.clone())
}

/// Recursos y prompts no tienen `isError`: un fallo es un error de protocolo.
fn protocol_error(e: WireError) -> ErrorData {
    let code = if e.code == "NOT_FOUND" {
        rmcp::model::ErrorCode::RESOURCE_NOT_FOUND
    } else {
        rmcp::model::ErrorCode::INVALID_PARAMS
    };
    ErrorData::new(code, format!("[{}] {}", e.code, e.message), None)
}

/// Sirve MCP por stdin/stdout hasta que el cliente cierre (RF-MCP-01).
pub async fn serve_stdio(backend: Arc<dyn Backend>) -> Result<(), String> {
    let service = TaskcraftMcp::new(backend)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| e.to_string())?;
    service.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Proxy hacia el daemon por el socket local (D01): la identidad es la de la
/// conexión (token o agente del `Hello`). Si el daemon se reinicia, reconecta.
pub struct RemoteBackend {
    opts: client::Connect,
    conn: Mutex<Option<client::Client>>,
    /// Eventos del daemon, abiertos con la primera suscripción (RF-MCP-10).
    changes: Mutex<Option<broadcast::Sender<u64>>>,
}

impl RemoteBackend {
    pub fn connect(opts: client::Connect) -> Result<Self, client::ClientError> {
        let conn = client::Client::connect(&opts)?;
        Ok(Self {
            opts,
            conn: Mutex::new(Some(conn)),
            changes: Mutex::new(None),
        })
    }
}

impl Backend for RemoteBackend {
    fn call(&self, _: Option<Actor>, command: &str, args: &ArgMap) -> Result<Value, WireError> {
        let mut guard = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        for attempt in 0..2 {
            if guard.is_none() {
                *guard =
                    Some(client::Client::connect(&self.opts).map_err(|e| e.to_wire(Lang::En))?);
            }
            let Some(conn) = guard.as_mut() else { break };
            match conn.request(command, args, Lang::En) {
                Err(e) if e.code == "UNAVAILABLE" && attempt == 0 => *guard = None,
                other => return other,
            }
        }
        Err(WireError::new(
            "UNAVAILABLE",
            "client-unavailable",
            "TaskCraft daemon unavailable",
        ))
    }

    fn changes(&self) -> Option<broadcast::Receiver<u64>> {
        let mut guard = self.changes.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(tx) = guard.as_ref() {
            return Some(tx.subscribe());
        }
        let (tx, rx) = broadcast::channel(64);
        let forward = tx.clone();
        client::subscribe(&self.opts, move |token| {
            let _ = forward.send(token);
            true
        })
        .ok()?;
        *guard = Some(tx);
        Some(rx)
    }
}

/// Cada cuánto se renueva la sesión HTTP de un agente activo.
const HTTP_SESSION_BEAT: Duration = Duration::from_secs(30);

/// Engine del propio daemon, para el MCP por HTTP: solo agentes con token.
/// Cada agente tiene una sesión `http` que se abre con su primera llamada y
/// se renueva mientras siga llamando (RF-ACT-03).
pub struct LocalBackend {
    engine: SharedEngine,
    sessions: Mutex<HashMap<AgentId, (String, Instant)>>,
    /// Escrituras confirmadas en el daemon, para los suscriptores (RF-MCP-10).
    changes: Option<broadcast::Sender<u64>>,
}

impl LocalBackend {
    pub fn new(engine: SharedEngine) -> Self {
        Self {
            engine,
            sessions: Mutex::new(HashMap::new()),
            changes: None,
        }
    }

    /// Con los eventos de escritura del daemon, para avisar a los suscriptores.
    pub fn with_changes(mut self, changes: broadcast::Sender<u64>) -> Self {
        self.changes = Some(changes);
        self
    }

    pub fn engine(&self) -> &SharedEngine {
        &self.engine
    }

    fn touch_session(&self, agent: AgentId) {
        let ctx = Context::new(Actor::Agent(agent));
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        match sessions.get(&agent) {
            Some((_, at)) if at.elapsed() < HTTP_SESSION_BEAT => {}
            Some((id, _)) => {
                let beat = Args::none().with("session", id.clone());
                if self
                    .engine
                    .dispatch(&ctx, "session.heartbeat", &beat)
                    .is_ok()
                {
                    if let Some(entry) = sessions.get_mut(&agent) {
                        entry.1 = Instant::now();
                    }
                } else {
                    sessions.remove(&agent);
                }
            }
            None => {
                let open = Args::none().with("transport", "http");
                if let Ok(v) = self.engine.dispatch(&ctx, "session.open", &open)
                    && let Some(id) = v.get("session").and_then(Value::as_str)
                {
                    sessions.insert(agent, (id.to_owned(), Instant::now()));
                }
            }
        }
    }
}

impl Backend for LocalBackend {
    fn call(
        &self,
        caller: Option<Actor>,
        command: &str,
        args: &ArgMap,
    ) -> Result<Value, WireError> {
        let Some(actor) = caller else {
            return Err(WireError::new(
                "FORBIDDEN",
                "agent-bad-token",
                i18n::t(Lang::En, "agent-bad-token"),
            ));
        };
        // La sesión del agente que actúa de verdad (D15: puede ser otro).
        let acting = match args.get(engine::AS_AGENT) {
            Some(name) => self
                .engine
                .read(|r| engine::identity::act_as(r, &actor, name))
                .ok(),
            None => Some(actor.clone()),
        };
        if let Some(Actor::Agent(agent)) = acting {
            self.touch_session(agent);
        }
        let ctx = Context::new(actor);
        self.engine
            .dispatch(&ctx, command, &Args::from_map(args.clone()))
            .map_err(|e| e.to_wire(Lang::En))
    }

    fn changes(&self) -> Option<broadcast::Receiver<u64>> {
        self.changes.as_ref().map(broadcast::Sender::subscribe)
    }
}
