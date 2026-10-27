use i18n::Msg;
use serde_json::Value;

use crate::command::Kind as K;
use crate::command::command;
use crate::{Args, Cx, EngineError};

command!(Get, "workspace.get", "cmd-workspace-get", get, read, []);
command!(WebToken, "workspace.web_token", "cmd-workspace-web_token", web_token, write, [opt "revoke": K::Bool]);
command!(Configure, "workspace.configure", "cmd-workspace-configure", configure, write, [opt "max_depth": K::Int, opt "trash_days": K::Int, opt "lease_minutes": K::Int, opt "agent_wip_limit": K::Int, opt "auto_archive_days": K::Int, opt "max_attachment_mb": K::Int, opt "read_only": K::Bool, opt "backup_hours": K::Int, opt "backup_keep": K::Int, opt "stale_weeks": K::Int, opt "urgent_days": K::Int]);

fn get(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    Ok(serde_json::to_value(cx.tx.workspace()?).unwrap_or_default())
}

fn configure(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    super::agent::supervisor_only(cx)?;
    let mut ws = cx.tx.workspace()?;
    if args.get("read_only").is_some() {
        ws.settings.read_only = args.flag("read_only")?;
    }
    if let Some(depth) = args.parse::<u32>("max_depth")? {
        if !(1..=20).contains(&depth) {
            return Err(EngineError::arg(
                "max_depth",
                Msg::new("arg-range").arg("min", 1u32).arg("max", 20u32),
            ));
        }
        ws.settings.max_depth = depth;
    }
    if let Some(days) = args.parse::<u32>("trash_days")? {
        if !(1..=3650).contains(&days) {
            return Err(EngineError::arg(
                "trash_days",
                Msg::new("arg-range").arg("min", 1u32).arg("max", 3650u32),
            ));
        }
        ws.settings.trash_days = days;
    }
    for (name, max, slot) in [
        ("lease_minutes", 1440, &mut ws.settings.lease_minutes),
        ("agent_wip_limit", 50, &mut ws.settings.agent_wip_limit),
        (
            "auto_archive_days",
            3650,
            &mut ws.settings.auto_archive_days,
        ),
        (
            "max_attachment_mb",
            1024,
            &mut ws.settings.max_attachment_mb,
        ),
        ("backup_keep", 365, &mut ws.settings.backup_keep),
        ("stale_weeks", 520, &mut ws.settings.stale_weeks),
        ("urgent_days", 365, &mut ws.settings.urgent_days),
    ] {
        if let Some(value) = args.parse::<u32>(name)? {
            if !(1..=max).contains(&value) {
                return Err(EngineError::arg(
                    name,
                    Msg::new("arg-range").arg("min", 1u32).arg("max", max),
                ));
            }
            *slot = value;
        }
    }
    if let Some(hours) = args.parse::<u32>("backup_hours")? {
        if hours > 720 {
            return Err(EngineError::arg(
                "backup_hours",
                Msg::new("arg-range").arg("min", 0u32).arg("max", 720u32),
            ));
        }
        ws.settings.backup_hours = hours;
    }
    cx.tx.update_workspace(&ws)?;
    Ok(serde_json::to_value(ws).unwrap_or_default())
}

/// Archivo con el SHA-256 del token web del supervisor (D14), junto a la base.
pub const WEB_TOKEN_FILE: &str = "web-token.sha256";

/// Emite (o anula) el token con el que el supervisor usa la app web (D14).
/// Se muestra una sola vez; solo se guarda su hash, fuera de la base.
fn web_token(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    super::agent::supervisor_only(cx)?;
    let home = cx
        .home
        .ok_or_else(|| EngineError::invalid(Msg::new("web-token-home")))?;
    let path = home.join(WEB_TOKEN_FILE);
    let failed = |e: std::io::Error| {
        EngineError::invalid(
            Msg::new("exchange-io").arg("detail", format!("{}: {e}", path.display())),
        )
    };
    if args.flag("revoke")? {
        if path.exists() {
            std::fs::remove_file(&path).map_err(failed)?;
        }
        return Ok(serde_json::json!({ "revoked": true }));
    }
    let token = foundation::secret::generate_token()
        .ok_or_else(|| EngineError::invalid(Msg::new("token-random")))?;
    std::fs::create_dir_all(home).map_err(failed)?;
    write_private(&path, &foundation::secret::hash_token(&token)).map_err(failed)?;
    Ok(serde_json::json!({ "token": token, "file": path.display().to_string() }))
}

/// Solo legible por el usuario (0600).
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)?.write_all(text.as_bytes())
}
