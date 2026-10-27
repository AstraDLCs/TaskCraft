//! Envío de webhooks (RF-INT-02): el engine deja los envíos en una cola
//! persistente al confirmar cada cambio; aquí se mandan con su firma y se
//! anota el resultado con `webhook.record`, que programa los reintentos.

use std::time::Duration;

use engine::{Args, Context};
use model::{Actor, Delivery, Webhook};

use crate::Shared;

const EVERY: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_secs(10);
const BATCH: usize = 20;

/// Cabecera con la firma: `sha256=<HMAC-SHA256 del cuerpo con el secreto>`.
pub const SIGNATURE_HEADER: &str = "X-TaskCraft-Signature";

pub(crate) fn spawn(shared: Shared) {
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown.subscribe();
        let mut ticker = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = shutdown.changed() => return,
            }
            let engine = shared.engine.clone();
            let ran = tokio::task::spawn_blocking(move || deliver_due(&engine)).await;
            if let Err(e) = ran {
                tracing::error!(error = %e, "webhook sender crashed");
            }
        }
    });
}

/// Envía lo que toca y anota cada resultado. Bloqueante: va en `spawn_blocking`.
pub(crate) fn deliver_due(engine: &engine::SharedEngine) {
    let now = model::Timestamp::now();
    let due = engine.read(|r| Ok((r.deliveries_due(now, BATCH)?, r.webhooks()?)));
    let (due, hooks) = match due {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!(error = %e, "webhook queue not readable");
            return;
        }
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    for d in due {
        let Some(hook) = hooks.iter().find(|h| h.id == d.webhook_id) else {
            continue;
        };
        let result = send(&agent, hook, &d);
        let mut args = Args::none()
            .with("delivery", d.id.to_string())
            .with("ok", result.is_ok().to_string());
        if let Err(e) = &result {
            tracing::info!(url = %hook.url, error = %e, "webhook failed; will retry");
            args = args.with("error", e.as_str());
        }
        if let Err(e) = engine.dispatch(&Context::new(Actor::System), "webhook.record", &args) {
            tracing::warn!(error = %e, "webhook result not recorded");
        }
    }
}

fn send(agent: &ureq::Agent, hook: &Webhook, d: &Delivery) -> Result<(), String> {
    let signature =
        foundation::secret::hmac_sha256_hex(hook.secret.as_bytes(), d.payload.as_bytes());
    agent
        .post(&hook.url)
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            concat!("TaskCraft/", env!("CARGO_PKG_VERSION")),
        )
        .header("X-TaskCraft-Event", d.trigger.as_str())
        .header("X-TaskCraft-Delivery", d.id.to_string())
        .header(SIGNATURE_HEADER, format!("sha256={signature}"))
        .send(d.payload.as_str())
        .map(drop)
        .map_err(|e| e.to_string())
}
