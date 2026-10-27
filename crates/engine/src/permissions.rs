//! Permisos de los agentes (D11): nivel mínimo de cada comando, pausa y
//! solo lectura global (RF-ACT-05, RF-SUP-08). El supervisor y el sistema no
//! pasan por aquí.

use i18n::Msg;
use model::{Agent, AgentLevel, Interface};
use store::Reader;

use crate::EngineError;

/// Nivel mínimo de un comando. Lo que no aparece es `read` si solo lee y
/// `execute` si escribe.
pub fn required(command: &str, mutates: bool) -> AgentLevel {
    match command {
        "task.create"
        | "plan.create"
        | "task.set_parent"
        | "task.link"
        | "task.unlink"
        | "task.relate"
        | "task.unrelate"
        | "task.delete"
        | "task.restore"
        | "task.archive"
        | "task.unarchive"
        | "task.set_epic"
        | "task.set_milestone"
        | "task.set_recurrence"
        | "task.clear_recurrence"
        | "task.someday"
        | "task.set_cycle"
        | "task.add_cycle"
        | "task.remove_cycle"
        | "cycle.create"
        | "cycle.update"
        | "cycle.close"
        | "cycle.delete"
        | "diagram.delete"
        | "epic.create"
        | "epic.update"
        | "epic.close"
        | "epic.reopen"
        | "template.instantiate"
        | "label.create"
        | "label.update"
        // D20: crear y cambiar columnas es de cualquier agente; borrarlas, no.
        | "status.delete"
        | "agent.create" => AgentLevel::Plan,
        "project.create"
        | "project.update"
        | "project.archive"
        | "project.unarchive"
        | "field.create"
        | "field.update"
        | "field.delete"
        | "template.create"
        | "template.from_task"
        | "template.delete"
        | "epic.delete"
        | "workspace.export"
        | "workspace.import"
        | "workspace.import_preview"
        | "workspace.export_markdown"
        | "workspace.backup"
        | "workspace.web_token"
        | "task.export_csv"
        | "trash.purge"
        | "project.delete"
        | "workspace.wipe"
        | "activity.prune" => AgentLevel::Admin,
        _ if mutates => AgentLevel::Execute,
        _ => AgentLevel::Read,
    }
}

/// ¿Puede `agent` ejecutar `command` ahora? Pausa o solo lectura global
/// rechazan las escrituras con `PAUSED`; un nivel insuficiente, con `FORBIDDEN`.
pub fn check(
    r: &dyn Reader,
    agent: &Agent,
    interface: Option<&Interface>,
    command: &str,
    mutates: bool,
) -> Result<(), EngineError> {
    // Las sesiones siguen funcionando en pausa: el agente sigue conectado.
    let session = command.starts_with("session.");
    if mutates && !session {
        if agent.paused || interface.is_some_and(|i| i.paused) {
            return Err(EngineError::Paused(
                Msg::new("agent-paused").arg("agent", &agent.name),
            ));
        }
        if r.workspace()?.settings.read_only {
            return Err(EngineError::Paused(Msg::new("workspace-read-only")));
        }
    }
    let needed = required(command, mutates);
    // D15: nunca por encima del nivel de su interfaz.
    let level = match interface {
        Some(i) if i.level.rank() < agent.level.rank() => i.level,
        _ => agent.level,
    };
    if level.rank() < needed.rank() {
        return Err(EngineError::Forbidden(
            Msg::new("agent-level")
                .arg("agent", &agent.name)
                .arg("level", level.as_str())
                .arg("needed", needed.as_str())
                .arg("command", command),
        ));
    }
    Ok(())
}

/// Una interfaz que aún no eligió agente (D15) solo lee y crea agentes.
pub fn check_interface(
    interface: &Interface,
    command: &str,
    mutates: bool,
) -> Result<(), EngineError> {
    if !mutates {
        return Ok(());
    }
    if command != "agent.create" {
        return Err(EngineError::Forbidden(
            Msg::new("interface-needs-agent").arg("interface", &interface.name),
        ));
    }
    if interface.paused {
        return Err(EngineError::Paused(
            Msg::new("agent-paused").arg("agent", &interface.name),
        ));
    }
    if interface.level.rank() < AgentLevel::Plan.rank() {
        return Err(EngineError::Forbidden(
            Msg::new("agent-level")
                .arg("agent", &interface.name)
                .arg("level", interface.level.as_str())
                .arg("needed", AgentLevel::Plan.as_str())
                .arg("command", command),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_table_and_defaults() {
        assert_eq!(required("task.get", false), AgentLevel::Read);
        assert_eq!(required("task.claim", true), AgentLevel::Execute);
        assert_eq!(required("plan.create", true), AgentLevel::Plan);
        assert_eq!(required("workspace.export", false), AgentLevel::Admin);
        assert_eq!(required("algo.nuevo", true), AgentLevel::Execute);
    }
}
