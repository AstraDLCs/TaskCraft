//! Formatos de texto solo de salida: Markdown con frontmatter YAML, un
//! archivo por tarea (RF-EXP-06), y CSV (RF-EXP-07).

use std::collections::HashMap;
use std::fmt::Write as _;

use model::{Actor, AgentId, CheckItem, EpicId, LabelId, ProjectId, TaskId};

use crate::Dump;

/// Un archivo Markdown: ruta relativa (`API/API-7-migrar-el-esquema.md`) y contenido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownFile {
    pub path: String,
    pub content: String,
}

/// Nombre de archivo seguro y legible: minúsculas, ASCII y guiones.
fn file_slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.trim().to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            other => other,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 60 {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Un texto como escalar YAML: una cadena JSON es YAML válido.
fn yaml(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
}

struct Names<'a> {
    projects: HashMap<ProjectId, &'a str>,
    labels: HashMap<LabelId, &'a str>,
    agents: HashMap<AgentId, &'a str>,
    epics: HashMap<EpicId, &'a str>,
    keys: HashMap<TaskId, String>,
}

impl<'a> Names<'a> {
    fn of(d: &'a Dump) -> Self {
        let projects: HashMap<_, _> = d.projects.iter().map(|p| (p.id, p.key.as_str())).collect();
        let keys = d
            .tasks
            .iter()
            .map(|t| {
                let p = projects.get(&t.project_id).copied().unwrap_or("?");
                (t.id, format!("{p}-{}", t.number))
            })
            .collect();
        Self {
            projects,
            labels: d.labels.iter().map(|l| (l.id, l.name.as_str())).collect(),
            agents: d.agents.iter().map(|a| (a.id, a.name.as_str())).collect(),
            epics: d.epics.iter().map(|e| (e.id, e.name.as_str())).collect(),
            keys,
        }
    }

    fn actor(&self, a: &Actor) -> String {
        match a {
            Actor::Agent(id) => self
                .agents
                .get(id)
                .map_or_else(|| id.to_string(), |n| (*n).to_owned()),
            other => other.to_string(),
        }
    }

    fn key(&self, id: TaskId) -> String {
        self.keys
            .get(&id)
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }
}

fn checklist(out: &mut String, title: &str, items: &[CheckItem]) {
    if items.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n## {title}\n");
    for i in items {
        let mark = if i.done { 'x' } else { ' ' };
        let _ = writeln!(out, "- [{mark}] {}", i.text);
    }
}

/// Un `.md` por tarea, en una carpeta por proyecto. El frontmatter lleva
/// los metadatos; el cuerpo, la descripción, la checklist, los criterios y
/// los comentarios. Los títulos de sección van en inglés: el archivo es
/// para git y herramientas, no para la interfaz.
pub fn markdown(d: &Dump) -> Vec<MarkdownFile> {
    let names = Names::of(d);
    let mut comments: HashMap<TaskId, Vec<&model::Comment>> = HashMap::new();
    for c in &d.comments {
        comments.entry(c.task_id).or_default().push(c);
    }
    d.tasks
        .iter()
        .filter(|t| t.deleted_at.is_none())
        .map(|t| {
            let key = names.key(t.id);
            let project = names.projects.get(&t.project_id).copied().unwrap_or("?");
            let mut out = String::from("---\n");
            let mut field = |name: &str, value: String| {
                let _ = writeln!(out, "{name}: {value}");
            };
            field("key", yaml(&key));
            field("title", yaml(&t.title));
            field("status", yaml(t.status.as_str()));
            if let Some(stage) = &t.stage {
                field("stage", yaml(stage));
            }
            field("priority", yaml(t.priority.as_str()));
            if !t.assignees.is_empty() {
                let list: Vec<String> = t.assignees.iter().map(|a| yaml(&names.actor(a))).collect();
                field("assignees", format!("[{}]", list.join(", ")));
            }
            let labels: Vec<String> = t
                .labels
                .iter()
                .filter_map(|l| names.labels.get(l))
                .map(|l| yaml(l))
                .collect();
            field("labels", format!("[{}]", labels.join(", ")));
            if let Some(p) = t.parent_id {
                field("parent", yaml(&names.key(p)));
            }
            for (name, group) in [("epic", t.epic_id), ("milestone", t.milestone_id)] {
                if let Some(e) = group.and_then(|g| names.epics.get(&g)) {
                    field(name, yaml(e));
                }
            }
            if let Some(s) = &t.start {
                field("start", yaml(&s.to_string()));
            }
            if let Some(due) = &t.due {
                field("due", yaml(&due.to_string()));
            }
            if let Some(e) = t.estimate {
                field("estimate", e.as_f64().to_string());
            }
            if !t.fields.is_empty() {
                out.push_str("fields:\n");
                for (k, v) in &t.fields {
                    let _ = writeln!(out, "  {k}: {}", yaml(v));
                }
            }
            let mut field = |name: &str, value: String| {
                let _ = writeln!(out, "{name}: {value}");
            };
            field("created", yaml(&t.created_at.to_string()));
            field("updated", yaml(&t.updated_at.to_string()));
            if t.archived_at.is_some() {
                field("archived", "true".into());
            }
            out.push_str("---\n\n");
            let _ = writeln!(out, "# {}", t.title);
            if !t.description.trim().is_empty() {
                let _ = writeln!(out, "\n{}", t.description.trim_end());
            }
            checklist(&mut out, "Checklist", &t.checklist);
            checklist(&mut out, "Acceptance criteria", &t.criteria);
            if let Some(list) = comments.get(&t.id) {
                let _ = writeln!(out, "\n## Comments");
                for c in list {
                    let _ = writeln!(
                        out,
                        "\n**{}** · {}\n\n{}",
                        names.actor(&c.author),
                        c.created_at,
                        c.body.trim_end()
                    );
                }
            }
            let slug = file_slug(&t.title);
            let name = if slug.is_empty() {
                key.clone()
            } else {
                format!("{key}-{slug}")
            };
            MarkdownFile {
                path: format!("{project}/{name}.md"),
                content: out,
            }
        })
        .collect()
}

/// Una celda CSV (RFC 4180): entre comillas si hace falta.
fn cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Tabla CSV con cabecera; filas separadas por CRLF.
pub fn csv(header: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for line in std::iter::once(header).chain(rows.iter().map(Vec::as_slice)) {
        let cells: Vec<String> = line.iter().map(|c| cell(c)).collect();
        out.push_str(&cells.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_only_when_needed() {
        let out = csv(
            &["key".into(), "title".into()],
            &[vec!["API-1".into(), "dice \"hola\", y se va".into()]],
        );
        assert_eq!(out, "key,title\r\nAPI-1,\"dice \"\"hola\"\", y se va\"\r\n");
    }

    #[test]
    fn slugs_are_ascii() {
        assert_eq!(
            file_slug("Migrar el esquema: índices ¡ya!"),
            "migrar-el-esquema-indices-ya"
        );
        assert_eq!(yaml("a: \"b\""), r#""a: \"b\"""#);
    }
}
