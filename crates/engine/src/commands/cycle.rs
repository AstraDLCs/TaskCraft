//! Ciclos o sprints (RF-INT-05): periodos de un proyecto con sus tareas. Una
//! tarea puede estar en varios. Al cerrar uno, lo que quedó abierto y no está
//! en otro ciclo abierto pasa también al siguiente, que se crea con la misma
//! duración si no existe. El daemon cierra solos los ciclos vencidos.

use i18n::Msg;
use jiff::ToSpan;
use jiff::civil::Date;
use model::{Cycle, CycleId, CycleState, Project, Task};
use serde_json::{Value, json};
use store::{Reader, TaskFilter};

use super::agent::supervisor_only;
use super::{detail, save};
use crate::command::Kind as K;
use crate::command::command;
use crate::lookup::{self, Keys};
use crate::{Args, Cx, EngineError, guard};

command!(CycleCreate, "cycle.create", "cmd-cycle-create", create, write, [req "project": K::Text, opt "name": K::Text, opt "start": K::Date, opt "end": K::Date, opt "days": K::Int]);
command!(CycleList, "cycle.list", "cmd-cycle-list", list, read, [req "project": K::Text, opt "closed": K::Bool]);
command!(CycleGet, "cycle.get", "cmd-cycle-get", get, read, [req "cycle": K::Text, opt "project": K::Text]);
command!(CycleUpdate, "cycle.update", "cmd-cycle-update", update, write, [req "cycle": K::Text, opt "project": K::Text, opt "name": K::Text, opt "start": K::Date, opt "end": K::Date]);
command!(CycleClose, "cycle.close", "cmd-cycle-close", close, write, [req "cycle": K::Text, opt "project": K::Text]);
command!(CycleDelete, "cycle.delete", "cmd-cycle-delete", delete, write, [req "cycle": K::Text, opt "project": K::Text]);
command!(
    CycleRollover,
    "cycle.rollover",
    "cmd-cycle-rollover",
    rollover,
    write,
    []
);
command!(SetCycle, "task.set_cycle", "cmd-task-set_cycle", set_cycle, write, [req "task": K::Text, req "cycle": K::Text, opt "revision": K::Int]);
command!(AddCycle, "task.add_cycle", "cmd-task-add_cycle", add_cycle, write, [req "task": K::Text, req "cycle": K::Text, opt "revision": K::Int]);
command!(RemoveCycle, "task.remove_cycle", "cmd-task-remove_cycle", remove_cycle, write, [req "task": K::Text, req "cycle": K::Text, opt "revision": K::Int]);

/// Duración por defecto de un ciclo, en días.
const DEFAULT_DAYS: i64 = 14;
const MAX_DAYS: i64 = 366;

fn today(cx: &Cx<'_>) -> Date {
    super::time::day_of(cx.now)
}

fn day(args: &Args, key: &str) -> Result<Option<Date>, EngineError> {
    super::time::parse_day(args, key)
}

/// Días del ciclo, ambos extremos incluidos.
fn length(c: &Cycle) -> i64 {
    i64::from((c.end - c.start).get_days()) + 1
}

fn add(d: Date, days: i64) -> Date {
    d.checked_add(days.days()).unwrap_or(d)
}

/// Nombre del ciclo siguiente: `Sprint 3` → `Sprint 4`; si no termina en
/// número, se le añade ` 2`.
fn next_name(previous: &str) -> String {
    let digits: String = previous
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    match digits.parse::<u64>() {
        Ok(n) => format!("{}{}", &previous[..previous.len() - digits.len()], n + 1),
        Err(_) => format!("{previous} 2"),
    }
}

