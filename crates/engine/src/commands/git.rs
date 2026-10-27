//! Integración con git (RF-INT-04, D16): vínculos de tareas con ramas,
//! commits y PRs, y la ingesta de commits que mueve a revisión lo que cierran.

use i18n::Msg;
use model::{GitKind, GitLink, GitLinkId, Status, Task};
use rules::git::{commit_url, references, subject};
use serde_json::{Value, json};
use store::Reader;

use super::save;
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, guard, views};

command!(GitLinkCmd, "git.link", "cmd-git-link", link, write, [req "task": K::Text, req "kind": K::Enum(&["branch", "commit", "pr"]), req "ref": K::Text, opt "title": K::Text, opt "url": K::Text]);
command!(GitUnlink, "git.unlink", "cmd-git-unlink", unlink, write, [req "task": K::Text, req "kind": K::Enum(&["branch", "commit", "pr"]), req "ref": K::Text]);
command!(GitLinks, "git.links", "cmd-git-links", links, read, [opt "task": K::Text, opt "project": K::Text, opt "kind": K::Enum(&["branch", "commit", "pr"])]);
command!(GitIngest, "git.ingest", "cmd-git-ingest", ingest, write, [req "sha": K::Text, req "message": K::Text, opt "branch": K::Text, opt "remote": K::Text]);

fn new_link(cx: &Cx<'_>, task: &Task, kind: GitKind, reference: &str, title: &str) -> GitLink {
    GitLink {
        id: GitLinkId::generate(),
        task_id: task.id,
        kind,
        reference: reference.trim().to_owned(),
        title: title.trim().to_owned(),
        url: None,
        actor: cx.ctx.actor.clone(),
        at: cx.now,
    }
}

/// Un SHA completo de git: 40 (SHA-1) o 64 (SHA-256) dígitos hexadecimales.
fn check_sha(sha: &str) -> Result<String, EngineError> {
    let sha = sha.trim().to_ascii_lowercase();
    if matches!(sha.len(), 40 | 64) && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(sha)
    } else {
        Err(EngineError::arg(
            "sha",
            Msg::new("git-bad-sha").arg("value", sha.as_str()),
        ))
    }
}

fn link(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    guard::check_scope(cx, task.project_id)?;
    let kind: GitKind = args.parse_required("kind")?;
    let reference = match kind {
        GitKind::Commit => check_sha(args.require("ref")?)?,
        _ => args.require("ref")?.trim().to_owned(),
    };
    if reference.is_empty() {
        return Err(EngineError::MissingArg("ref".into()));
    }
    let mut l = new_link(
        cx,
        &task,
        kind,
        &reference,
        args.get("title").unwrap_or_default(),
    );
    l.url = args
        .get("url")
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned);
    // Un PR sin URL aparte: si la referencia es una URL, es su enlace.
    if l.url.is_none() && kind == GitKind::Pr && reference.starts_with("http") {
        l.url = Some(reference.clone());
    }
    let created = cx.tx.insert_git_link(&l)?;
    let r: &dyn Reader = &*cx.tx;
    let mut out = views::git_link(r, &Keys::load(r)?, &l)?;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("created".into(), json!(created));
    }
    Ok(out)
}

fn unlink(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let task = lookup::editable_task(&*cx.tx, args.require("task")?)?;
    guard::check_scope(cx, task.project_id)?;
    let kind: GitKind = args.parse_required("kind")?;
    let reference = args.require("ref")?.trim().to_ascii_lowercase();
    let found = cx.tx.git_links(Some(task.id))?.into_iter().find(|l| {
        l.kind == kind
            && (l.reference.to_ascii_lowercase() == reference
                || (kind == GitKind::Commit
                    && reference.len() >= 7
                    && l.reference.starts_with(&reference)))
    });
    let removed = match found {
        Some(l) => cx.tx.delete_git_link(l.id)?,
        None => false,
    };
    Ok(json!({ "removed": removed }))
}

