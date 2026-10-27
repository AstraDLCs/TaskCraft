use serde::{Deserialize, Serialize};

use crate::{Timestamp, WorkspaceId};

/// Contenedor raíz: un almacenamiento = un workspace (RF-MOD-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
    pub name: String,
    pub created_at: Timestamp,
    pub settings: Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Profundidad máxima de subtareas; 1 = solo tareas raíz (RF-MOD-04).
    pub max_depth: u32,
    /// Días que una tarea borrada se puede restaurar antes de la purga (RF-HIS-08).
    pub trash_days: u32,
    /// Duración de un claim sin latido (RF-COO-01).
    pub lease_minutes: u32,
    /// Tareas en progreso que puede tener a la vez un agente (RF-COO-07).
    pub agent_wip_limit: u32,
    /// Días sin cambios tras los que una tarea cerrada se archiva sola (RF-HIS-07).
    pub auto_archive_days: u32,
    /// Tamaño máximo de un adjunto, en MB (RF-MOD-17).
    #[serde(default = "default_attachment_mb")]
    pub max_attachment_mb: u32,
    /// Solo lectura para todos los agentes (RF-SUP-08).
    #[serde(default)]
    pub read_only: bool,
    /// Horas entre copias automáticas; 0 = sin copias automáticas (RF-ALM-06).
    #[serde(default = "default_backup_hours")]
    pub backup_hours: u32,
    /// Copias que se conservan; las más antiguas se borran.
    #[serde(default = "default_backup_keep")]
    pub backup_keep: u32,
    /// Semanas sin cambios tras las que una tarea pendiente está envejecida (RF-VIS-16).
    #[serde(default = "default_stale_weeks")]
    pub stale_weeks: u32,
    /// Días hasta el vencimiento en los que una tarea es urgente (RF-VIS-15).
    #[serde(default = "default_urgent_days")]
    pub urgent_days: u32,
}

fn default_attachment_mb() -> u32 {
    10
}

fn default_backup_hours() -> u32 {
    24
}

fn default_backup_keep() -> u32 {
    7
}

fn default_stale_weeks() -> u32 {
    4
}

fn default_urgent_days() -> u32 {
    3
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_depth: 5,
            trash_days: 30,
            lease_minutes: 15,
            agent_wip_limit: 1,
            auto_archive_days: 30,
            max_attachment_mb: default_attachment_mb(),
            read_only: false,
            backup_hours: default_backup_hours(),
            backup_keep: default_backup_keep(),
            stale_weeks: default_stale_weeks(),
            urgent_days: default_urgent_days(),
        }
    }
}
