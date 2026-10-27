//! L5 — Motor: registro de comandos y ejecución transaccional.
//!
//! Botones, atajos, CLI y MCP llegan todos a [`Engine::dispatch`]: es la única
//! forma de modificar el estado (RNF-02). Las lecturas tipadas van por
//! [`Engine::read`], que no permite escribir.

mod args;
mod command;
mod commands;
mod context;
mod error;
mod guard;
pub mod identity;
pub mod lookup;
pub mod permissions;
mod recorder;
mod shared;
pub mod telemetry;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod triggers;
pub mod views;

use std::panic::{AssertUnwindSafe, catch_unwind};

pub use args::Args;
pub use command::{Command, Cx, Kind, Param, Registry};
pub use commands::WEB_TOKEN_FILE;
pub use context::Context;
pub use error::EngineError;
use model::Timestamp;
use model::{Actor, AgentStatus};
use recorder::Recorder;
pub use serde_json::Value;
pub use shared::{LocalDispatch, SharedEngine};
use store::{Reader, Store};

/// Se llama tras cada comando que escribió, con el `change_token` nuevo.
pub type CommitListener = Box<dyn Fn(u64) + Send + Sync>;

pub struct Engine {
    store: Box<dyn Store>,
    registry: Registry,
    on_commit: Option<CommitListener>,
    home: Option<std::path::PathBuf>,
}

impl Engine {
    pub fn new(store: impl Store + 'static) -> Self {
        Self {
            store: Box::new(store),
            registry: Registry::standard(),
            on_commit: None,
            home: None,
        }
    }

    /// Carpeta del workspace: ahí van las copias de seguridad (RF-ALM-06).
    pub fn set_home(&mut self, dir: impl Into<std::path::PathBuf>) {
        self.home = Some(dir.into());
    }

