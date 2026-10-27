//! Recursos de solo lectura (RF-MCP-05) y prompts predefinidos (RF-MCP-06).
//! Los recursos se leen con los mismos comandos que las tools, así respetan
//! la identidad y los permisos de quien pregunta.

use std::sync::Arc;

use model::Actor;
use protocol::{ArgMap, WireError};
use rmcp::model::{
    GetPromptResult, Prompt, PromptArgument, PromptMessage, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, Role,
};
use serde_json::Value;

use crate::Backend;

const SCHEME: &str = "taskcraft://";

fn args(pairs: &[(&str, &str)]) -> ArgMap {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

async fn call(
    backend: &Arc<dyn Backend>,
    caller: Option<Actor>,
    command: &'static str,
    pairs: ArgMap,
) -> Result<Value, WireError> {
    let backend = backend.clone();
    tokio::task::spawn_blocking(move || backend.call(caller, command, &pairs))
        .await
        .unwrap_or_else(|e| Err(WireError::new("INTERNAL", "error-panicked", e.to_string())))
}

/// Un recurso por proyecto (y su árbol) más el feed de actividad.
pub async fn list(
    backend: &Arc<dyn Backend>,
    caller: Option<Actor>,
) -> Result<Vec<Resource>, WireError> {
    let projects = call(backend, caller, "project.list", args(&[("limit", "500")])).await?;
    let mut out = vec![
        Resource::new(format!("{SCHEME}activity"), "activity")
            .with_description("Latest workspace activity (who changed what)"),
    ];
    let items = projects.get("items").and_then(Value::as_array);
    for p in items.into_iter().flatten() {
        let key = p["key"].as_str().unwrap_or_default();
        let name = p["name"].as_str().unwrap_or_default();
        out.push(
            Resource::new(format!("{SCHEME}project/{key}"), format!("project {key}"))
                .with_description(format!(
                    "Project {key} · {name}: settings, workflow and counts"
                )),
        );
        out.push(
            Resource::new(format!("{SCHEME}project/{key}/tree"), format!("tree {key}"))
                .with_description(format!("Task tree of {key}: key, title, status and parent")),
        );
    }
    Ok(out)
}

pub fn templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(format!("{SCHEME}task/{{key}}"), "task")
            .with_description("A task in detail, by key (API-42)"),
        ResourceTemplate::new(format!("{SCHEME}project/{{key}}"), "project")
            .with_description("A project with its workflow and fields"),
        ResourceTemplate::new(format!("{SCHEME}project/{{key}}/tree"), "project tree")
            .with_description("All live tasks of a project"),
        ResourceTemplate::new(format!("{SCHEME}diagram/{{key}}"), "diagram").with_description(
            "A Mermaid diagram by key (API-D3): source, render_error and the tasks citing it",
        ),
    ]
}

/// ¿Es una URI de recurso que [`read`] sabe leer?
pub fn is_known(uri: &str) -> bool {
    let Some(path) = uri.strip_prefix(SCHEME) else {
        return false;
    };
    matches!(
        path.split('/').collect::<Vec<_>>().as_slice(),
        ["activity"] | ["task", _] | ["project", _] | ["project", _, "tree"] | ["diagram", _]
    )
}

/// Lee `taskcraft://…` como JSON.
pub async fn read(
    backend: &Arc<dyn Backend>,
    caller: Option<Actor>,
    uri: &str,
) -> Result<ReadResourceResult, WireError> {
    let path = uri.strip_prefix(SCHEME).unwrap_or(uri);
    let parts: Vec<&str> = path.split('/').collect();
    let value = match parts.as_slice() {
        ["activity"] => call(backend, caller, "activity.list", args(&[("limit", "50")])).await?,
        ["task", key] => call(backend, caller, "task.get", args(&[("task", key)])).await?,
        ["project", key] => call(backend, caller, "project.get", args(&[("project", key)])).await?,
        ["diagram", key] => call(backend, caller, "diagram.get", args(&[("diagram", key)])).await?,
        ["project", key, "tree"] => {
            let pairs = args(&[
                ("project", key),
                ("limit", "500"),
                (
                    "fields",
                    "key,title,status,stage,priority,parent,ready,claimed_by,labels",
                ),
            ]);
            call(backend, caller, "task.list", pairs).await?
        }
        _ => {
            return Err(WireError::new(
                "NOT_FOUND",
                "mcp-unknown-resource",
                format!("Unknown resource `{uri}`"),
            ));
        }
    };
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    Ok(ReadResourceResult::new(vec![
        ResourceContents::TextResourceContents {
            uri: uri.to_owned(),
            mime_type: Some("application/json".into()),
            text,
            meta: None,
        },
    ]))
}

pub fn prompts() -> Vec<Prompt> {
    let project = || PromptArgument::new("project").with_description("Project key, e.g. API");
    vec![
        Prompt::new(
            "plan_project",
            Some("Turn a specification into a task tree with plan_create"),
            Some(vec![
                PromptArgument::new("spec")
                    .with_description("The specification or goal to plan")
                    .with_required(true),
                project().with_required(true),
            ]),
        ),
        Prompt::new(
            "work_next_task",
            Some("Take the next available task, do it and hand it over"),
            Some(vec![project()]),
        ),
        Prompt::new(
            "status_report",
            Some("Summarise the state of the work for the supervisor"),
            Some(vec![project()]),
        ),
    ]
}