/// Un ciclo por ULID, por nombre dentro del proyecto, o `current`/`next`.
pub(crate) fn find(
    r: &dyn Reader,
    reference: &str,
    project: Option<&Project>,
    today: Date,
) -> Result<Cycle, EngineError> {
    let not_found = || EngineError::not_found(Msg::new("notfound-cycle").arg("cycle", reference));
    if let Ok(id) = reference.parse::<CycleId>() {
        return r.cycle(id)?.ok_or_else(not_found);
    }
    let cycles = r.cycles(project.map(|p| p.id))?;
    let wanted = reference.trim().to_lowercase();
    let open = cycles.iter().filter(|c| c.closed_at.is_none());
    let found = match wanted.as_str() {
        "current" | "actual" => open
            .clone()
            .find(|c| c.state(today) == CycleState::Active)
            .or_else(|| open.clone().find(|c| c.state(today) == CycleState::Overdue)),
        "next" | "siguiente" => open
            .clone()
            .filter(|c| c.state(today) == CycleState::Planned)
            .min_by_key(|c| c.start),
        _ => cycles.iter().find(|c| c.name.to_lowercase() == wanted),
    };
    found.cloned().ok_or_else(not_found)
}

/// El proyecto que se indicó, si se indicó.
fn project_arg(r: &dyn Reader, args: &Args) -> Result<Option<Project>, EngineError> {
    args.get("project")
        .map(|p| lookup::project(r, p))
        .transpose()
}

fn check(r: &dyn Reader, c: &Cycle) -> Result<(), EngineError> {
    let name = c.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(EngineError::arg(
            "name",
            Msg::new("arg-length").arg("min", 1u32).arg("max", 60u32),
        ));
    }
    if c.end < c.start || length(c) > MAX_DAYS {
        return Err(EngineError::arg(
            "end",
            Msg::new("cycle-bad-dates")
                .arg("start", c.start.to_string())
                .arg("end", c.end.to_string()),
        ));
    }
    for other in r.cycles(Some(c.project_id))? {
        if other.id != c.id
            && other.closed_at.is_none()
            && other.start <= c.end
            && c.start <= other.end
        {
            return Err(EngineError::invalid(
                Msg::new("cycle-overlap").arg("other", &other.name),
            ));
        }
        if other.id != c.id && other.name.to_lowercase() == name.to_lowercase() {
            return Err(EngineError::invalid(
                Msg::new("cycle-name-taken").arg("name", name),
            ));
        }
    }
    Ok(())
}

/// Tareas vivas del proyecto que están en el ciclo.
fn tasks_in(r: &dyn Reader, c: &Cycle) -> Result<Vec<Task>, EngineError> {
    Ok(r.tasks(&TaskFilter {
        include_archived: true,
        cycle: Some(c.id),
        ..TaskFilter::project(c.project_id)
    })?)
}

pub(crate) fn view(r: &dyn Reader, c: &Cycle, today: Date) -> Result<Value, EngineError> {
    let tasks = tasks_in(r, c)?;
    let counted: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status != model::Status::Cancelled)
        .collect();
    let done = counted
        .iter()
        .filter(|t| t.status == model::Status::Done)
        .count();
    let percent = if counted.is_empty() {
        0
    } else {
        done * 100 / counted.len()
    };
    Ok(json!({
        "id": c.id,
        "name": c.name,
        "start": c.start.to_string(),
        "end": c.end.to_string(),
        "days": length(c),
        "state": c.state(today),
        "closed_at": c.closed_at,
        "progress": { "tasks": counted.len(), "done": done, "percent": percent },
    }))
}

fn create(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = lookup::project(&*cx.tx, args.require("project")?)?;
    guard::check_scope(cx, project.id)?;
    let last = cx
        .tx
        .cycles(Some(project.id))?
        .into_iter()
        .max_by_key(|c| c.end);
    let start = match (day(args, "start")?, &last) {
        (Some(s), _) => s,
        (None, Some(l)) => add(l.end, 1).max(today(cx)),
        (None, None) => today(cx),
    };
    let days = match (args.parse::<i64>("days")?, &last) {
        (Some(d), _) => d,
        (None, Some(l)) => length(l),
        (None, None) => DEFAULT_DAYS,
    };
    let end = match day(args, "end")? {
        Some(e) => e,
        None => add(start, days.clamp(1, MAX_DAYS) - 1),
    };
    let name = match (args.get("name"), &last) {
        (Some(n), _) => n.trim().to_owned(),
        (None, Some(l)) => next_name(&l.name),
        (None, None) => "Sprint 1".to_owned(),
    };
    let cycle = Cycle {
        id: CycleId::generate(),
        project_id: project.id,
        name,
        start,
        end,
        closed_at: None,
        created_at: cx.now,
        updated_at: cx.now,
    };
    check(&*cx.tx, &cycle)?;
    cx.tx.insert_cycle(&cycle)?;
    view(&*cx.tx, &cycle, today(cx))
}