    /// Avisa de cada escritura confirmada (eventos del daemon, RNF-08).
    pub fn set_on_commit(&mut self, listener: CommitListener) {
        self.on_commit = Some(listener);
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Añade un comando al registro (extensiones y tests).
    pub fn register(&mut self, command: impl Command + 'static) {
        self.registry.register(command);
    }

    /// Ejecuta un comando como transacción (RF-ALM-04, RNF-03): si falla o
    /// entra en pánico, no queda ningún cambio en el almacenamiento.
    pub fn dispatch(&mut self, ctx: &Context, id: &str, args: &Args) -> Result<Value, EngineError> {
        let span = tracing::info_span!(
            "command",
            id,
            actor = %ctx.actor,
            correlation = %ctx.correlation,
        );
        let _entered = span.enter();

        let result = self.run(ctx, id, args, Timestamp::now(), 0);
        match &result {
            Ok(_) => {
                tracing::debug!("ok");
                self.notify(id);
            }
            Err(EngineError::Panicked(_)) => tracing::error!("panic caught; nothing was saved"),
            Err(e) => tracing::warn!(code = e.code(), error = %e, "rejected"),
        }
        result
    }

    /// Como `dispatch`, pero a la hora indicada: los tests de leases controlan el reloj.
    #[cfg(any(test, feature = "testing"))]
    pub fn dispatch_at(
        &mut self,
        ctx: &Context,
        id: &str,
        args: &Args,
        now: Timestamp,
    ) -> Result<Value, EngineError> {
        self.run(ctx, id, args, now, 0)
    }

    /// El comando y, si confirmó cambios que disparan automatizaciones,
    /// esas automatizaciones (RF-INT-03), hasta `MAX_AUTOMATION_DEPTH`.
    fn run(
        &mut self,
        ctx: &Context,
        id: &str,
        args: &Args,
        now: Timestamp,
        depth: u8,
    ) -> Result<Value, EngineError> {
        // D15, RF-MCP-11: `as_agent` = actuar como otro agente de la misma
        // interfaz en esta llamada. Se resuelve aquí para todos los frentes.
        if let Some(name) = args.get(AS_AGENT).map(str::trim).filter(|n| !n.is_empty()) {
            let actor = self.read(|r| identity::act_as(r, &ctx.actor, name))?;
            let switched = Context {
                actor,
                correlation: ctx.correlation,
            };
            return self.run(&switched, id, &args.without(AS_AGENT), now, depth);
        }
        let (value, events) = self.run_transaction(ctx, id, args, now)?;
        if !events.is_empty() && depth < MAX_AUTOMATION_DEPTH {
            self.automate(ctx, &events, now, depth);
        }
        Ok(value)
    }

    /// Ejecuta como System las automatizaciones que encajan con los eventos.
    /// Un fallo no deshace el comando que las disparó: queda en el log.
    fn automate(
        &mut self,
        ctx: &Context,
        events: &[(model::Trigger, model::TaskId)],
        now: Timestamp,
        depth: u8,
    ) {
        let planned = match self.read(|r| plan_automations(r, events, now)) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "automations not evaluated");
                return;
            }
        };
        let system = Context {
            actor: Actor::System,
            correlation: ctx.correlation,
        };
        for (name, command, args) in planned {
            match self.run(&system, &command, &args, now, depth + 1) {
                Ok(_) => tracing::info!(automation = %name, command = %command, "automation ran"),
                Err(e) => {
                    tracing::warn!(automation = %name, command = %command, error = %e, "automation failed")
                }
            }
        }
    }

    fn run_transaction(
        &mut self,
        ctx: &Context,
        id: &str,
        args: &Args,
        now: Timestamp,
    ) -> Result<(Value, Vec<(model::Trigger, model::TaskId)>), EngineError> {
        let command = self
            .registry
            .get(id)
            .ok_or_else(|| EngineError::UnknownCommand(id.to_owned()))?;
        args::validate(command.params(), args)?;
        let mut tx = self.store.begin()?;
        match check_actor(&*tx, &ctx.actor)? {
            Caller::Agent(agent, interface) => {
                permissions::check(&*tx, &agent, interface.as_deref(), id, command.mutates())?;
            }
            Caller::Interface(interface) => {
                permissions::check_interface(&interface, id, command.mutates())?;
            }
            Caller::Free => {}
        }
        // Un reintento con la misma clave devuelve la respuesta guardada sin
        // volver a crear nada (RF-MCP-09).
        let idempotency = args
            .get("idempotency_key")
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_owned);
        if let Some(key) = &idempotency
            && let Some((original, response)) = tx.idempotent(&ctx.actor, key)?
        {
            if original != id {
                return Err(EngineError::invalid(
                    i18n::Msg::new("idempotency-key-reused")
                        .arg("key", key)
                        .arg("command", original),
                ));
            }
            return serde_json::from_str(&response)
                .map(|v| (v, Vec::new()))
                .map_err(|e| EngineError::Store(store::StoreError::Corrupt(e.to_string())));
        }

        let home = self.home.as_deref();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            if command.raw() {
                let writer: &mut dyn store::Writer = &mut *tx;
                let mut cx = Cx {
                    tx: writer,
                    ctx,
                    now,
                    history: Vec::new(),
                    home,
                };
                let value = command.run(&mut cx, args)?;
                // La importación no dispara eventos: trae historial ajeno.
                return Ok::<_, EngineError>((value, cx.history, false));
            }
            let mut recorder = Recorder::new(&mut *tx);
            let mut cx = Cx {
                tx: &mut recorder,
                ctx,
                now,
                history: Vec::new(),
                home,
            };
            let value = command.run(&mut cx, args)?;
            // Lo que se terminó y se repite ya tiene su siguiente (D17).
            for id in recorder.completed_tasks() {
                let mut cx = Cx {
                    tx: &mut recorder,
                    ctx,
                    now,
                    history: Vec::new(),
                    home,
                };
                commands::recurrence::spawn_next(&mut cx, id)?;
            }
            let entries = recorder
                .into_changes()
                .into_iter()
                .map(|change| change.into_entry(ctx, id, now))
                .collect();
            Ok((value, entries, true))
        }));
        match outcome {
            Ok(Ok((value, entries, live))) => {
                // El historial entra en la misma transacción que los cambios.
                for entry in &entries {
                    tx.append_activity(entry)?;
                }
                // Y los webhooks se encolan con ellos (RF-INT-02).
                let events = if live {
                    triggers::detect(&entries)
                } else {
                    Vec::new()
                };
                triggers::enqueue(&mut *tx, ctx, id, &events, now)?;
                if let Some(key) = &idempotency {
                    tx.save_idempotent(&ctx.actor, key, id, &value.to_string(), now)?;
                }
                tx.commit()?;
                Ok((value, events))
            }
            // Al descartar `tx` sin commit, el almacenamiento hace rollback.
            Ok(Err(e)) => Err(e),
            Err(_) => Err(EngineError::Panicked(id.to_owned())),
        }
    }

    fn notify(&self, id: &str) {
        let wrote = self.registry.get(id).is_some_and(|c| c.mutates());
        if let (true, Some(listener)) = (wrote, &self.on_commit)
            && let Ok(token) = self.store.change_token()
        {
            listener(token);
        }
    }

    /// Lectura tipada y consistente, sin posibilidad de escribir.
    pub fn read<R>(
        &mut self,
        f: impl FnOnce(&dyn Reader) -> Result<R, EngineError>,
    ) -> Result<R, EngineError> {
        let reader = self.store.read()?;
        f(&*reader)
    }

    /// Cambia cuando alguien confirma una escritura; la UI lo usa para refrescar.
    pub fn change_token(&self) -> Result<u64, EngineError> {
        Ok(self.store.change_token()?)
    }
}

