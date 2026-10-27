//! iCalendar (RFC 5545) de las tareas con vencimiento (RF-INT-07, D18):
//! `VTODO` para CalDAV y `VEVENT` para las suscripciones `.ics`.

use std::hash::{DefaultHasher, Hash, Hasher};

use jiff::ToSpan;
use jiff::tz::TimeZone;
use model::{Priority, Status, Task, Timestamp, When};

/// Lo que se publica de una tarea.
pub(crate) struct Item {
    pub key: String,
    pub task: Task,
}

impl Item {
    /// `ETag` de WebDAV: cambia con cada revisión de la tarea.
    pub fn etag(&self) -> String {
        format!("\"{}-{}\"", self.task.id, self.task.revision)
    }
}

/// Texto de iCalendar: `\`, `;`, `,` y saltos de línea escapados.
fn text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// Una línea de contenido plegada a 75 octetos (sin partir caracteres) y
/// terminada en CRLF.
fn line(out: &mut String, content: &str) {
    let mut width = 0;
    for ch in content.chars() {
        let len = ch.len_utf8();
        if width + len > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += len;
    }
    out.push_str("\r\n");
}

fn utc(ts: Timestamp) -> String {
    ts.to_zoned(TimeZone::UTC)
        .strftime("%Y%m%dT%H%M%SZ")
        .to_string()
}

/// `;VALUE=DATE:20261101` o `:20261101T220000Z`.
fn when(w: &When) -> String {
    match w {
        When::Day(d) => format!(";VALUE=DATE:{}", d.strftime("%Y%m%d")),
        When::At(z) => format!(":{}", utc(z.timestamp())),
    }
}

fn status(s: Status) -> &'static str {
    match s {
        Status::Done => "COMPLETED",
        Status::Cancelled => "CANCELLED",
        Status::InProgress | Status::InReview => "IN-PROCESS",
        Status::Backlog | Status::Todo => "NEEDS-ACTION",
    }
}

/// 1 (máxima) … 9; 0 = sin prioridad.
fn priority(p: Priority) -> u8 {
    match p {
        Priority::Urgent => 1,
        Priority::High => 3,
        Priority::Medium => 5,
        Priority::Low => 7,
        Priority::None => 0,
    }
}

fn common(out: &mut String, item: &Item, now: Timestamp) {
    let t = &item.task;
    line(out, &format!("UID:{}@taskcraft", t.id));
    line(out, &format!("DTSTAMP:{}", utc(now)));
    line(out, &format!("CREATED:{}", utc(t.created_at)));
    line(out, &format!("LAST-MODIFIED:{}", utc(t.updated_at)));
    line(out, &format!("SEQUENCE:{}", t.revision));
    line(
        out,
        &format!("SUMMARY:{}", text(&format!("{} {}", item.key, t.title))),
    );
    if !t.description.trim().is_empty() {
        line(out, &format!("DESCRIPTION:{}", text(&t.description)));
    }
    line(out, &format!("PRIORITY:{}", priority(t.priority)));
}

/// Tarea como `VTODO` (CalDAV).
pub(crate) fn todo(item: &Item, now: Timestamp) -> String {
    let mut out = String::new();
    let t = &item.task;
    line(&mut out, "BEGIN:VTODO");
    common(&mut out, item, now);
    if let Some(start) = &t.start {
        line(&mut out, &format!("DTSTART{}", when(start)));
    }
    if let Some(due) = &t.due {
        line(&mut out, &format!("DUE{}", when(due)));
    }
    line(&mut out, &format!("STATUS:{}", status(t.status)));
    if t.status == Status::Done {
        line(&mut out, &format!("COMPLETED:{}", utc(t.updated_at)));
        line(&mut out, "PERCENT-COMPLETE:100");
    }
    line(&mut out, "END:VTODO");
    out
}

/// Tarea como `VEVENT` en su vencimiento (suscripciones `.ics`): de día
/// completo, o de 30 minutos si el vencimiento tiene hora.
pub(crate) fn event(item: &Item, now: Timestamp) -> Option<String> {
    let t = &item.task;
    let due = t.due.as_ref()?;
    let mut out = String::new();
    line(&mut out, "BEGIN:VEVENT");
    common(&mut out, item, now);
    match due {
        When::Day(d) => {
            line(
                &mut out,
                &format!("DTSTART;VALUE=DATE:{}", d.strftime("%Y%m%d")),
            );
            let next = d.checked_add(1.day()).unwrap_or(*d);
            line(
                &mut out,
                &format!("DTEND;VALUE=DATE:{}", next.strftime("%Y%m%d")),
            );
        }
        When::At(z) => {
            line(&mut out, &format!("DTSTART:{}", utc(z.timestamp())));
            line(&mut out, "DURATION:PT30M");
        }
    }
    let transp = if t.status.category().is_closed() {
        "TRANSPARENT"
    } else {
        "OPAQUE"
    };
    line(&mut out, &format!("TRANSP:{transp}"));
    line(&mut out, "END:VEVENT");
    Some(out)
}

/// Un `VCALENDAR` con los componentes dados.
pub(crate) fn calendar(name: &str, components: &[String]) -> String {
    let mut out = String::new();
    line(&mut out, "BEGIN:VCALENDAR");
    line(&mut out, "VERSION:2.0");
    line(&mut out, "PRODID:-//TaskCraft//TaskCraft//ES");
    line(&mut out, "CALSCALE:GREGORIAN");
    line(&mut out, &format!("X-WR-CALNAME:{}", text(name)));
    for c in components {
        out.push_str(c);
    }
    line(&mut out, "END:VCALENDAR");
    out
}

/// `getctag`: cambia cuando cambia cualquier tarea del calendario.
pub(crate) fn ctag(items: &[Item]) -> String {
    let mut h = DefaultHasher::new();
    for i in items {
        i.task.id.to_string().hash(&mut h);
        i.task.revision.hash(&mut h);
    }
    items.len().hash(&mut h);
    format!("\"{:x}\"", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_folds_long_lines() {
        assert_eq!(text("a;b,c\\d\ne"), "a\\;b\\,c\\\\d\\ne");
        let mut out = String::new();
        line(&mut out, &format!("SUMMARY:{}", "ñ".repeat(60)));
        for physical in out.split("\r\n").filter(|l| !l.is_empty()) {
            assert!(physical.len() <= 75, "{} octetos", physical.len());
        }
        let unfolded = out.replace("\r\n ", "");
        assert_eq!(unfolded.trim_end(), format!("SUMMARY:{}", "ñ".repeat(60)));
    }
}
