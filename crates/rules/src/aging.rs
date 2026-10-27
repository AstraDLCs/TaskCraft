//! Tareas envejecidas (RF-VIS-16): pendientes que nadie toca desde hace semanas.

use jiff::Timestamp;
use model::{Status, Task};

/// Clave de la etapa «Algún día», dentro de «Backlog», a la que se mandan.
pub const SOMEDAY: &str = "someday";

/// ¿Lleva `weeks` semanas o más sin cambios y sigue pendiente (backlog o por
/// hacer, sin estar ya en «Algún día»)?
pub fn is_stale(task: &Task, weeks: u32, now: Timestamp) -> bool {
    let pending = matches!(task.status, Status::Backlog | Status::Todo);
    let parked = task.stage.as_deref() == Some(SOMEDAY);
    let idle = now.duration_since(task.updated_at).as_secs();
    pending
        && !parked
        && task.archived_at.is_none()
        && task.deleted_at.is_none()
        && weeks > 0
        && idle >= i64::from(weeks) * 7 * 24 * 3600
}

/// Semanas completas sin cambios.
pub fn idle_weeks(task: &Task, now: Timestamp) -> i64 {
    now.duration_since(task.updated_at).as_secs() / (7 * 24 * 3600)
}
