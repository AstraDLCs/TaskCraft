//! Logs con `tracing` (RNF-13). El nivel se configura con `TASKCRAFT_LOG`
//! usando la sintaxis de `EnvFilter` (ej. `TASKCRAFT_LOG=engine=debug`).

use tracing_subscriber::EnvFilter;

pub const LOG_ENV: &str = "TASKCRAFT_LOG";

/// Inicializa los logs a stderr y redirige los pánicos al log.
/// Llamarlo una vez al arrancar cada app; llamadas repetidas no hacen nada.
pub fn init(default_directive: &str) {
    let filter =
        EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| EnvFilter::new(default_directive));
    let installed = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .is_ok();
    if installed {
        std::panic::set_hook(Box::new(|info| tracing::error!(%info, "panic")));
    }
}
