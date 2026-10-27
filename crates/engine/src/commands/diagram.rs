//! Diagramas Mermaid (RF-MOD-19, D21): por proyecto, con clave `API-D3`. Las
//! tareas los citan con `[[API-D3]]` (enlace) o `![[API-D3]]` (incrustado) en
//! la descripción o en los comentarios. Una fuente que no se puede dibujar se
//! guarda igual y se avisa con `render_error`.

use i18n::Msg;
use model::{Diagram, DiagramId};
use serde_json::{Value, json};
use store::Reader;

use super::paged;
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::views::{diagram_key, diagram_ref};
use crate::{Args, Cx, EngineError};

const THEMES: &[&str] = &["light", "dark"];

command!(Create, "diagram.create", "cmd-diagram-create", create, write, [req "project": K::Text, req "title": K::Text, req "source": K::Text, opt "description": K::Text]);
command!(List, "diagram.list", "cmd-diagram-list", list, read, [opt "project": K::Text, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Get, "diagram.get", "cmd-diagram-get", get, read, [req "diagram": K::Text, opt "project": K::Text, opt "svg": K::Bool, opt "theme": K::Enum(THEMES)]);
command!(Update, "diagram.update", "cmd-diagram-update", update, write, [req "diagram": K::Text, opt "project": K::Text, opt "title": K::Text, opt "source": K::Text, opt "description": K::Text]);
command!(Delete, "diagram.delete", "cmd-diagram-delete", delete, write, [req "diagram": K::Text, opt "project": K::Text]);

/// El diagrama completo: fuente, error de dibujo y tareas que lo citan.
fn view(r: &dyn Reader, d: &Diagram) -> Result<Value, EngineError> {
    let keys = Keys::load(r)?;
    let mut cited_by: Vec<String> = Vec::new();
    for m in r.diagram_mentions_of(d.id)? {
        let key = keys.of_id(r, m.source)?;
        if !cited_by.contains(&key) {
            cited_by.push(key);
        }
    }
    let project = r.project(d.project_id)?.map(|p| p.key).unwrap_or_default();
    Ok(json!({
        "id": d.id,
        "key": diagram_key(r, d)?,
        "project": project,
        "number": d.number,
        "title": d.title,
        "description": d.description,
        "source": d.source,
        "render_error": diagram::check(&d.source).err().map(|e| e.detail()),
        "cited_by": cited_by,
        "created_at": d.created_at,
        "updated_at": d.updated_at,
    }))
}

fn target_project(cx: &Cx<'_>, args: &Args) -> Result<Option<model::ProjectId>, EngineError> {
    args.get("project")
        .map(|p| lookup::project(&*cx.tx, p).map(|p| p.id))
        .transpose()
}

fn find(cx: &Cx<'_>, args: &Args) -> Result<Diagram, EngineError> {
    let project = target_project(cx, args)?;
    lookup::diagram(&*cx.tx, args.require("diagram")?, project)
}

fn title(raw: &str) -> Result<String, EngineError> {
    let title = raw.trim();
    if title.is_empty() || title.chars().count() > 120 {
        return Err(EngineError::arg(
            "title",
            Msg::new("arg-length").arg("min", 1u32).arg("max", 120u32),
        ));
    }
    Ok(title.to_owned())
}

fn source(raw: &str) -> Result<String, EngineError> {
    if raw.len() > diagram::MAX_SOURCE {
        return Err(EngineError::arg(
            "source",
            Msg::new("arg-too-large").arg("max", diagram::MAX_SOURCE),
        ));
    }
    Ok(raw.to_owned())
}

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut project = lookup::project(&*cx.tx, args.require("project")?)?;
    crate::guard::check_scope(cx, project.id)?;
    let diagram = Diagram {
        id: DiagramId::generate(),
        project_id: project.id,
        number: project.next_diagram,
        title: title(args.require("title")?)?,
        description: args.get("description").unwrap_or_default().to_owned(),
        source: source(args.require("source")?)?,
        created_at: cx.now,
        updated_at: cx.now,
    };
    // El número no se reutiliza al borrar: una cita vieja no apunta a otro.
    project.next_diagram = project.next_diagram.saturating_add(1);
    cx.tx.update_project(&project)?;
    cx.tx.insert_diagram(&diagram)?;
    view(&*cx.tx, &diagram)
}

fn list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = target_project(cx, args)?;
    let r: &dyn Reader = &*cx.tx;
    paged(r.diagrams(project)?, args, |d| {
        let mut out = diagram_ref(r, d)?;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("description".into(), json!(d.description));
            obj.insert("updated_at".into(), json!(d.updated_at));
        }
        Ok(out)
    })
}

/// Con `svg=true` incluye el SVG dibujado (tema `light` por defecto).
fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let diagram = find(cx, args)?;
    let mut out = view(&*cx.tx, &diagram)?;
    if args.flag("svg")? {
        let theme = match args.get("theme") {
            Some(t) if t.eq_ignore_ascii_case("dark") => diagram::Theme::Dark,
            _ => diagram::Theme::Light,
        };
        let svg = diagram::render_svg(&diagram.source, theme).ok();
        if let Some(obj) = out.as_object_mut() {
            obj.insert("svg".into(), json!(svg));
        }
    }
    Ok(out)
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut diagram = find(cx, args)?;
    crate::guard::check_scope(cx, diagram.project_id)?;
    if let Some(t) = args.get("title") {
        diagram.title = title(t)?;
    }
    if let Some(s) = args.get("source") {
        diagram.source = source(s)?;
    }
    if let Some(d) = args.get("description") {
        diagram.description = d.to_owned();
    }
    diagram.updated_at = cx.now;
    cx.tx.update_diagram(&diagram)?;
    view(&*cx.tx, &diagram)
}

/// Las citas que quedan en los textos dejan de apuntar a nada.
fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let diagram = find(cx, args)?;
    crate::guard::check_scope(cx, diagram.project_id)?;
    let key = diagram_key(&*cx.tx, &diagram)?;
    cx.tx.delete_diagram(diagram.id)?;
    Ok(json!({ "deleted": diagram.id, "key": key, "title": diagram.title }))
}
