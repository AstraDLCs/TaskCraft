//! Tareas envejecidas (RF-VIS-16): pendientes que nadie toca desde hace
//! semanas, y su destino natural, la etapa «Algún día».

use model::{Status, StatusDef};
use rules::SOMEDAY;
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::{detail, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, guard, views};

command!(Stale, "task.stale", "cmd-task-stale", stale, read, [opt "project": K::Text, opt "weeks": K::Int]);
command!(Someday, "task.someday", "cmd-task-someday", someday, write, [req "task": K::Text, opt "revision": K::Int]);

fn stale(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let weeks = match args.parse::<u32>("weeks")? {
        Some(w) => w.clamp(1, 520),
        None => r.workspace()?.settings.stale_weeks,
    };
    let filter = match args.get("project") {
        Some(p) => TaskFilter::project(lookup::project(r, p)?.id),
        None => TaskFilter::default(),
    };
    let keys = Keys::load(r)?;
    let mut tasks: Vec<_> = r
        .tasks(&filter)?
        .into_iter()
        .filter(|t| rules::is_stale(t, weeks, cx.now))
        .collect();
    tasks.sort_by_key(|t| t.updated_at);
    let mut items = Vec::new();
    for t in &tasks {
        items.push(json!({
            "key": keys.of(t),
            "title": t.title,
            "status": t.status,
            "stage": t.stage,
            "updated_at": t.updated_at,
            "idle_weeks": rules::idle_weeks(t, cx.now),
            "assignee": t.assignee().map(|a| views::actor_name(r, a)).transpose()?,
        }));
    }
    Ok(json!({ "weeks": weeks, "items": items }))
}

/// La pasa a «Algún día» (una etapa de «Backlog»), que se crea en el
/// proyecto si aún no existe.
fn someday(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    let mut project = cx
        .tx
        .project(task.project_id)?
        .ok_or_else(|| EngineError::not_found(i18n::Msg::new("notfound-task-project")))?;
    rules::check_transition(task.status, Status::Backlog)?;
    let mut flow = rules::workflow(&project);
    if !flow.iter().any(|d| d.key == SOMEDAY) {
        let at = flow
            .iter()
            .rposition(|d| d.base == Status::Backlog)
            .map_or(0, |i| i + 1);
        flow.insert(at, StatusDef::new(SOMEDAY, String::new(), Status::Backlog));
        project.workflow = flow;
        project.updated_at = cx.now;
        cx.tx.update_project(&project)?;
    }
    task.status = Status::Backlog;
    task.stage = Some(SOMEDAY.to_owned());
    save(cx, &mut task)?;
    detail(cx, &task)
}