pub fn prompt(
    name: &str,
    arguments: &serde_json::Map<String, Value>,
) -> Result<GetPromptResult, WireError> {
    let arg = |k: &str| {
        arguments
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let project = arg("project");
    let in_project = if project.is_empty() {
        String::new()
    } else {
        format!(" in project {project}")
    };
    let text = match name {
        "plan_project" => format!(
            "Plan the following specification as TaskCraft tasks{in_project}.\n\n\
             Specification:\n{spec}\n\n\
             1. Read the project with the resource taskcraft://project/{project} to learn its \
             workflow, fields and estimate unit.\n\
             2. Break the work into tasks small enough for one agent session, with subtasks \
             (parent), dependencies (blocked_by), checklists and acceptance criteria.\n\
             3. Create everything at once with plan_create (it is all or nothing). Use `ref` \
             to link items of the same plan.\n\
             4. Report the created keys.",
            spec = arg("spec"),
        ),
        "work_next_task" => format!(
            "Work on TaskCraft{in_project}.\n\n\
             0. If your interface has several agents, pick yours with agent_use (create it with \
             agent_create if it does not exist).\n\
             1. Call task_next with claim=true{project_arg}. If nothing is available, stop. The \
             answer brings the notes left by previous agents: read them first. What you do is \
             your function (your agent's description).\n\
             2. Read the task with task_get: description, checklist and acceptance criteria.\n\
             3. Do the work. Leave progress notes with comment_add and tick steps and criteria \
             with task_check. Any write renews your lease; otherwise call task_heartbeat.\n\
             4. When done, hand it over with task_move and a `note`: to done, or to the column \
             of whoever comes next if the task asks for it (create that agent's column with \
             status_create owner=<agent> if it does not exist). Nobody has to approve it.\n\
             5. If you are blocked, use task_escalate with a precise question, or task_release \
             with a reason.\n\
             6. If you know the tokens or the cost of your work, report them with cost_report \
             (tokens_in, tokens_out, amount, model) before moving on. Put the task key in your \
             commit messages so they get linked, and record pull requests with git_link.",
            project_arg = if project.is_empty() {
                String::new()
            } else {
                format!(" and project={project}")
            },
        ),
        "status_report" => format!(
            "Write a short status report of TaskCraft{in_project} for the supervisor.\n\n\
             Use task_list (status filters), the resource taskcraft://activity and \
             activity_summary since the start of the day. Cover: what was finished, what is \
             in review, what is blocked or escalated, which agents are working on what, and \
             risks. Be concise and cite task keys."
        ),
        _ => {
            return Err(WireError::new(
                "NOT_FOUND",
                "mcp-unknown-prompt",
                format!("Unknown prompt `{name}`"),
            ));
        }
    };
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        Role::User,
        text,
    )]))
}

#[cfg(test)]
mod tests {
    use engine::{Args, Context, Engine, SharedEngine};
    use store_sqlite::SqliteStore;

    use super::*;

    /// El supervisor, en proceso.
    struct Supervisor(SharedEngine);

    impl Backend for Supervisor {
        fn call(&self, _: Option<Actor>, command: &str, args: &ArgMap) -> Result<Value, WireError> {
            self.0
                .dispatch(
                    &Context::supervisor(),
                    command,
                    &Args::from_map(args.clone()),
                )
                .map_err(|e| e.to_wire(i18n::Lang::En))
        }
    }

    fn backend() -> Arc<dyn Backend> {
        let engine = SharedEngine::new(Engine::new(SqliteStore::open_in_memory().unwrap()));
        let sup = Context::supervisor();
        engine
            .dispatch(&sup, "project.create", &Args::none().with("key", "API"))
            .unwrap();
        engine
            .dispatch(
                &sup,
                "task.create",
                &Args::none().with("project", "API").with("title", "Uno"),
            )
            .unwrap();
        Arc::new(Supervisor(engine))
    }

    #[tokio::test]
    async fn resources_list_and_read_projects_tasks_and_activity() {
        let b = backend();
        let uris: Vec<String> = list(&b, None)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.uri)
            .collect();
        assert!(uris.contains(&"taskcraft://project/API".to_owned()));
        assert!(uris.contains(&"taskcraft://project/API/tree".to_owned()));
        assert!(uris.contains(&"taskcraft://activity".to_owned()));
        let text = |r: ReadResourceResult| match r.contents.into_iter().next() {
            Some(ResourceContents::TextResourceContents { text, .. }) => text,
            _ => String::new(),
        };
        let task = text(read(&b, None, "taskcraft://task/API-1").await.unwrap());
        assert!(task.contains("\"title\": \"Uno\""));
        let tree = text(
            read(&b, None, "taskcraft://project/API/tree")
                .await
                .unwrap(),
        );
        assert!(tree.contains("API-1"));
        assert!(read(&b, None, "taskcraft://nada").await.is_err());
    }

    #[test]
    fn prompts_render_with_their_arguments() {
        let names: Vec<String> = prompts().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["plan_project", "work_next_task", "status_report"]);
        let args = serde_json::json!({ "spec": "Login con OAuth", "project": "API" });
        let result = prompt("plan_project", args.as_object().unwrap()).unwrap();
        let text = serde_json::to_string(&result).unwrap();
        assert!(text.contains("Login con OAuth") && text.contains("plan_create"));
        assert!(prompt("nope", &serde_json::Map::new()).is_err());
    }
}
