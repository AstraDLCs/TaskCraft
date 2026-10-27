//! Avisos a los clientes suscritos a recursos (RF-MCP-10): con cada escritura
//! confirmada se vuelven a leer los recursos suscritos de la sesión y se avisa
//! solo de los que cambiaron.
//!
//! Sirve a las dos formas del protocolo: `resources/subscribe` (la clásica) y
//! `subscriptions/listen` (2026-07-28).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

use model::Actor;
use tokio::sync::broadcast;

use crate::{Backend, resources};

/// Recursos suscritos y la huella de su última lectura.
#[derive(Default)]
pub(crate) struct Watched(HashMap<String, u64>);

pub(crate) type Shared = Arc<Mutex<Watched>>;

impl Watched {
    pub fn insert(&mut self, uri: String, digest: u64) {
        self.0.insert(uri, digest);
    }

    pub fn remove(&mut self, uri: &str) {
        self.0.remove(uri);
    }

    fn uris(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Guarda la huella nueva; ¿era distinta?
    fn update(&mut self, uri: &str, digest: u64) -> bool {
        match self.0.get_mut(uri) {
            Some(old) if *old == digest => false,
            Some(old) => {
                *old = digest;
                true
            }
            // Se canceló mientras se leía.
            None => false,
        }
    }
}

/// Huella del contenido de un recurso; `None` si no se puede leer.
pub(crate) async fn digest(
    backend: &Arc<dyn Backend>,
    caller: Option<Actor>,
    uri: &str,
) -> Option<u64> {
    let read = resources::read(backend, caller, uri).await.ok()?;
    let text = serde_json::to_string(&read).ok()?;
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    Some(h.finish())
}

/// Espera la siguiente escritura y junta las que lleguen a la vez. `false`
/// cuando el canal se cerró (el daemon se fue).
async fn next_change(rx: &mut broadcast::Receiver<u64>) -> bool {
    match rx.recv().await {
        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {
            while !matches!(
                rx.try_recv(),
                Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed)
            ) {}
            true
        }
        Err(broadcast::error::RecvError::Closed) => false,
    }
}

/// Bucle de una sesión: tras cada escritura, `notify` recibe cada recurso
/// suscrito que cambió. Termina si `notify` falla (el cliente se fue) o si
/// el canal de cambios se cierra.
pub(crate) async fn run<F, Fut>(
    backend: Arc<dyn Backend>,
    caller: Option<Actor>,
    watched: Shared,
    mut changes: broadcast::Receiver<u64>,
    notify: F,
) where
    F: Fn(String) -> Fut,
    Fut: Future<Output = bool>,
{
    while next_change(&mut changes).await {
        let uris = watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .uris();
        for uri in uris {
            let Some(d) = digest(&backend, caller.clone(), &uri).await else {
                continue;
            };
            let changed = watched
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .update(&uri, d);
            if changed && !notify(uri).await {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use engine::{Args, Context, Engine, SharedEngine};
    use protocol::{ArgMap, WireError};
    use serde_json::Value;
    use store_sqlite::SqliteStore;
    use tokio::sync::mpsc;

    use super::*;

    /// Backend del supervisor que avisa de cada escritura del engine.
    struct Sup(SharedEngine, broadcast::Sender<u64>);

    impl Backend for Sup {
        fn call(&self, _: Option<Actor>, command: &str, args: &ArgMap) -> Result<Value, WireError> {
            self.0
                .dispatch(
                    &Context::supervisor(),
                    command,
                    &Args::from_map(args.clone()),
                )
                .map_err(|e| e.to_wire(i18n::Lang::En))
        }

        fn changes(&self) -> Option<broadcast::Receiver<u64>> {
            Some(self.1.subscribe())
        }
    }

    fn update(engine: &SharedEngine, task: &str, title: &str) {
        engine
            .dispatch(
                &Context::supervisor(),
                "task.update",
                &Args::none().with("task", task).with("title", title),
            )
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_the_resources_that_changed_are_notified() {
        let engine = SharedEngine::new(Engine::new(SqliteStore::open_in_memory().unwrap()));
        let sup = Context::supervisor();
        engine
            .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
            .unwrap();
        for title in ["uno", "dos", "tres"] {
            engine
                .dispatch(
                    &sup,
                    "task.create",
                    &Args::none().with("project", "API").with("title", title),
                )
                .unwrap();
        }
        let (tx, _) = broadcast::channel(16);
        let forward = tx.clone();
        engine.set_on_commit(Box::new(move |token| {
            let _ = forward.send(token);
        }));
        let backend: Arc<dyn Backend> = Arc::new(Sup(engine.clone(), tx));

        let watched = Shared::default();
        for uri in ["taskcraft://task/API-1", "taskcraft://task/API-2"] {
            let d = digest(&backend, None, uri).await.unwrap();
            watched.lock().unwrap().insert(uri.into(), d);
        }
        let (seen, mut got) = mpsc::unbounded_channel();
        let changes = backend.changes().unwrap();
        tokio::spawn(run(backend, None, watched, changes, move |uri| {
            let seen = seen.clone();
            async move { seen.send(uri).is_ok() }
        }));
        let next = async |got: &mut mpsc::UnboundedReceiver<String>| {
            tokio::time::timeout(Duration::from_secs(5), got.recv())
                .await
                .unwrap()
                .unwrap()
        };

        // Una tarea no suscrita no avisa; la suscrita, una vez.
        update(&engine, "API-3", "cambia sin suscriptores");
        update(&engine, "API-1", "cambia");
        assert_eq!(next(&mut got).await, "taskcraft://task/API-1");
        update(&engine, "API-2", "cambia también");
        assert_eq!(next(&mut got).await, "taskcraft://task/API-2");
        assert!(got.try_recv().is_err(), "nada más");
    }
}
