//! Adjuntos (RF-MOD-17): archivos ligados a una tarea, con un tamaño máximo
//! configurable. Por MCP llegan en base64 o como ruta de un archivo local.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use i18n::Msg;
use model::{Attachment, AttachmentId};
use serde_json::{Value, json};

use super::paged;
use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError, lookup, views};

command!(Add, "attachment.add", "cmd-attachment-add", add, write, [req "task": K::Text, opt "name": K::Text, opt "data": K::Text, opt "path": K::Text, opt "mime": K::Text, opt "idempotency_key": K::Text]);
command!(List, "attachment.list", "cmd-attachment-list", list_attachments, read, [req "task": K::Text, opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]);
command!(Get, "attachment.get", "cmd-attachment-get", get, read, [req "attachment": K::Text, opt "path": K::Text]);
command!(Remove, "attachment.remove", "cmd-attachment-remove", remove, write, [req "attachment": K::Text]);

pub(crate) fn view(r: &dyn store::Reader, a: &Attachment) -> Result<Value, EngineError> {
    Ok(json!({
        "id": a.id,
        "name": a.name,
        "mime": a.mime,
        "size": a.size,
        "sha256": a.sha256,
        "author": views::actor_name(r, &a.author)?,
        "created_at": a.created_at,
    }))
}

/// Tipo MIME por la extensión; lo que no se reconoce es binario.
fn mime_of(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" => "text/html",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn io_error(path: &str, e: std::io::Error) -> EngineError {
    EngineError::invalid(Msg::new("exchange-io").arg("detail", format!("{path}: {e}")))
}

fn add(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = crate::guard::task_for_write(cx, args)?;
    let (data, default_name) = match (args.get("data"), args.get("path")) {
        (Some(b64), None) => {
            let bytes = STANDARD
                .decode(b64.trim())
                .map_err(|_| EngineError::arg("data", Msg::new("attachment-bad-base64")))?;
            (bytes, None)
        }
        (None, Some(path)) => {
            let bytes = std::fs::read(path).map_err(|e| io_error(path, e))?;
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            (bytes, name)
        }
        _ => return Err(EngineError::invalid(Msg::new("attachment-source"))),
    };
    let name = args
        .get("name")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .or(default_name)
        .ok_or_else(|| EngineError::MissingArg("name".into()))?;
    let max_mb = cx.tx.workspace()?.settings.max_attachment_mb;
    let limit = u64::from(max_mb).saturating_mul(1024 * 1024);
    let size = u64::try_from(data.len()).unwrap_or(u64::MAX);
    if size > limit {
        return Err(EngineError::invalid(
            Msg::new("attachment-too-big")
                .arg("size", size / 1024)
                .arg("max", max_mb),
        ));
    }
    let attachment = Attachment {
        id: AttachmentId::generate(),
        task_id: task.id,
        mime: args
            .get("mime")
            .map(str::to_owned)
            .unwrap_or_else(|| mime_of(&name).to_owned()),
        name,
        size,
        sha256: foundation::secret::sha256_hex(&data),
        author: cx.ctx.actor.clone(),
        created_at: cx.now,
    };
    cx.tx.insert_attachment(&attachment, &data)?;
    view(&*cx.tx, &attachment)
}

fn list_attachments(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::task(&*cx.tx, args.require("task")?)?;
    let r: &dyn store::Reader = &*cx.tx;
    paged(r.attachments(task.id)?, args, |a| view(r, a))
}

fn find(cx: &Cx<'_>, reference: &str) -> Result<Attachment, EngineError> {
    let not_found =
        || EngineError::not_found(Msg::new("notfound-attachment").arg("attachment", reference));
    let id: AttachmentId = reference.parse().map_err(|_| not_found())?;
    cx.tx.attachment(id)?.ok_or_else(not_found)
}

/// Con `path=` lo guarda en ese archivo; si no, lo devuelve en base64.
fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let a = find(cx, args.require("attachment")?)?;
    let data = cx.tx.attachment_data(&a.sha256)?.unwrap_or_default();
    let mut out = view(&*cx.tx, &a)?;
    let extra = match args.get("path") {
        Some(path) => {
            std::fs::write(path, &data).map_err(|e| io_error(path, e))?;
            json!({ "saved_to": path })
        }
        None => json!({ "data": STANDARD.encode(&data) }),
    };
    if let (Some(o), Value::Object(e)) = (out.as_object_mut(), extra) {
        o.extend(e);
    }
    Ok(out)
}

fn remove(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let a = find(cx, args.require("attachment")?)?;
    let task = cx.tx.task(a.task_id)?.ok_or_else(|| {
        EngineError::not_found(Msg::new("notfound-task").arg("task", a.task_id.to_string()))
    })?;
    crate::guard::check_write(cx, &task, None)?;
    cx.tx.delete_attachment(a.id)?;
    Ok(json!({ "deleted": a.id, "name": a.name }))
}
