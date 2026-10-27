//! Exportación e importación del formato `.taskcraft` (RF-EXP-01…05, 09).
//! Solo el supervisor; los agentes podrán con el permiso de administrar
//! (RF-ACT-05).

use std::fs::File;
use std::io::BufWriter;

use exchange::{ExchangeError, Manifest, Mode};
use i18n::{Localize, Msg};
use model::{Action, ActivityEntry, ActivityId, EntityKind, FieldChange};
use serde_json::{Value, json};
use store::Reader;

use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup};

command!(Export, "workspace.export", "cmd-workspace-export", export, read, [req "path": K::Text, opt "projects": K::List]);
command!(ImportPreview, "workspace.import_preview", "cmd-workspace-import_preview", preview, read, [req "path": K::Text, opt "mode": K::Enum(&["workspace", "project", "merge"])]);
command!(Import, "workspace.import", "cmd-workspace-import", import, raw, [req "path": K::Text, opt "mode": K::Enum(&["workspace", "project", "merge"])]);
command!(Backup, "workspace.backup", "cmd-workspace-backup", backup, read, [opt "dir": K::Text, opt "auto": K::Bool]);
command!(ExportMarkdown, "workspace.export_markdown", "cmd-workspace-export_markdown", export_markdown, read, [req "dir": K::Text, opt "projects": K::List]);
command!(ExportCsv, "task.export_csv", "cmd-task-export_csv", export_csv, read, [req "path": K::Text, opt "project": K::Text, opt "filter": K::Text, opt "view": K::Text, opt "columns": K::List, opt "archived": K::Bool]);

fn failed(e: ExchangeError) -> EngineError {
    match e {
        ExchangeError::Store(s) => EngineError::Store(s),
        other => EngineError::invalid(other.msg()),
    }
}

fn mode(args: &Args) -> Result<Mode, EngineError> {
    match args.get("mode") {
        None => Ok(Mode::Workspace),
        Some(m) => m.parse().map_err(|_| {
            EngineError::arg(
                "mode",
                Msg::new("arg-one-of")
                    .arg("value", m)
                    .arg("valid", "workspace, project, merge"),
            )
        }),
    }
}

fn export(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let keys = args.list("projects");
    let mut ids = Vec::new();
    for key in &keys {
        ids.push(lookup::project(r, key)?.id);
    }
    let only = (!ids.is_empty()).then_some(ids.as_slice());
    let dump = exchange::collect(r, only)?;
    let manifest = Manifest {
        format: exchange::FORMAT.into(),
        version: exchange::VERSION,
        exported_at: cx.now,
        app_version: env!("CARGO_PKG_VERSION").into(),
        workspace: dump.workspace.name.clone(),
        projects: dump.projects.iter().map(|p| p.key.clone()).collect(),
    };
    let path = args.require("path")?;
    let file = File::create(path).map_err(|e| {
        EngineError::invalid(Msg::new("exchange-io").arg("detail", format!("{path}: {e}")))
    })?;
    exchange::write(BufWriter::new(file), &manifest, &dump).map_err(failed)?;
    Ok(json!({
        "path": path,
        "format_version": exchange::VERSION,
        "projects": manifest.projects,
        "tasks": dump.tasks.len(),
        "comments": dump.comments.len(),
        "activity": dump.activity.len(),
        "agents": dump.agents.len(),
    }))
}

/// Copias de un workspace en `dir`, de la más antigua a la más reciente.
fn backups(dir: &std::path::Path, prefix: &str) -> Vec<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".taskcraft"))
        })
        .collect();
    found.sort();
    found
}

/// Copia de seguridad como `.taskcraft` (RF-ALM-06): sirve para cualquier
/// backend y se restaura importándola. Con `auto=true` (el daemon, cada
/// hora) solo copia si toca según `backup_hours`. Conserva `backup_keep`.
fn backup(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let ws = r.workspace()?;
    let dir = match (args.get("dir"), cx.home) {
        (Some(d), _) => std::path::PathBuf::from(d),
        (None, Some(home)) => home.join("backups"),
        (None, None) => return Err(EngineError::MissingArg("dir".into())),
    };
    let prefix = format!("{}-", exchange_slug(&ws.name));
    if args.flag("auto")? {
        let hours = i64::from(ws.settings.backup_hours);
        // La fecha va en el nombre: `<workspace>-AAAAMMDD-HHMMSS.taskcraft` (UTC).
        let last = backups(&dir, &prefix).last().and_then(|p| {
            let name = p.file_stem()?.to_str()?.strip_prefix(&prefix)?;
            let at = jiff::civil::DateTime::strptime("%Y%m%d-%H%M%S", name).ok()?;
            at.to_zoned(jiff::tz::TimeZone::UTC)
                .ok()
                .map(|z| z.timestamp())
        });
        let due = hours > 0 && last.is_none_or(|l| cx.now.duration_since(l).as_hours() >= hours);
        if !due {
            return Ok(json!({ "skipped": true, "dir": dir.display().to_string() }));
        }
    }
    let (path, dump_tasks) = write_copy(cx, &dir, &prefix)?;
    // Rotación: se borran las más antiguas.
    let all = backups(&dir, &prefix);
    let keep = usize::try_from(ws.settings.backup_keep.max(1)).unwrap_or(usize::MAX);
    let mut removed = 0;
    for old in all.iter().take(all.len().saturating_sub(keep)) {
        if std::fs::remove_file(old).is_ok() {
            removed += 1;
        }
    }
    Ok(json!({
        "path": path.display().to_string(),
        "tasks": dump_tasks,
        "kept": all.len() - removed,
        "removed": removed,
    }))
}

