//! Reglas de repetición (RF-MOD-18, D17): cómo se escriben, qué fecha toca
//! después y cómo se leen.

use i18n::{Lang, Msg, t, tr};
use jiff::civil::Date;
use jiff::{Span, ToSpan};
use model::{Recurrence, RepeatUnit};

use crate::RuleError;

/// Límite de `interval` y de `after`: más de esto es un error de tecleo.
const MAX_INTERVAL: u16 = 366;

fn bad(value: &str) -> RuleError {
    RuleError::Recurrence(value.to_owned())
}

/// `3d`, `2w`, `1m`, `1y` (también `3`, en días) → (cantidad, unidad).
fn amount(text: &str) -> Option<(u16, RepeatUnit)> {
    let text = text.trim().to_lowercase();
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let n: u16 = digits.parse().ok()?;
    let unit = match &text[digits.len()..] {
        "" | "d" => RepeatUnit::Day,
        "w" | "s" => RepeatUnit::Week,
        "m" => RepeatUnit::Month,
        "y" | "a" => RepeatUnit::Year,
        other => other.parse().ok()?,
    };
    (1..=MAX_INTERVAL).contains(&n).then_some((n, unit))
}

/// `mon`, `lunes`, `jue`… → 1 (lunes) … 7 (domingo).
fn weekday(text: &str) -> Option<u8> {
    let t = text.trim().to_lowercase();
    let prefix = |names: &[&str]| names.iter().any(|n| t.len() >= 2 && n.starts_with(&t));
    [
        (1, &["monday", "lunes"][..]),
        (2, &["tuesday", "martes"]),
        (3, &["wednesday", "miercoles", "miércoles"]),
        (4, &["thursday", "jueves"]),
        (5, &["friday", "viernes"]),
        (6, &["saturday", "sabado", "sábado"]),
        (7, &["sunday", "domingo"]),
    ]
    .into_iter()
    .find(|(_, names)| prefix(names))
    .map(|(n, _)| n)
}

/// Regla a partir de los argumentos de `task.set_recurrence`: `every`
/// (`week` o `2w`) con `interval`, `on` y `day`, o `after` (`3d`).
pub fn parse(
    every: Option<&str>,
    interval: Option<u16>,
    on: Option<&str>,
    day: Option<u8>,
    after: Option<&str>,
) -> Result<Recurrence, RuleError> {
    match (every, after) {
        (Some(_), Some(a)) => Err(bad(a)),
        (None, Some(a)) => {
            let (n, unit) = amount(a).ok_or_else(|| bad(a))?;
            let days = match unit {
                RepeatUnit::Day => n,
                RepeatUnit::Week => n.saturating_mul(7),
                RepeatUnit::Month => n.saturating_mul(30),
                RepeatUnit::Year => n.saturating_mul(365),
            };
            Ok(Recurrence::After { days })
        }
        (Some(e), None) => {
            let (n, unit) = match e.trim().parse::<RepeatUnit>() {
                Ok(unit) => (1, unit),
                Err(_) => amount(e).ok_or_else(|| bad(e))?,
            };
            let interval = interval.unwrap_or(n);
            if !(1..=MAX_INTERVAL).contains(&interval) {
                return Err(bad(&interval.to_string()));
            }
            let mut weekdays = Vec::new();
            if let Some(on) = on.filter(|o| !o.trim().is_empty()) {
                if unit != RepeatUnit::Week {
                    return Err(bad(on));
                }
                for name in on.split(',') {
                    let d = weekday(name).ok_or_else(|| bad(name))?;
                    if !weekdays.contains(&d) {
                        weekdays.push(d);
                    }
                }
                weekdays.sort_unstable();
            }
            let month_day = match day {
                Some(d) if unit != RepeatUnit::Month || !(1..=31).contains(&d) => {
                    return Err(bad(&d.to_string()));
                }
                other => other,
            };
            Ok(Recurrence::Every {
                unit,
                interval,
                weekdays,
                month_day,
            })
        }
        (None, None) => Err(bad("")),
    }
}

/// Mismo día en otro mes; si no existe, el último del mes.
fn clamp_day(year: i16, month: i8, day: i8) -> Option<Date> {
    let first = Date::new(year, month, 1).ok()?;
    Date::new(year, month, day.min(first.days_in_month())).ok()
}

