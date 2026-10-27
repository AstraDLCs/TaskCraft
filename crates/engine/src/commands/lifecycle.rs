//! Archivado (RF-HIS-07) y papelera (RF-HIS-08). Ambos actúan sobre la tarea
//! y todo su subárbol.

use i18n::Msg;
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::{days_ago, detail, paged, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, views};

command!(Archive, "task.archive", "cmd-task-archive", archive, write, [req "task": K::Text]);
command!(Unarchive, "task.unarchive", "cmd-task-unarchive", unarchive, write, [req "task": K::Text]);
command!(AutoArchive, "task.auto_archive", "cmd-task-auto_archive", auto_archive, write, [opt "days": K::Int]);
command!(Delete, "task.delete", "cmd-task-delete", delete, write, [req "task": K::Text]);
command!(Restore, "task.restore", "cmd-task-restore", restore, write, [req "task": K::Text]);
command!(
    TrashList,
    "trash.list",
    "cmd-trash-list",
    trash_list,
    read,
    [opt "limit": K::Int, opt "cursor": K::Text, opt "fields": K::List]
);
command!(TrashPurge, "trash.purge", "cmd-trash-purge", trash_purge, write, [opt "days": K::Int, opt "backup": K::Bool, opt "dir": K::Text]);

fn archive(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let root = crate::guard::task_for_write(cx, args)?;
    let tree = lookup::subtree(&*cx.tx, root.clone())?;
    let open: Vec<_> = tree
        .iter()
        .filter(|t| t.is_live() && !t.status.category().is_closed())
        .collect();
    if !open.is_empty() {
        let keys = Keys::load(&*cx.tx)?;
        let list: Vec<_> = open.iter().map(|t| keys.of(t)).collect();
        return Err(EngineError::invalid(
            Msg::new("archive-open-tasks")
                .arg("tasks", list.join(", "))
                .arg("count", list.len()),
        ));
    }
    archive_tree(cx, tree)?;
    let root = lookup::task(&*cx.tx, &root.id.to_string())?;
    detail(cx, &root)
}

fn archive_tree(cx: &mut Cx<'_>, tree: Vec<model::Task>) -> Result<(), EngineError> {
    for mut t in tree {
        if t.is_live() {
            t.archived_at = Some(cx.now);
            save(cx, &mut t)?;
        }
    }
    Ok(())
}

fn unarchive(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let root = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_write(cx, &root, None)?;
    if root.archived_at.is_none() {
        return Err(EngineError::invalid(Msg::new("not-archived")));
    }
    if let Some(parent) = root.parent_id.and_then(|id| cx.tx.task(id).ok().flatten())
        && parent.archived_at.is_some()
    {
        return Err(EngineError::invalid(Msg::new("parent-archived")));
    }
    for mut t in lookup::subtree(&*cx.tx, root.clone())? {
        if t.archived_at.is_some() && t.deleted_at.is_none() {
            t.archived_at = None;
            save(cx, &mut t)?;
        }
    }
    let root = lookup::task(&*cx.tx, &root.id.to_string())?;
    detail(cx, &root)
}

fn auto_archive(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let cutoff = days_ago(cx, args.parse("days")?.unwrap_or(30))?;
    let candidates = cx.tx.tasks(&TaskFilter::default())?;
    let mut archived = Vec::new();
    for task in candidates {
        // Puede haberse archivado ya como parte del subárbol de otra.
        let Some(task) = cx.tx.task(task.id)? else {
            continue;
        };
        let parent_open = match task.parent_id {
            Some(id) => cx.tx.task(id)?.is_some_and(|p| p.is_live()),
            None => false,
        };
        if !task.is_live()
            || parent_open
            || !task.status.category().is_closed()
            || task.updated_at > cutoff
        {
            continue;
        }
        let tree = lookup::subtree(&*cx.tx, task)?;
        if tree
            .iter()
            .all(|t| !t.is_live() || t.status.category().is_closed())
        {
            let keys = Keys::load(&*cx.tx)?;
            if let Some(root) = tree.first() {
                archived.push(keys.of(root));
            }
            archive_tree(cx, tree)?;
        }
    }
    Ok(json!({ "count": archived.len(), "archived": archived }))
}

fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let root = lookup::task(&*cx.tx, args.require("task")?)?;
    crate::guard::check_write(cx, &root, None)?;
    let tree = lookup::subtree(&*cx.tx, root.clone())?;
    let mut count = 0;
    for mut t in tree {
        if t.deleted_at.is_none() {
            t.deleted_at = Some(cx.now);
            save(cx, &mut t)?;
            count += 1;
        }
    }
    let keys = Keys::load(&*cx.tx)?;
    Ok(json!({
        "deleted": keys.of(&root),
        "count": count,
        "restore": format!("task.restore task={}", keys.of(&root)),
    }))
}

fn restore(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let root = lookup::task_any(&*cx.tx, args.require("task")?)?;
    crate::guard::check_write(cx, &root, None)?;
    let Some(deleted_at) = root.deleted_at else {
        return Err(EngineError::invalid(Msg::new("not-in-trash")));
    };
    if let Some(parent) = root.parent_id.and_then(|id| cx.tx.task(id).ok().flatten())
        && parent.deleted_at.is_some()
    {
        return Err(EngineError::invalid(Msg::new("parent-in-trash")));
    }
    // Solo vuelve lo que se borró junto con ella, no lo borrado antes por separado.
    for mut t in lookup::subtree(&*cx.tx, root.clone())? {
        if t.deleted_at == Some(deleted_at) {
            t.deleted_at = None;
            save(cx, &mut t)?;
        }
    }
    let root = lookup::task(&*cx.tx, &root.id.to_string())?;
    detail(cx, &root)
}

fn trash_list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let keys = Keys::load(r)?;
    let filter = TaskFilter {
        deleted: true,
        include_archived: true,
        ..TaskFilter::default()
    };
    paged(r.tasks(&filter)?, args, |t| {
        let mut view = views::task_summary(r, &keys, t)?;
        if let Some(obj) = view.as_object_mut() {
            obj.insert("deleted_at".into(), json!(t.deleted_at));
        }
        Ok(view)
    })
}

fn trash_purge(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let days = match args.parse("days")? {
        Some(d) => d,
        None => cx.tx.workspace()?.settings.trash_days,
    };
    let cutoff = days_ago(cx, days)?;
    // Desde la app (Gestión de información) se pide antes la copia (D23); el
    // daemon, al purgar solo lo vencido, no.
    let backup = if args.flag("backup")? {
        Some(super::exchange::safety_copy(cx, args)?)
    } else {
        None
    };
    let filter = TaskFilter {
        deleted: true,
        include_archived: true,
        ..TaskFilter::default()
    };
    let mut purged = 0usize;
    for t in cx.tx.tasks(&filter)? {
        if t.deleted_at.is_some_and(|at| at <= cutoff) && cx.tx.task(t.id)?.is_some() {
            cx.tx.purge_task(t.id)?;
            purged += 1;
        }
    }
    Ok(json!({ "purged": purged, "older_than_days": days, "backup": backup }))
}