/// Escribe `<dir>/<prefix><AAAAMMDD-HHMMSS>.taskcraft` con todo el workspace
/// tal como está en esta transacción. Devuelve la ruta y cuántas tareas.
fn write_copy(
    cx: &Cx<'_>,
    dir: &std::path::Path,
    prefix: &str,
) -> Result<(std::path::PathBuf, usize), EngineError> {
    let r: &dyn Reader = &*cx.tx;
    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    let stamp = cx
        .now
        .to_zoned(jiff::tz::TimeZone::UTC)
        .strftime("%Y%m%d-%H%M%S")
        .to_string();
    let dump = exchange::collect(r, None)?;
    let manifest = Manifest {
        format: exchange::FORMAT.into(),
        version: exchange::VERSION,
        exported_at: cx.now,
        app_version: env!("CARGO_PKG_VERSION").into(),
        workspace: dump.workspace.name.clone(),
        projects: dump.projects.iter().map(|p| p.key.clone()).collect(),
    };
    // Nunca se pisa una copia: dos en el mismo segundo llevan `-2`, `-3`…
    let mut n = 1u32;
    let (path, file) = loop {
        let name = match n {
            1 => format!("{prefix}{stamp}.taskcraft"),
            _ => format!("{prefix}{stamp}-{n}.taskcraft"),
        };
        let path = dir.join(name);
        match File::create_new(&path) {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => n += 1,
            Err(e) => return Err(io(&path, e)),
        }
    };
    exchange::write(BufWriter::new(file), &manifest, &dump).map_err(failed)?;
    Ok((path, dump.tasks.len()))
}

/// Copia obligatoria antes de eliminar información (D23), con todo tal como
/// estaba: `antes-de-borrar-<workspace>-<fecha>.taskcraft` en `dir` o en la
/// carpeta de copias. No entra en la rotación de las copias automáticas.
pub(super) fn safety_copy(cx: &Cx<'_>, args: &Args) -> Result<String, EngineError> {
    let ws = cx.tx.workspace()?;
    let dir = match (args.get("dir"), cx.home) {
        (Some(d), _) => std::path::PathBuf::from(d),
        (None, Some(home)) => home.join("backups"),
        (None, None) => return Err(EngineError::MissingArg("dir".into())),
    };
    let prefix = format!("antes-de-borrar-{}-", exchange_slug(&ws.name));
    let (path, _) = write_copy(cx, &dir, &prefix)?;
    Ok(path.display().to_string())
}

/// Nombre de archivo a partir del nombre del workspace.
fn exchange_slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() { "workspace".into() } else { s }
}

fn io(path: &std::path::Path, e: std::io::Error) -> EngineError {
    EngineError::invalid(Msg::new("exchange-io").arg("detail", format!("{}: {e}", path.display())))
}

/// Un `.md` por tarea con frontmatter YAML, en carpetas por proyecto
/// (RF-EXP-06). Solo exportación: el formato nativo es `.taskcraft`.
fn export_markdown(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let mut ids = Vec::new();
    for key in args.list("projects") {
        ids.push(lookup::project(r, &key)?.id);
    }
    let only = (!ids.is_empty()).then_some(ids.as_slice());
    let dump = exchange::collect(r, only)?;
    let dir = std::path::Path::new(args.require("dir")?);
    let files = exchange::markdown(&dump);
    for f in &files {
        let path = dir.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        std::fs::write(&path, &f.content).map_err(|e| io(&path, e))?;
    }
    Ok(json!({
        "dir": dir.display().to_string(),
        "files": files.len(),
        "projects": dump.projects.iter().map(|p| p.key.clone()).collect::<Vec<_>>(),
    }))
}

/// Las tareas de una vista guardada o de un filtro, como CSV (RF-EXP-07).
fn export_csv(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let tasks = super::task::select(cx, args)?;
    let mut columns = args.list("columns");
    if columns.is_empty()
        && let Some(view) = args.get("view")
    {
        columns = super::view::find(&*cx.tx, view)?.columns;
    }
    if columns.is_empty() {
        columns = super::view::CSV_COLUMNS
            .iter()
            .map(|c| (*c).to_owned())
            .collect();
    }
    let rows = super::view::rows(cx, &tasks, &columns)?;
    let path = std::path::Path::new(args.require("path")?);
    std::fs::write(path, exchange::csv(&columns, &rows)).map_err(|e| io(path, e))?;
    Ok(json!({
        "path": path.display().to_string(),
        "rows": rows.len(),
        "columns": columns,
    }))
}