fn list(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = lookup::project(r, args.require("project")?)?;
    let closed = args.flag("closed")?;
    let items = r
        .cycles(Some(project.id))?
        .iter()
        .filter(|c| closed || c.closed_at.is_none())
        .map(|c| view(r, c, today(cx)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "project": project.key, "items": items }))
}

fn get(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let r: &dyn Reader = &*cx.tx;
    let project = project_arg(r, args)?;
    let cycle = find(r, args.require("cycle")?, project.as_ref(), today(cx))?;
    let keys = Keys::load(r)?;
    let mut out = view(r, &cycle, today(cx))?;
    let tasks: Vec<Value> = tasks_in(r, &cycle)?
        .iter()
        .map(|t| json!({ "key": keys.of(t), "title": t.title, "status": t.status }))
        .collect();
    if let Some(obj) = out.as_object_mut() {
        obj.insert("tasks".into(), Value::Array(tasks));
    }
    Ok(out)
}

fn update(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = project_arg(&*cx.tx, args)?;
    let mut cycle = find(&*cx.tx, args.require("cycle")?, project.as_ref(), today(cx))?;
    guard::check_scope(cx, cycle.project_id)?;
    if let Some(name) = args.get("name") {
        cycle.name = name.trim().to_owned();
    }
    if let Some(start) = day(args, "start")? {
        cycle.start = start;
    }
    if let Some(end) = day(args, "end")? {
        cycle.end = end;
    }
    cycle.updated_at = cx.now;
    check(&*cx.tx, &cycle)?;
    cx.tx.update_cycle(&cycle)?;
    view(&*cx.tx, &cycle, today(cx))
}

/// Cierra el ciclo y pasa lo abierto al siguiente (que se crea si falta).
fn close_and_carry(cx: &mut Cx<'_>, mut cycle: Cycle) -> Result<Value, EngineError> {
    if cycle.closed_at.is_some() {
        return Err(EngineError::invalid(
            Msg::new("cycle-closed").arg("cycle", &cycle.name),
        ));
    }
    let still_open: Vec<CycleId> = cx
        .tx
        .cycles(Some(cycle.project_id))?
        .iter()
        .filter(|c| c.id != cycle.id && c.closed_at.is_none())
        .map(|c| c.id)
        .collect();
    // Lo que ya está planificado en otro ciclo abierto no se traspasa.
    let open: Vec<Task> = tasks_in(&*cx.tx, &cycle)?
        .into_iter()
        .filter(|t| !t.status.category().is_closed() && t.archived_at.is_none())
        .filter(|t| !t.cycles.iter().any(|c| still_open.contains(c)))
        .collect();
    cycle.closed_at = Some(cx.now);
    cycle.updated_at = cx.now;
    cx.tx.update_cycle(&cycle)?;
    let mut next_view = Value::Null;
    let mut carried = Vec::new();
    if !open.is_empty() {
        let next = cx
            .tx
            .cycles(Some(cycle.project_id))?
            .into_iter()
            .filter(|c| c.closed_at.is_none() && c.start > cycle.end)
            .min_by_key(|c| c.start);
        let next = match next {
            Some(n) => n,
            None => {
                let start = add(cycle.end, 1);
                let n = Cycle {
                    id: CycleId::generate(),
                    project_id: cycle.project_id,
                    name: next_name(&cycle.name),
                    start,
                    end: add(start, length(&cycle) - 1),
                    closed_at: None,
                    created_at: cx.now,
                    updated_at: cx.now,
                };
                check(&*cx.tx, &n)?;
                cx.tx.insert_cycle(&n)?;
                n
            }
        };
        let keys = Keys::load(&*cx.tx)?;
        for mut t in open {
            // Conserva el ciclo cerrado: así se ve por cuáles pasó.
            t.add_cycle(next.id);
            carried.push(keys.of(&t));
            save(cx, &mut t)?;
        }
        next_view = view(&*cx.tx, &next, today(cx))?;
    }
    Ok(json!({
        "closed": view(&*cx.tx, &cycle, today(cx))?,
        "carried": carried,
        "next": next_view,
    }))
}