fn links(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let task = args
        .get("task")
        .map(|t| lookup::task_any(r, t))
        .transpose()?;
    let project = args
        .get("project")
        .map(|p| lookup::project(r, p))
        .transpose()?;
    let kind: Option<GitKind> = args.parse("kind")?;
    let keys = Keys::load(r)?;
    let mut items = Vec::new();
    for l in r.git_links(task.as_ref().map(|t| t.id))? {
        if kind.is_some_and(|k| k != l.kind) {
            continue;
        }
        if let Some(p) = &project
            && r.task(l.task_id)?.is_none_or(|t| t.project_id != p.id)
        {
            continue;
        }
        items.push(views::git_link(r, &keys, &l)?);
    }
    Ok(json!({ "items": items }))
}

/// Un commit (D16): vincula las tareas que menciona, la rama a las tareas
/// cuya clave lleva, y pasa a revisión las que cierra.
fn ingest(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let sha = check_sha(args.require("sha")?)?;
    let message = args.require("message")?;
    let title = subject(message).to_owned();
    let url = args.get("remote").and_then(|r| commit_url(r, &sha));
    let mut linked = Vec::new();
    let mut moved = Vec::new();
    let mut not_moved = Vec::new();
    let mut unknown = Vec::new();
    for reference in references(message) {
        let Some(task) = known_task(cx, &reference.key)? else {
            unknown.push(reference.key);
            continue;
        };
        if let Err(e) = guard::check_scope(cx, task.project_id) {
            not_moved.push(json!({ "task": reference.key, "reason": e.key() }));
            continue;
        }
        let mut l = new_link(cx, &task, GitKind::Commit, &sha, &title);
        l.url = url.clone();
        let created = cx.tx.insert_git_link(&l)?;
        linked.push(reference.key.clone());
        // Un commit ya ingerido no mueve nada otra vez.
        if reference.closes && created {
            match to_review(cx, task) {
                Ok(true) => moved.push(reference.key),
                Ok(false) => {}
                Err(e) => not_moved.push(json!({ "task": reference.key, "reason": e.key() })),
            }
        }
    }
    let mut branch_linked = Vec::new();
    if let Some(branch) = args.get("branch").map(str::trim).filter(|b| !b.is_empty()) {
        for reference in references(branch) {
            let Some(task) = known_task(cx, &reference.key)? else {
                continue;
            };
            if guard::check_scope(cx, task.project_id).is_err() {
                continue;
            }
            let l = new_link(cx, &task, GitKind::Branch, branch, "");
            if cx.tx.insert_git_link(&l)? {
                branch_linked.push(reference.key);
            }
        }
    }
    Ok(json!({
        "sha": sha,
        "linked": linked,
        "moved": moved,
        "not_moved": not_moved,
        "unknown": unknown,
        "branch_linked": branch_linked,
    }))
}

/// La tarea viva con esa clave, si existe.
fn known_task(cx: &Cx<'_>, key: &str) -> Result<Option<Task>, EngineError> {
    match lookup::editable_task(&*cx.tx, key) {
        Ok(t) => Ok(Some(t)),
        Err(EngineError::NotFound(_)) => Ok(None),
        Err(e) if e.key() == "task-in-trash" || e.key() == "task-archived" => Ok(None),
        Err(e) => Err(e),
    }
}

/// Pasa la tarea a revisión como haría su ejecutor al entregar (D16): por en
/// curso si estaba por hacer, ronda de revisión nueva y sin claim. `false` si
/// ya estaba en revisión o cerrada.
fn to_review(cx: &mut Cx<'_>, mut task: Task) -> Result<bool, EngineError> {
    guard::check_write(cx, &task, None)?;
    let path: &[&str] = match task.status {
        Status::Backlog | Status::Todo => &["in_progress", "in_review"],
        Status::InProgress => &["in_review"],
        Status::InReview | Status::Done | Status::Cancelled => return Ok(false),
    };
    let project = cx
        .tx
        .project(task.project_id)?
        .ok_or_else(|| EngineError::not_found(Msg::new("notfound-task-project")))?;
    let mut steps = Vec::new();
    let mut from = task.status;
    for key in path {
        let (to, stage) = rules::resolve_status(&project, key)?;
        rules::check_transition(from, to)?;
        steps.push((to, stage));
        from = to;
    }
    for (to, stage) in steps {
        task.status = to;
        task.stage = stage;
        if to == Status::InReview {
            task.claim = None;
        }
        save(cx, &mut task)?;
    }
    Ok(true)
}