/// Argumento que vale en cualquier comando para actuar como otro agente de
/// la misma interfaz (D15).
pub const AS_AGENT: &str = "as_agent";

/// Cuántas automatizaciones encadenadas se permiten: evita los bucles.
const MAX_AUTOMATION_DEPTH: u8 = 3;

/// Automatizaciones que tocan: (nombre, comando, argumentos ya rellenados).
fn plan_automations(
    r: &dyn Reader,
    events: &[(model::Trigger, model::TaskId)],
    now: Timestamp,
) -> Result<Vec<(String, String, Args)>, EngineError> {
    let rules: Vec<_> = r.automations()?.into_iter().filter(|a| a.enabled).collect();
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let keys = lookup::Keys::load(r)?;
    let mut out = Vec::new();
    for (trigger, id) in events {
        let Some(task) = r.task(*id)? else { continue };
        let key = keys.of(&task);
        let parent = task
            .parent_id
            .map(|p| keys.of_id(r, p))
            .transpose()?
            .unwrap_or_default();
        let project = r
            .project(task.project_id)?
            .map(|p| p.key)
            .unwrap_or_default();
        for a in rules.iter().filter(|a| a.trigger == *trigger) {
            if a.project_id.is_some_and(|p| p != task.project_id) {
                continue;
            }
            if !commands::task_matches(r, now, &task, &a.condition)? {
                continue;
            }
            let args = a.args.iter().fold(Args::none(), |acc, (k, v)| {
                acc.with(k.as_str(), triggers::fill(v, &key, &parent, &project))
            });
            out.push((a.name.clone(), a.command.clone(), args));
        }
    }
    Ok(out)
}

/// Quién llama, para los permisos (D11, D15).
enum Caller {
    /// Supervisor, sistema o integración: sin límites de agente.
    Free,
    Agent(Box<model::Agent>, Option<Box<model::Interface>>),
    Interface(Box<model::Interface>),
}

/// Un agente solo puede actuar si está registrado y activo (RF-ACT-02,
/// RF-ACT-06), y su interfaz también (D15).
fn check_actor(r: &dyn Reader, actor: &Actor) -> Result<Caller, EngineError> {
    let inactive = |status: AgentStatus, name: &str| -> Result<(), EngineError> {
        let key = match status {
            AgentStatus::Active => return Ok(()),
            AgentStatus::Disabled => "agent-disabled",
            AgentStatus::Revoked => "agent-revoked",
        };
        Err(EngineError::forbidden(
            i18n::Msg::new(key).arg("agent", name),
        ))
    };
    match actor {
        Actor::Agent(id) => {
            let agent = r.agent(*id)?.ok_or_else(|| {
                EngineError::forbidden(i18n::Msg::new("agent-unknown").arg("id", id.to_string()))
            })?;
            inactive(agent.status, &agent.name)?;
            let interface = agent
                .interface_id
                .map(|i| r.interface(i))
                .transpose()?
                .flatten();
            if let Some(i) = &interface {
                inactive(i.status, &i.name)?;
            }
            Ok(Caller::Agent(Box::new(agent), interface.map(Box::new)))
        }
        Actor::Interface(id) => {
            let interface = r.interface(*id)?.ok_or_else(|| {
                EngineError::forbidden(i18n::Msg::new("agent-unknown").arg("id", id.to_string()))
            })?;
            inactive(interface.status, &interface.name)?;
            Ok(Caller::Interface(Box::new(interface)))
        }
        _ => Ok(Caller::Free),
    }
}

#[cfg(test)]
mod tests;
