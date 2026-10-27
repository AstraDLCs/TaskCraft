//! Salida legible (RF-INT-01): tablas para listados y `campo: valor` para el
//! detalle. `--json` imprime la respuesta cruda.

use i18n::{Lang, Msg, tr};
use serde_json::Value;

/// Columnas preferidas de una tabla, en orden; se usan las que existan.
const COLUMNS: &[&str] = &[
    "key",
    "name",
    "title",
    "status",
    "priority",
    "kind",
    "claimed_by",
    "connected",
    "due",
    "at",
    "actor_name",
    "command",
    "target",
    "author_name",
    "body",
    "color",
];
const MAX_CELL: usize = 48;

pub fn render(value: &Value, lang: Lang) -> String {
    match value {
        Value::Object(map) => match map.get("items") {
            Some(Value::Array(items)) => {
                let mut out = table(items);
                let total = map.get("total").and_then(Value::as_u64);
                let next = map.get("next_cursor").and_then(Value::as_str);
                if let (Some(total), Some(next)) = (total, next) {
                    out.push_str(&tr(
                        lang,
                        &Msg::new("cli-more")
                            .arg("shown", items.len())
                            .arg("total", total)
                            .arg("cursor", next),
                    ));
                    out.push('\n');
                }
                out
            }
            _ => detail(value),
        },
        other => format!("{}\n", scalar(other)),
    }
}

fn table(items: &[Value]) -> String {
    if items.is_empty() {
        return String::from("—\n");
    }
    let columns: Vec<&str> = COLUMNS
        .iter()
        .copied()
        .filter(|c| {
            items
                .iter()
                .any(|i| i.get(*c).is_some_and(|v| !v.is_null()))
        })
        .take(6)
        .collect();
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|item| {
            columns
                .iter()
                .map(|c| clip(&item.get(*c).map(scalar).unwrap_or_default()))
                .collect()
        })
        .collect();
    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            rows.iter()
                .filter_map(|r| r.get(i))
                .map(|s| s.chars().count())
                .chain([c.len()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    let line = |cells: &[String], out: &mut String| {
        let mut parts = Vec::new();
        for (cell, w) in cells.iter().zip(&widths) {
            parts.push(format!("{cell:<w$}"));
        }
        out.push_str(parts.join("  ").trim_end());
        out.push('\n');
    };
    let header: Vec<String> = columns.iter().map(|c| c.to_uppercase()).collect();
    line(&header, &mut out);
    for r in &rows {
        line(r, &mut out);
    }
    out
}

/// Detalle de un objeto: escalares como `campo: valor`; listas de pasos o
/// criterios con su casilla; el resto, compacto.
fn detail(value: &Value) -> String {
    let Value::Object(map) = value else {
        return format!("{}\n", scalar(value));
    };
    let width = map.keys().map(String::len).max().unwrap_or(0);
    let mut out = String::new();
    for (k, v) in map {
        match v {
            Value::Null => continue,
            Value::Array(items) if items.iter().all(|i| i.get("text").is_some()) => {
                if items.is_empty() {
                    continue;
                }
                out.push_str(&format!("{k}:\n"));
                for i in items {
                    let mark = if i.get("done").and_then(Value::as_bool) == Some(true) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    out.push_str(&format!("  {mark} {}\n", scalar(&i["text"])));
                }
            }
            Value::Array(items) if items.iter().any(Value::is_object) => {
                out.push_str(&format!("{k}: {}\n", items.len()));
            }
            Value::String(s) if s.contains('\n') => {
                out.push_str(&format!("{k}:\n"));
                for l in s.lines() {
                    out.push_str(&format!("  {l}\n"));
                }
            }
            other => out.push_str(&format!("{k:<width$}  {}\n", scalar(other))),
        }
    }
    out
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(scalar).collect::<Vec<_>>().join(", "),
        Value::Object(map) => map
            .get("key")
            .or_else(|| map.get("name"))
            .or_else(|| map.get("id"))
            .map(scalar)
            .unwrap_or_else(|| v.to_string()),
        other => other.to_string(),
    }
}

fn clip(s: &str) -> String {
    let one_line = s.replace('\n', " ");
    if one_line.chars().count() <= MAX_CELL {
        one_line
    } else {
        let cut: String = one_line.chars().take(MAX_CELL - 1).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lists_render_as_tables_with_pagination_hint() {
        let v = json!({
            "count": 2, "total": 3, "next_cursor": "2",
            "items": [
                { "key": "API-1", "title": "Uno", "status": "todo", "labels": [] },
                { "key": "API-2", "title": "Dos", "status": "done", "labels": [] }
            ]
        });
        let out = render(&v, Lang::En);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "KEY    TITLE  STATUS");
        assert_eq!(lines[1], "API-1  Uno    todo");
        assert!(lines[3].contains("--cursor 2"));
    }

    #[test]
    fn details_show_fields_and_checklists() {
        let v = json!({
            "key": "API-1",
            "title": "Uno",
            "checklist": [{ "text": "paso", "done": true }, { "text": "otro", "done": false }],
            "due": null
        });
        let out = render(&v, Lang::Es);
        assert!(out.contains("key        API-1"));
        assert!(out.contains("  [x] paso\n  [ ] otro"));
        assert!(!out.contains("due"));
    }
}