fn add_months(base: Date, months: i32, day: i8) -> Option<Date> {
    let total = i32::from(base.year()) * 12 + i32::from(base.month() - 1) + months;
    let year = i16::try_from(total.div_euclid(12)).ok()?;
    let month = i8::try_from(total.rem_euclid(12) + 1).ok()?;
    clamp_day(year, month, day)
}

fn monday_of(d: Date) -> Date {
    d.checked_sub(Span::new().days(i64::from(d.weekday().to_monday_zero_offset())))
        .unwrap_or(d)
}

/// Siguiente fecha por calendario estrictamente posterior a `base`.
fn step(rule: &Recurrence, base: Date) -> Option<Date> {
    let Recurrence::Every {
        unit,
        interval,
        weekdays,
        month_day,
    } = rule
    else {
        return None;
    };
    let n = i64::from(*interval);
    match unit {
        RepeatUnit::Day => base.checked_add(n.days()).ok(),
        RepeatUnit::Week if weekdays.is_empty() => base.checked_add((7 * n).days()).ok(),
        RepeatUnit::Week => {
            // El día marcado siguiente, en semanas que tocan (cada `interval`).
            let week0 = monday_of(base);
            let mut d = base.checked_add(1.day()).ok()?;
            for _ in 0..(7 * n + 7) {
                let weeks = i64::from((monday_of(d) - week0).get_days() / 7);
                let wd = d.weekday().to_monday_one_offset();
                if weeks % n == 0 && weekdays.iter().any(|w| i8::try_from(*w) == Ok(wd)) {
                    return Some(d);
                }
                d = d.checked_add(1.day()).ok()?;
            }
            None
        }
        RepeatUnit::Month => {
            let day = month_day.map_or(base.day(), |d| i8::try_from(d).unwrap_or(31));
            let months = i32::from(*interval);
            // Con `day` posterior al de la base en el mismo mes, ese mes cuenta.
            let same = clamp_day(base.year(), base.month(), day).filter(|d| *d > base);
            match same {
                Some(d) if month_day.is_some() && *interval == 1 => Some(d),
                _ => add_months(base, months, day),
            }
        }
        RepeatUnit::Year => {
            let year = base.year().checked_add(i16::try_from(*interval).ok()?)?;
            clamp_day(year, base.month(), base.day())
        }
    }
}

/// Vencimiento de la repetición siguiente (D17). `previous` es el
/// vencimiento de la que se terminó; `done` el día en que se terminó.
pub fn next_due(rule: &Recurrence, previous: Option<Date>, done: Date) -> Date {
    match rule {
        Recurrence::After { days } => done.checked_add(i64::from(*days).days()).unwrap_or(done),
        Recurrence::Every { .. } => {
            let mut d = previous.unwrap_or(done);
            // Si se terminó tarde, la primera fecha que todavía no pasó.
            for _ in 0..10_000 {
                match step(rule, d) {
                    Some(next) if next > done => return next,
                    Some(next) => d = next,
                    None => break,
                }
            }
            done.checked_add(1.day()).unwrap_or(done)
        }
    }
}

/// Texto de la regla para personas: «Cada 2 semanas (lun, jue)».
pub fn describe(rule: &Recurrence, lang: Lang) -> String {
    match rule {
        Recurrence::After { days } => tr(
            lang,
            &Msg::new("recurrence-after").arg("n", u32::from(*days)),
        ),
        Recurrence::Every {
            unit,
            interval,
            weekdays,
            month_day,
        } => {
            let n = u32::from(*interval);
            let base = tr(
                lang,
                &match unit {
                    RepeatUnit::Day => Msg::new("recurrence-every-day"),
                    RepeatUnit::Week => Msg::new("recurrence-every-week"),
                    RepeatUnit::Month => Msg::new("recurrence-every-month"),
                    RepeatUnit::Year => Msg::new("recurrence-every-year"),
                }
                .arg("n", n),
            );
            match (weekdays.is_empty(), month_day) {
                (false, _) => {
                    let days: Vec<String> =
                        weekdays.iter().map(|d| t(lang, weekday_key(*d))).collect();
                    tr(
                        lang,
                        &Msg::new("recurrence-on-days")
                            .arg("base", base)
                            .arg("days", days.join(", ")),
                    )
                }
                (true, Some(d)) => tr(
                    lang,
                    &Msg::new("recurrence-on-day")
                        .arg("base", base)
                        .arg("day", u32::from(*d)),
                ),
                (true, None) => base,
            }
        }
    }
}

