//! Tools MCP generadas desde el registro de comandos (RF-MCP-02/03): nombre,
//! descripción en inglés (RNF-12), JSON Schema de entrada y salida, y la
//! conversión de los argumentos JSON al texto que espera `engine::Args`.

use std::sync::Arc;

use engine::{Command, Kind, Param, Registry};
use i18n::{Lang, t};
use protocol::ArgMap;
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};

/// Comandos que no se ofrecen como tools: las sesiones las gestiona la
/// conexión y las tareas de sistema las corre el daemon.
fn exposed(c: &dyn Command) -> bool {
    !c.id().starts_with("session.") && !c.id().starts_with("system.")
}

/// `task.claim` → `task_claim`: un nombre que aceptan todos los clientes.
pub fn tool_name(command: &str) -> String {
    command.replace('.', "_")
}

/// Tool propia del MCP (no es un comando): elegir con qué agente se actúa.
pub const AGENT_USE: &str = "agent_use";

/// Todas las tools, en el orden del registro, más `agent_use` (D15).
pub fn all(registry: &Registry) -> Vec<Tool> {
    let mut tools: Vec<Tool> = registry.iter().filter(|c| exposed(*c)).map(tool).collect();
    tools.push(agent_use());
    tools
}

/// `agent_use`: fija el agente de esta sesión (RF-MCP-11). Sin `agent`, dice
/// cuál está activo y cuáles puede usar.
fn agent_use() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "agent": {
                "type": "string",
                "description": "Agent of your interface to act as from now on (omit to list them)"
            }
        },
        "additionalProperties": false
    });
    let schema = match schema {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    };
    Tool::new(
        AGENT_USE,
        "Choose which of your interface's agents you act as in this session (or list them). \
         Create agents with agent_create; any tool also accepts as_agent for a single call.",
        Arc::new(schema),
    )
    .with_raw_output_schema(Arc::new(match json!({ "type": "object" }) {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }))
    .with_annotations(ToolAnnotations::new().read_only(true).open_world(false))
}

/// Comando del registro que corresponde a una tool, si se ofrece.
pub fn command_for<'r>(registry: &'r Registry, tool: &str) -> Option<&'r dyn Command> {
    registry
        .iter()
        .find(|c| exposed(*c) && tool_name(c.id()) == tool)
}

fn tool(c: &dyn Command) -> Tool {
    let destructive = matches!(
        c.id(),
        "task.delete" | "trash.purge" | "agent.revoke" | "inbox.discard"
    );
    let annotations = ToolAnnotations::new()
        .read_only(!c.mutates())
        .destructive(destructive)
        .open_world(false);
    // El nivel que exige a un agente (D11), para que no intente lo que no puede.
    let level = engine::permissions::required(c.id(), c.mutates());
    let description = format!(
        "{} [agent level: {}]",
        t(Lang::En, c.label()),
        level.as_str()
    );
    Tool::new(
        tool_name(c.id()),
        description,
        Arc::new(input_schema(c.params())),
    )
    .with_title(c.id())
    .with_raw_output_schema(Arc::new(output_schema(c)))
    .with_annotations(annotations)
}

fn input_schema(params: &[Param]) -> JsonObject {
    let mut properties = serde_json::Map::new();
    for p in params {
        let mut schema = kind_schema(p.kind);
        if let Some(obj) = schema.as_object_mut() {
            obj.insert(
                "description".into(),
                json!(i18n::lookup(Lang::En, &p.help_key()).unwrap_or_default()),
            );
        }
        properties.insert(p.name.to_owned(), schema);
    }
    // D15: cualquier tool puede ejecutarse como otro agente de la interfaz.
    properties.insert(
        engine::AS_AGENT.to_owned(),
        json!({
            "type": "string",
            "description": "Act as this agent of your interface for this call only"
        }),
    );
    let required: Vec<&str> = params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name)
        .collect();
    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), json!(required));
    }
    schema.insert("additionalProperties".into(), json!(false));
    schema
}

fn kind_schema(kind: Kind) -> Value {
    match kind {
        Kind::Text => json!({ "type": "string" }),
        Kind::Int => json!({ "type": "integer" }),
        Kind::Bool => json!({ "type": "boolean" }),
        Kind::Date => json!({ "type": "string", "format": "date-time" }),
        Kind::List => json!({ "type": "array", "items": { "type": "string" } }),
        Kind::Json => json!({ "type": "array", "items": { "type": "object" } }),
        Kind::Enum(values) => json!({ "type": "string", "enum": values }),
    }
}