fn load(path: &str) -> Result<(Manifest, exchange::Dump), EngineError> {
    let file = File::open(path).map_err(|e| {
        EngineError::invalid(Msg::new("exchange-io").arg("detail", format!("{path}: {e}")))
    })?;
    exchange::read(std::io::BufReader::new(file)).map_err(failed)
}

/// Ciclos entre las dependencias del archivo (los rechazaría `task.link`);
/// al fusionar, junto con las que ya hay en el destino.
fn cycles(
    r: &dyn Reader,
    dump: &exchange::Dump,
    mode: Mode,
) -> Result<Option<String>, EngineError> {
    let mut accepted = match mode {
        Mode::Merge => r.dependencies()?,
        _ => Vec::new(),
    };
    for dep in &dump.dependencies {
        if let Some(cycle) = rules::find_cycle(&accepted, *dep) {
            let path: Vec<String> = cycle.iter().map(ToString::to_string).collect();
            return Ok(Some(path.join(" -> ")));
        }
        accepted.push(*dep);
    }
    Ok(None)
}

/// Qué se crearía, qué colisiona y qué errores hay, sin tocar nada (RF-EXP-05).
fn preview(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mode = mode(args)?;
    let (manifest, dump) = load(args.require("path")?)?;
    let header = json!({
        "mode": mode,
        "format_version": manifest.version,
        "exported_at": manifest.exported_at,
        "workspace": manifest.workspace,
        "projects": manifest.projects,
    });
    let mut out = header;
    let cycle = cycles(&*cx.tx, &dump, mode)?;
    let details = match (cycle, exchange::plan(&*cx.tx, dump, mode)) {
        (Some(cycle), _) => {
            json!({ "can_import": false, "errors": [{ "code": "cycle", "detail": cycle }] })
        }
        (None, Ok(plan)) => json!({
            "can_import": true,
            "creates": plan.creates,
            "updates": plan.updates,
            "collisions": plan.collisions,
            "errors": [],
        }),
        (None, Err(ExchangeError::Invalid(problems))) => {
            json!({ "can_import": false, "errors": problems })
        }
        (None, Err(ExchangeError::Store(e))) => return Err(EngineError::Store(e)),
        (None, Err(other)) => json!({
            "can_import": false,
            "errors": [{ "code": "rejected", "detail": other.to_string() }],
        }),
    };
    if let (Some(o), Value::Object(d)) = (out.as_object_mut(), details) {
        o.extend(d);
    }
    Ok(out)
}

/// Importa en una sola transacción: si algo falla, no queda nada (RF-EXP-05).
fn import(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mode = mode(args)?;
    let (manifest, dump) = load(args.require("path")?)?;
    if let Some(cycle) = cycles(&*cx.tx, &dump, mode)? {
        return Err(EngineError::invalid(
            Msg::new("dependency-cycle").arg("path", cycle),
        ));
    }
    let plan = exchange::plan(&*cx.tx, dump, mode).map_err(failed)?;
    let creates = plan.creates;
    let updates = plan.updates;
    let collisions = plan.collisions.clone();
    // Las menciones se derivan del texto: se recalculan en el destino (RF-REL-04).
    let texts: Vec<(model::TaskId, Option<model::CommentId>, String)> = plan
        .dump
        .tasks
        .iter()
        .map(|t| (t.id, None, t.description.clone()))
        .chain(
            plan.dump
                .comments
                .iter()
                .map(|c| (c.task_id, Some(c.id), c.body.clone())),
        )
        .filter(|(_, _, text)| text.contains("[["))
        .collect();
    let imported = exchange::apply(cx.tx, plan)?;
    for (task, comment, text) in texts {
        super::sync_mentions(cx, task, comment, &text)?;
    }
    cx.history = imported;
    let ws = cx.tx.workspace()?;
    cx.history.push(ActivityEntry {
        id: ActivityId::generate(),
        at: cx.now,
        actor: cx.ctx.actor.clone(),
        correlation: cx.ctx.correlation,
        command: "workspace.import".into(),
        action: Action::Created,
        entity: EntityKind::Workspace,
        entity_id: ws.id.to_string(),
        project_id: None,
        task_id: None,
        changes: vec![FieldChange {
            field: "imported".into(),
            before: Value::Null,
            after: json!({ "from": manifest.workspace, "mode": mode, "creates": creates, "updates": updates }),
        }],
    });
    Ok(json!({ "mode": mode, "creates": creates, "updates": updates, "collisions": collisions }))
}
