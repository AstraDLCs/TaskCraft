//! Conexión de la app con los datos (D08): lee de una réplica de solo lectura
//! y escribe a través del daemon. En tests, o si no hay daemon, lectura y
//! escritura van a un engine dentro del proceso.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use engine::{Args, Engine, EngineError, LocalDispatch, Registry, SharedEngine, Value};
use i18n::Lang;
use model::Actor;
use protocol::{Dispatch, WireError};
use store::{Reader, Store};

enum Reads {
    /// Conexión de solo lectura al mismo almacenamiento que usa el daemon.
    Replica(Box<dyn Store>),
    Engine(SharedEngine),
}

/// Abre otra réplica de solo lectura (para el hilo de carga).
pub type ReplicaFactory = Box<dyn Fn() -> Result<Box<dyn Store>, store::StoreError> + Send>;
/// Despierta la ventana cuando hay algo nuevo que dibujar.
pub type Waker = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

/// Hilo que construye la réplica de la app con su propia conexión, para que
/// una recarga grande (50 000 tareas) no congele la ventana (RNF-06).
struct Loader {
    requests: Sender<crate::data::Load>,
    results: Receiver<Result<crate::data::Snapshot, String>>,
}

pub struct Conn {
    reads: Reads,
    writes: Box<dyn Dispatch>,
    /// Lo pone a `true` el hilo de eventos del daemon (RNF-08).
    notified: Arc<AtomicBool>,
    loader: Option<Loader>,
    waker: Waker,
}

impl Conn {
    /// Todo dentro del proceso, como supervisor (tests y modo sin daemon).
    pub fn local(engine: Engine) -> Self {
        let shared = SharedEngine::new(engine);
        Self {
            writes: Box::new(LocalDispatch::new(shared.clone(), Actor::Supervisor)),
            reads: Reads::Engine(shared),
            notified: Arc::new(AtomicBool::new(false)),
            loader: None,
            waker: Waker::default(),
        }
    }

    /// Réplica local para leer y el daemon (u otro `Dispatch`) para escribir.
    pub fn remote(replica: impl Store + 'static, writes: Box<dyn Dispatch>) -> Self {
        Self {
            reads: Reads::Replica(Box::new(replica)),
            writes,
            notified: Arc::new(AtomicBool::new(false)),
            loader: None,
            waker: Waker::default(),
        }
    }

    /// Carga la réplica de la app en un hilo con su propia conexión. Si no
    /// puede abrirla, la carga sigue siendo síncrona.
    pub fn with_background_loader(mut self, open: ReplicaFactory) -> Self {
        let (requests, inbox) = channel::<crate::data::Load>();
        let (outbox, results) = channel();
        let waker = self.waker.clone();
        let spawned = std::thread::Builder::new()
            .name("taskcraft-loader".into())
            .spawn(move || {
                let Ok(mut store) = open() else { return };
                while let Ok(mut opts) = inbox.recv() {
                    // Si llegaron varias peticiones, solo importa la última.
                    while let Ok(newer) = inbox.try_recv() {
                        opts = newer;
                    }
                    let snap = match store.read() {
                        Ok(r) => crate::data::build(&*r, &opts).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    };
                    if outbox.send(snap).is_err() {
                        return;
                    }
                    if let Some(wake) = waker
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .as_ref()
                    {
                        wake();
                    }
                }
            });
        if spawned.is_ok() {
            self.loader = Some(Loader { requests, results });
        }
        self
    }

    /// Qué hacer para despertar la ventana (el hilo de carga y el de eventos).
    pub fn set_waker(&self, wake: Box<dyn Fn() + Send>) {
        *self.waker.lock().unwrap_or_else(PoisonError::into_inner) = Some(wake);
    }

    /// Pide una réplica nueva al hilo de carga; `false` si no hay hilo.
    pub fn request_snapshot(&self, opts: &crate::data::Load) -> bool {
        self.loader
            .as_ref()
            .is_some_and(|l| l.requests.send(opts.clone()).is_ok())
    }

    /// La última réplica que terminó el hilo de carga, si hay alguna nueva.
    pub fn take_snapshot(&mut self) -> Option<Result<crate::data::Snapshot, String>> {
        let loader = self.loader.as_ref()?;
        let mut latest = None;
        loop {
            match loader.results.try_recv() {
                Ok(s) => latest = Some(s),
                Err(TryRecvError::Empty) => return latest,
                Err(TryRecvError::Disconnected) => {
                    self.loader = None;
                    return latest;
                }
            }
        }
    }

    /// `false` si lee y escribe en este proceso (sin daemon).
    pub fn is_remote(&self) -> bool {
        matches!(self.reads, Reads::Replica(_))
    }

    /// Bandera que el hilo de eventos pone a `true` cuando algo cambia.
    pub fn notifier(&self) -> Arc<AtomicBool> {
        self.notified.clone()
    }

    /// ¿Llegó un evento desde la última vez que se preguntó?
    pub fn take_notification(&self) -> bool {
        self.notified.swap(false, Ordering::SeqCst)
    }

    pub fn read<R>(
        &mut self,
        f: impl FnOnce(&dyn Reader) -> Result<R, EngineError>,
    ) -> Result<R, EngineError> {
        match &mut self.reads {
            Reads::Replica(store) => {
                let reader = store.read()?;
                f(&*reader)
            }
            Reads::Engine(engine) => engine.read(f),
        }
    }

    /// Cambia con cada escritura confirmada, venga de donde venga.
    pub fn change_token(&self) -> Option<u64> {
        match &self.reads {
            Reads::Replica(store) => store.change_token().ok(),
            Reads::Engine(engine) => engine.change_token().ok(),
        }
    }

    pub fn dispatch(&mut self, id: &str, args: &Args, lang: Lang) -> Result<Value, WireError> {
        self.writes.dispatch(id, &args.to_map(), lang)
    }
}

/// Metadata de los comandos (paleta Ctrl+K): la misma en la app y el daemon,
/// que se compilan juntos.
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::standard)
}
