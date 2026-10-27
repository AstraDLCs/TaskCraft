use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use store::Reader;

use i18n::Lang;
use model::Actor;
use protocol::{ArgMap, Dispatch, WireError};

use crate::{Args, CommitListener, Context, Engine, EngineError, Value};

/// Handle clonable a un único `Engine` compartido entre conexiones (D01).
///
/// Serializa los comandos: cada uno observa un estado consistente.
#[derive(Clone)]
pub struct SharedEngine(Arc<Mutex<Engine>>);

impl SharedEngine {
    pub fn new(engine: Engine) -> Self {
        Self(Arc::new(Mutex::new(engine)))
    }

    pub fn dispatch(&self, ctx: &Context, id: &str, args: &Args) -> Result<Value, EngineError> {
        self.lock().dispatch(ctx, id, args)
    }

    /// Como `dispatch`, a una hora dada (tests de leases).
    #[cfg(any(test, feature = "testing"))]
    pub fn dispatch_at(
        &self,
        ctx: &Context,
        id: &str,
        args: &Args,
        now: model::Timestamp,
    ) -> Result<Value, EngineError> {
        self.lock().dispatch_at(ctx, id, args, now)
    }

    pub fn set_on_commit(&self, listener: CommitListener) {
        self.lock().set_on_commit(listener);
    }

    pub fn change_token(&self) -> Result<u64, EngineError> {
        self.lock().change_token()
    }

    pub fn read<R>(
        &self,
        f: impl FnOnce(&dyn Reader) -> Result<R, EngineError>,
    ) -> Result<R, EngineError> {
        self.lock().read(f)
    }

    // `dispatch` captura los pánicos y descarta la transacción antes de soltar
    // el lock, así que un mutex envenenado sigue teniendo datos consistentes.
    fn lock(&self) -> MutexGuard<'_, Engine> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Ejecuta comandos en este proceso como un actor fijo: la CLI `--offline`, el
/// MCP HTTP del daemon y los tests usan la misma interfaz que un cliente.
pub struct LocalDispatch {
    engine: SharedEngine,
    actor: Actor,
}

impl LocalDispatch {
    pub fn new(engine: SharedEngine, actor: Actor) -> Self {
        Self { engine, actor }
    }
}

impl Dispatch for LocalDispatch {
    fn dispatch(&mut self, command: &str, args: &ArgMap, lang: Lang) -> Result<Value, WireError> {
        let ctx = Context::new(self.actor.clone());
        self.engine
            .dispatch(&ctx, command, &Args::from_map(args.clone()))
            .map_err(|e| e.to_wire(lang))
    }
}
