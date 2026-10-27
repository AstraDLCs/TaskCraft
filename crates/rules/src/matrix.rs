//! Matriz de priorización de Eisenhower (RF-VIS-15): importante = prioridad
//! alta o urgente; urgente = vence en los próximos días (o ya venció).

use jiff::ToSpan;
use jiff::civil::Date;
use model::{Priority, Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quadrant {
    /// Importante y urgente: hacer ya.
    DoFirst,
    /// Importante, no urgente: planificar.
    Schedule,
    /// Urgente, no importante: delegar.
    Delegate,
    /// Ni una cosa ni la otra: descartar o aparcar.
    Drop,
}

impl Quadrant {
    /// En el orden de la matriz: fila de arriba (importantes), urgente a la izquierda.
    pub const ALL: [Quadrant; 4] = [
        Quadrant::DoFirst,
        Quadrant::Schedule,
        Quadrant::Delegate,
        Quadrant::Drop,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::DoFirst => "do",
            Self::Schedule => "schedule",
            Self::Delegate => "delegate",
            Self::Drop => "drop",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::DoFirst => "quadrant-do",
            Self::Schedule => "quadrant-schedule",
            Self::Delegate => "quadrant-delegate",
            Self::Drop => "quadrant-drop",
        }
    }

    pub fn important(self) -> bool {
        matches!(self, Self::DoFirst | Self::Schedule)
    }

    pub fn urgent(self) -> bool {
        matches!(self, Self::DoFirst | Self::Delegate)
    }

    fn of(important: bool, urgent: bool) -> Self {
        match (important, urgent) {
            (true, true) => Self::DoFirst,
            (true, false) => Self::Schedule,
            (false, true) => Self::Delegate,
            (false, false) => Self::Drop,
        }
    }
}

pub fn is_important(p: Priority) -> bool {
    matches!(p, Priority::Urgent | Priority::High)
}

/// Vence en `urgent_days` días o menos (o ya venció).
pub fn is_urgent(task: &Task, today: Date, urgent_days: u32) -> bool {
    let limit = today
        .checked_add(i64::from(urgent_days).days())
        .unwrap_or(today);
    task.due.as_ref().is_some_and(|d| d.date() <= limit)
}

pub fn quadrant(task: &Task, today: Date, urgent_days: u32) -> Quadrant {
    Quadrant::of(
        is_important(task.priority),
        is_urgent(task, today, urgent_days),
    )
}

/// Prioridad al llevar una tarea a la fila de las importantes (al menos
/// alta) o a la de las no importantes (como mucho media). `None`: no cambia.
pub fn priority_for(current: Priority, important: bool) -> Option<Priority> {
    match (important, is_important(current)) {
        (true, false) => Some(Priority::High),
        (false, true) => Some(Priority::Medium),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importance_follows_priority_and_urgency_the_due_date() {
        assert!(is_important(Priority::Urgent) && is_important(Priority::High));
        assert!(!is_important(Priority::Medium));
        assert_eq!(priority_for(Priority::Low, true), Some(Priority::High));
        assert_eq!(priority_for(Priority::Urgent, true), None);
        assert_eq!(
            priority_for(Priority::Urgent, false),
            Some(Priority::Medium)
        );
        assert_eq!(priority_for(Priority::Low, false), None);
        assert_eq!(
            Quadrant::ALL.map(|q| (q.important(), q.urgent())),
            [(true, true), (true, false), (false, true), (false, false)]
        );
    }
}