/// Clave del nombre corto de un día (1 = lunes).
pub fn weekday_key(d: u8) -> &'static str {
    match d {
        1 => "weekday-1",
        2 => "weekday-2",
        3 => "weekday-3",
        4 => "weekday-4",
        5 => "weekday-5",
        6 => "weekday-6",
        _ => "weekday-7",
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn every(e: &str, on: Option<&str>, day: Option<u8>) -> Recurrence {
        parse(Some(e), None, on, day, None).unwrap()
    }

    #[test]
    fn parses_the_documented_forms() {
        assert_eq!(
            every("week", Some("thu,lun"), None),
            Recurrence::Every {
                unit: RepeatUnit::Week,
                interval: 1,
                weekdays: vec![1, 4],
                month_day: None
            }
        );
        assert_eq!(
            every("2w", None, None),
            Recurrence::Every {
                unit: RepeatUnit::Week,
                interval: 2,
                weekdays: vec![],
                month_day: None
            }
        );
        assert_eq!(
            parse(None, None, None, None, Some("2w")).unwrap(),
            Recurrence::After { days: 14 }
        );
        for (e, on, day, after) in [
            (Some("month"), Some("mon"), None, None),
            (Some("week"), None, Some(1), None),
            (Some("month"), None, Some(32), None),
            (Some("fortnight"), None, None, None),
            (Some("day"), None, None, Some("3d")),
            (None, None, None, None),
            (Some("0d"), None, None, None),
        ] {
            assert!(
                parse(e, None, on, day, after).is_err(),
                "{e:?} {on:?} {day:?}"
            );
        }
    }

    #[test]
    fn calendar_rules_count_from_the_previous_due_date() {
        let done = date(2026, 1, 1);
        // Diario y cada 2 semanas.
        let d = every("day", None, None);
        assert_eq!(next_due(&d, Some(date(2026, 1, 1)), done), date(2026, 1, 2));
        let w2 = every("2w", None, None);
        assert_eq!(
            next_due(&w2, Some(date(2026, 1, 5)), done),
            date(2026, 1, 19)
        );
        // Lunes y jueves: del lunes 5 al jueves 8, del jueves al lunes 12.
        let mt = every("week", Some("mon,thu"), None);
        assert_eq!(
            next_due(&mt, Some(date(2026, 1, 5)), done),
            date(2026, 1, 8)
        );
        assert_eq!(
            next_due(&mt, Some(date(2026, 1, 8)), done),
            date(2026, 1, 12)
        );
        // Cada 2 semanas los lunes: salta la semana siguiente.
        let biweekly = parse(Some("week"), Some(2), Some("mon"), None, None).unwrap();
        assert_eq!(
            next_due(&biweekly, Some(date(2026, 1, 5)), done),
            date(2026, 1, 19)
        );
    }

    #[test]
    fn months_and_years_clamp_to_the_last_day() {
        let done = date(2026, 1, 1);
        let m31 = every("month", None, Some(31));
        assert_eq!(
            next_due(&m31, Some(date(2026, 1, 31)), done),
            date(2026, 2, 28)
        );
        assert_eq!(
            next_due(&m31, Some(date(2026, 2, 28)), done),
            date(2026, 3, 31)
        );
        let first = every("month", None, Some(1));
        assert_eq!(
            next_due(&first, Some(date(2026, 1, 15)), done),
            date(2026, 2, 1)
        );
        let m = every("month", None, None);
        assert_eq!(
            next_due(&m, Some(date(2026, 1, 31)), done),
            date(2026, 2, 28)
        );
        let y = every("year", None, None);
        assert_eq!(
            next_due(&y, Some(date(2028, 2, 29)), done),
            date(2029, 2, 28)
        );
        assert_eq!(next_due(&y, Some(date(2028, 2, 29)), done).year(), 2029);
    }

    #[test]
    fn late_completion_skips_to_a_future_date_and_after_counts_from_done() {
        let weekly = every("week", Some("mon"), None);
        // Vencía el lunes 5 de enero y se terminó el miércoles 21.
        assert_eq!(
            next_due(&weekly, Some(date(2026, 1, 5)), date(2026, 1, 21)),
            date(2026, 1, 26)
        );
        let after = Recurrence::After { days: 3 };
        assert_eq!(
            next_due(&after, Some(date(2026, 1, 1)), date(2026, 1, 10)),
            date(2026, 1, 13)
        );
        // Sin vencimiento previo cuenta desde el día en que se terminó.
        assert_eq!(
            next_due(&every("day", None, None), None, date(2026, 1, 10)),
            date(2026, 1, 11)
        );
    }
}