/// Los listados devuelven siempre la misma forma (RF-MCP-08); el resto, un
/// objeto con el detalle de lo que se creó o cambió.
fn output_schema(c: &dyn Command) -> JsonObject {
    let paged = c.params().iter().any(|p| p.name == "limit");
    let schema = if paged {
        json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" },
                "total": { "type": "integer" },
                "items": { "type": "array", "items": { "type": "object" } },
                "next_cursor": { "type": ["string", "null"] }
            },
            "required": ["count", "items"]
        })
    } else {
        json!({ "type": "object" })
    };
    match schema {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}

/// JSON del cliente → texto de `engine::Args`. Las listas de escalares se
/// unen con comas; los objetos (plan.create) viajan como JSON.
pub fn to_args(arguments: Option<JsonObject>) -> ArgMap {
    let mut args = ArgMap::new();
    for (name, value) in arguments.unwrap_or_default() {
        let text = match value {
            Value::Null => continue,
            Value::String(s) => s,
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::Array(items) if items.iter().all(is_scalar) => {
                items.iter().map(scalar_text).collect::<Vec<_>>().join(",")
            }
            other => other.to_string(),
        };
        args.insert(name, text);
    }
    args
}

fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Number(_) | Value::Bool(_))
}

fn scalar_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Qué hacer ante cada código de error (RF-MCP-07), para el agente.
pub fn hint(code: &str) -> &'static str {
    match code {
        "CONFLICT" => {
            "The task changed since you read it. Review `current` and retry with revision set to its current revision."
        }
        "NOT_READY" => {
            "The task has open blockers or is not in a claimable status. Call task_next to get an available task."
        }
        "LOCKED_BY_HUMAN" => {
            "The supervisor locked this or must answer a question first. Work on something else (task_next)."
        }
        "CLAIMED_BY_OTHER" => "Another agent holds this task. Call task_next to get a free one.",
        "FORBIDDEN" => {
            "You are not allowed to do this. If you really need it, ask the supervisor with task_escalate."
        }
        "NOT_FOUND" => {
            "Check the reference (task keys look like API-42). Use task_list or task_search to find it."
        }
        "INVALID" => "Fix the arguments as the message says and call the tool again.",
        "UNAVAILABLE" => "The TaskCraft daemon is not reachable. Retry in a few seconds.",
        "PAUSED" => {
            "The supervisor paused writes for you. Keep reading if useful, then stop and wait; do not retry in a loop."
        }
        _ => "Unexpected server error; nothing was saved. Retry once, then escalate.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_becomes_a_tool_with_schema_and_description() {
        let registry = Registry::standard();
        let tools = all(&registry);
        assert!(tools.iter().all(|t| !t.name.contains('.')));
        let next = tools.iter().find(|t| t.name == "task_next").unwrap();
        assert!(
            next.description
                .as_deref()
                .unwrap()
                .contains("Your next task")
        );
        assert_eq!(next.input_schema["properties"]["claim"]["type"], "boolean");
        assert!(tools.iter().all(|t| t.output_schema.is_some()));
        assert!(!tools.iter().any(|t| t.name.starts_with("session_")));
        let claim = tools.iter().find(|t| t.name == "task_claim").unwrap();
        assert_eq!(claim.input_schema["required"], json!(["task"]));
        let list = tools.iter().find(|t| t.name == "task_list").unwrap();
        assert!(list.annotations.as_ref().unwrap().read_only_hint == Some(true));
        assert!(list.output_schema.as_ref().unwrap()["properties"]["items"].is_object());
    }

    #[test]
    fn rf_mcp_04_minimum_set_is_present() {
        let registry = Registry::standard();
        for name in [
            "project_list",
            "project_get",
            "project_create",
            "task_create",
            "task_get",
            "task_update",
            "task_move",
            "task_list",
            "task_search",
            "task_next",
            "task_claim",
            "task_heartbeat",
            "task_release",
            "task_submit_review",
            "task_escalate",
            "plan_create",
            "task_check",
            "task_link",
            "task_unlink",
            "comment_add",
            "inbox_add",
            "activity_list",
        ] {
            assert!(command_for(&registry, name).is_some(), "falta {name}");
        }
    }

    #[test]
    fn json_arguments_become_engine_text() {
        let args = to_args(Some(
            json!({
                "task": "API-1",
                "claim": true,
                "revision": 3,
                "criteria": [1, 3],
                "labels": ["db", "rust"],
                "items": [{ "title": "x" }],
                "due": null
            })
            .as_object()
            .cloned()
            .unwrap(),
        ));
        assert_eq!(args["task"], "API-1");
        assert_eq!(args["claim"], "true");
        assert_eq!(args["revision"], "3");
        assert_eq!(args["criteria"], "1,3");
        assert_eq!(args["labels"], "db,rust");
        assert_eq!(args["items"], r#"[{"title":"x"}]"#);
        assert!(!args.contains_key("due"));
    }
}
