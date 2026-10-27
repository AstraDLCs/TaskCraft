use serde::{Deserialize, Serialize};

use crate::text_enum::text_enum;

/// Regla de repetición de una tarea (RF-MOD-18, D17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Recurrence {
    /// Por calendario, contando desde el vencimiento anterior.
    Every {
        unit: RepeatUnit,
        interval: u16,
        /// Días de la semana (1 = lunes … 7 = domingo); solo con `week`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weekdays: Vec<u8>,
        /// Día del mes (1–31; en un mes más corto, el último); solo con `month`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        month_day: Option<u8>,
    },
    /// `days` días después de terminarla.
    After { days: u16 },
}

text_enum! {
    pub enum RepeatUnit ("error-parse-repeat-unit", "repeat-unit") {
        Day = "day" | "days" | "dia" | "día" | "dias" | "días",
        Week = "week" | "weeks" | "semana" | "semanas",
        Month = "month" | "months" | "mes" | "meses",
        Year = "year" | "years" | "año" | "años",
    }
}