fn close(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = project_arg(&*cx.tx, args)?;
    let cycle = find(&*cx.tx, args.require("cycle")?, project.as_ref(), today(cx))?;
    guard::check_scope(cx, cycle.project_id)?;
    close_and_carry(cx, cycle)
}

fn delete(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let project = project_arg(&*cx.tx, args)?;
    let cycle = find(&*cx.tx, args.require("cycle")?, project.as_ref(), today(cx))?;
    guard::check_scope(cx, cycle.project_id)?;
    cx.tx.delete_cycle(cycle.id)?;
    Ok(json!({ "deleted": cycle.name }))
}

/// Tarea periódica del daemon: cierra los ciclos vencidos y traspasa.
fn rollover(cx: &mut Cx<'_>, _: &Args) -> Result<Value, EngineError> {
    supervisor_only(cx)?;
    let due: Vec<Cycle> = cx
        .tx
        .cycles(None)?
        .into_iter()
        .filter(|c| c.state(today(cx)) == CycleState::Overdue)
        .collect();
    let mut closed = Vec::new();
    for c in due {
        closed.push(close_and_carry(cx, c)?);
    }
    Ok(json!({ "closed": closed }))
}

/// Ciclo abierto del proyecto de la tarea al que se la puede añadir.
fn open_cycle_of(cx: &Cx<'_>, task: &Task, reference: &str) -> Result<Cycle, EngineError> {
    let project = cx.tx.project(task.project_id)?;
    let cycle = find(&*cx.tx, reference, project.as_ref(), today(cx))?;
    if cycle.project_id != task.project_id {
        return Err(EngineError::invalid(Msg::new("cycle-other-project")));
    }
    if cycle.closed_at.is_some() {
        return Err(EngineError::invalid(
            Msg::new("cycle-closed").arg("cycle", &cycle.name),
        ));
    }
    Ok(cycle)
}

/// La deja solo en ese ciclo (o en ninguno); para sumar otro, `task.add_cycle`.
fn set_cycle(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    let reference = args.require("cycle")?;
    task.cycles = if matches!(reference.trim(), "none" | "ninguno" | "") {
        Vec::new()
    } else {
        vec![open_cycle_of(cx, &task, reference)?.id]
    };
    save(cx, &mut task)?;
    detail(cx, &task)
}

/// La planifica también en otro ciclo abierto, sin quitarla de los que tiene.
fn add_cycle(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    let cycle = open_cycle_of(cx, &task, args.require("cycle")?)?;
    if task.add_cycle(cycle.id) {
        save(cx, &mut task)?;
    }
    detail(cx, &task)
}

/// La saca de un ciclo, abierto o cerrado.
fn remove_cycle(cx: &mut Cx<'_>, args: &Args) -> Result<Value, EngineError> {
    let mut task = guard::task_for_write(cx, args)?;
    let project = cx.tx.project(task.project_id)?;
    let cycle = find(&*cx.tx, args.require("cycle")?, project.as_ref(), today(cx))?;
    if task.remove_cycle(cycle.id) {
        save(cx, &mut task)?;
    }
    detail(cx, &task)
}

#[cfg(test)]
mod tests {
    use super::next_name;

    #[test]
    fn next_names_count_up() {
        assert_eq!(next_name("Sprint 3"), "Sprint 4");
        assert_eq!(next_name("S9"), "S10");
        assert_eq!(next_name("Otoño"), "Otoño 2");
    }
}
