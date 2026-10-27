//! Tareas periódicas del actor System (D08): expirar leases (RF-COO-03),
//! archivar (RF-HIS-07), purgar la papelera (RF-HIS-08) y copias de
//! seguridad (RF-ALM-06).

use std::time::Duration;

use engine::{Args, Context};
use model::Actor;

use crate::Shared;

const LEASES_EVERY: Duration = Duration::from_secs(30);
const HOURLY: Duration = Duration::from_secs(60 * 60);
const DAILY: Duration = Duration::from_secs(24 * 60 * 60);

pub(crate) fn spawn(shared: Shared) {
    every(shared.clone(), LEASES_EVERY, &["system.expire_leases"]);
    // Cada hora se mira si toca copia según `backup_hours`.
    every(shared.clone(), HOURLY, &["workspace.backup"]);
    every(
        shared,
        DAILY,
        &[
            "task.auto_archive",
            "trash.purge",
            "system.housekeeping",
            "cycle.rollover",
        ],
    );
}

/// Ejecuta los comandos al arrancar y luego cada `period`, hasta el cierre.
fn every(shared: Shared, period: Duration, commands: &'static [&'static str]) {
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown.subscribe();
        let mut ticker = tokio::time::interval(period);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = shutdown.changed() => return,
            }
            for &id in commands {
                let engine = shared.engine.clone();
                let ran = tokio::task::spawn_blocking(move || {
                    engine.dispatch(&Context::new(Actor::System), id, &auto_args(&engine, id))
                })
                .await;
                match ran {
                    Ok(Ok(_)) => tracing::debug!(command = id, "periodic task done"),
                    Ok(Err(e)) => tracing::warn!(command = id, error = %e, "periodic task failed"),
                    Err(e) => tracing::error!(command = id, error = %e, "periodic task crashed"),
                }
            }
        }
    });
}

/// `task.auto_archive` usa los días configurados en el workspace y
/// `workspace.backup` copia solo si toca; los demás, sus valores por defecto.
fn auto_args(engine: &engine::SharedEngine, id: &str) -> Args {
    if id == "workspace.backup" {
        return Args::none().with("auto", "true");
    }
    if id != "task.auto_archive" {
        return Args::none();
    }
    match engine.read(|r| Ok(r.workspace()?.settings.auto_archive_days)) {
        Ok(days) => Args::none().with("days", days.to_string()),
        Err(_) => Args::none(),
    }
}
