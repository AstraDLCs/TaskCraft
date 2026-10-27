use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;
use crate::{AutomationId, DeliveryId, ProjectId, Timestamp, WebhookId};

text_enum! {
    /// Lo que le pasa a una tarea y puede disparar una automatización o un
    /// webhook (RF-INT-02, RF-INT-03).
    pub enum Trigger ("error-parse-trigger", "trigger") {
        Created = "created" | "creada",
        StatusChanged = "status_changed" | "status" | "estado",
        Completed = "completed" | "done" | "terminada",
        Assigned = "assigned" | "asignada",
        Commented = "commented" | "comentada",
        Escalated = "escalated" | "escalada",
    }
}

/// «Si pasa `trigger` y la tarea cumple `condition` (D12), ejecutar
/// `command` con `args`» como actor System (RF-INT-03). En `args`, `{task}`,
/// `{parent}` y `{project}` se sustituyen por las claves de la tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Automation {
    pub id: AutomationId,
    /// Único, sin distinguir mayúsculas.
    pub name: String,
    pub trigger: Trigger,
    pub condition: String,
    pub command: String,
    pub args: BTreeMap<String, String>,
    pub enabled: bool,
    /// Solo las tareas de este proyecto; `None` = todas.
    pub project_id: Option<ProjectId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Destino de eventos salientes (RF-INT-02): POST con JSON firmado con
/// HMAC-SHA256 usando `secret`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub id: WebhookId,
    pub url: String,
    /// No sale nunca en listados ni en el historial.
    #[serde(skip_serializing, default)]
    pub secret: String,
    pub triggers: Vec<Trigger>,
    pub enabled: bool,
    pub created_at: Timestamp,
}

text_enum! {
    pub enum DeliveryStatus ("error-parse-delivery-status", "delivery-status") {
        Pending = "pending",
        Delivered = "delivered",
        Failed = "failed",
    }
}

/// Un envío pendiente o hecho: la cola persiste en el workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: DeliveryId,
    pub webhook_id: WebhookId,
    pub trigger: Trigger,
    /// El JSON que se envía, ya formado al confirmar el cambio.
    pub payload: String,
    pub attempts: u32,
    pub next_at: Timestamp,
    pub status: DeliveryStatus,
    pub last_error: String,
    pub created_at: Timestamp,
}
