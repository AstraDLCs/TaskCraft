//! Importes del coste por agente (RF-HIS-06): se guardan en millonésimas
//! para sumar sin errores de coma flotante.

use crate::RuleError;

/// Mayor importe aceptado en un reporte: un millón en la moneda.
const MAX_MICROS: u64 = 1_000_000 * 1_000_000;

/// `0.42`, `12`, `0,0035` → millonésimas. Hasta 6 decimales; no negativos.
pub fn parse_amount(raw: &str) -> Result<u64, RuleError> {
    let bad = || RuleError::Amount(raw.to_owned());
    let text = raw.trim().replace(',', ".");
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if (whole.is_empty() && frac.is_empty()) || !digits(whole) || !digits(frac) || frac.len() > 6 {
        return Err(bad());
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| bad())?
    };
    let frac: u64 = format!("{frac:0<6}").parse().map_err(|_| bad())?;
    let micros = whole
        .checked_mul(1_000_000)
        .and_then(|w| w.checked_add(frac))
        .filter(|m| *m <= MAX_MICROS)
        .ok_or_else(bad)?;
    Ok(micros)
}

/// Millonésimas → texto con al menos 2 decimales y sin ceros sobrantes:
/// `420000` → `0.42`, `3500` → `0.0035`, `12000000` → `12.00`.
pub fn format_amount(micros: u64) -> String {
    let whole = micros / 1_000_000;
    let frac = format!("{:06}", micros % 1_000_000);
    let frac = frac.trim_end_matches('0');
    let frac = if frac.len() < 2 {
        format!("{frac:0<2}")
    } else {
        frac.to_owned()
    };
    format!("{whole}.{frac}")
}

/// Código de moneda ISO 4217: tres letras, en mayúsculas.
pub fn currency(raw: &str) -> Result<String, RuleError> {
    let code = raw.trim().to_ascii_uppercase();
    if code.len() == 3 && code.chars().all(|c| c.is_ascii_uppercase()) {
        Ok(code)
    } else {
        Err(RuleError::Currency(raw.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_round_trip_without_float_errors() {
        assert_eq!(parse_amount("0.42"), Ok(420_000));
        assert_eq!(parse_amount("0,0035"), Ok(3_500));
        assert_eq!(parse_amount("12"), Ok(12_000_000));
        assert_eq!(parse_amount(".5"), Ok(500_000));
        assert_eq!(parse_amount("0.000001"), Ok(1));
        for bad in ["", ".", "-1", "1.0000001", "1e3", "abc", "2000000"] {
            assert!(parse_amount(bad).is_err(), "{bad}");
        }
        assert_eq!(format_amount(420_000), "0.42");
        assert_eq!(format_amount(3_500), "0.0035");
        assert_eq!(format_amount(12_000_000), "12.00");
        assert_eq!(format_amount(0), "0.00");
        assert_eq!(format_amount(1), "0.000001");
        let sum: u64 = ["0.1", "0.2"]
            .iter()
            .map(|a| parse_amount(a).unwrap())
            .sum();
        assert_eq!(format_amount(sum), "0.30");
    }

    #[test]
    fn currency_is_three_letters() {
        assert_eq!(currency(" usd "), Ok("USD".into()));
        assert!(currency("US").is_err());
        assert!(currency("U$D").is_err());
    }
}
