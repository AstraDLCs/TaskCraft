use std::fmt;
use std::str::FromStr;

use jiff::Zoned;
use jiff::civil::Date;

use crate::serde_str::serde_as_str;

/// Fecha de inicio o vencimiento: día completo, o instante con zona horaria
/// (RF-MOD-09). Texto: `2026-11-01` o `2026-11-01T17:00:00-05:00[America/Lima]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    Day(Date),
    At(Zoned),
}

impl When {
    /// Día civil, para ordenar y comparar con "hoy".
    pub fn date(&self) -> Date {
        match self {
            Self::Day(d) => *d,
            Self::At(z) => z.date(),
        }
    }
}

impl fmt::Display for When {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Day(d) => d.fmt(f),
            Self::At(z) => z.fmt(f),
        }
    }
}

impl FromStr for When {
    type Err = jiff::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains('T') {
            s.parse().map(Self::At)
        } else {
            s.parse().map(Self::Day)
        }
    }
}

serde_as_str!(When);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_forms() {
        for text in ["2026-11-01", "2026-11-01T17:00:00-05:00[America/Lima]"] {
            let when: When = text.parse().unwrap();
            assert_eq!(when.to_string(), text);
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!("mañana".parse::<When>().is_err());
    }
}
