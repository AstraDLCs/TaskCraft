//! La CLI real contra un daemon autoarrancado (M2.8): comandos generados,
//! salida legible y JSON, `--offline` y el proxy MCP por stdio.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_taskcraft-cli");

fn tempdb() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("taskcraft-cli-{}-{n}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("workspace.db")
}

/// Detiene el daemon del workspace al terminar el test.
struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = tc(&self.0, &["serve", "--stop"]);
    }
}

fn tc(db: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("--db")
        .arg(db)
        .args(["--lang", "en"])
        .args(args)
        .env("TASKCRAFT_DAEMON_BIN", BIN)
        .env_remove("TASKCRAFT_AGENT")
        .env_remove("TASKCRAFT_AGENT_TOKEN")
        .output()
        .unwrap()
}

fn ok(db: &Path, args: &[&str]) -> String {
    let out = tc(db, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Interfaz con un agente del mismo nombre (D22); devuelve el token de la
/// interfaz.
fn register(db: &Path, name: &str) -> String {
    let token = json_of(db, &["interface", "register", name])["token"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(db, &["agent", "register", name, "--interface", name]);
    token
}

fn json_of(db: &Path, args: &[&str]) -> Value {
    let mut all = args.to_vec();
    all.push("--json");
    serde_json::from_str(&ok(db, &all)).unwrap()
}

#[test]
fn commands_autostart_the_daemon_and_print_tables() {
    let db = tempdb();
    let ws = Workspace(db.clone());
    assert!(
        !tc(&ws.0, &["serve", "--status"]).status.success(),
        "sin daemon aún"
    );

    ok(&db, &["project", "create", "API", "--name", "Backend"]);
    ok(
        &db,
        &[
            "task",
            "create",
            "--project",
            "API",
            "--title",
            "Design schema",
            "--priority",
            "high",
        ],
    );
    assert!(
        tc(&db, &["serve", "--status"]).status.success(),
        "se arrancó solo"
    );

    let table = ok(&db, &["task", "list"]);
    let mut lines = table.lines();
    assert!(lines.next().unwrap().starts_with("KEY"));
    assert!(lines.next().unwrap().contains("Design schema"));

    let detail = json_of(&db, &["task", "get", "API-1"]);
    assert_eq!(detail["priority"], "high");

    let err = tc(&db, &["task", "get", "API-9"]);
    assert!(!err.status.success());
    assert!(String::from_utf8_lossy(&err.stderr).contains("[NOT_FOUND]"));
}

#[test]
fn agents_act_by_token_and_offline_mode_reads_the_same_data() {
    let db = tempdb();
    let _ws = Workspace(db.clone());
    ok(&db, &["project", "create", "API"]);
    ok(&db, &["task", "create", "--project", "API", "--title", "x"]);
    let token = register(&db, "claude-1");
    let claimed = json_of(&db, &["--token", &token, "task", "next", "--claim"]);
    assert_eq!(claimed["task"]["key"], "API-1");
    assert_eq!(claimed["claimed"], true);

    let offline = json_of(&db, &["--offline", "task", "get", "API-1"]);
    assert_eq!(offline["assignee_name"], "claude-1");
}

/// Envía mensajes JSON-RPC por stdin y lee las respuestas por id.
struct McpProc {
    child: std::process::Child,
    out: BufReader<std::process::ChildStdout>,
}

impl McpProc {
    fn send(&mut self, msg: Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    }

    fn response(&mut self, id: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "sin respuesta a {id}");
            let mut line = String::new();
            if self.out.read_line(&mut line).unwrap() == 0 {
                panic!("el proxy MCP terminó");
            }
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }
}

impl McpProc {
    /// Siguiente notificación `method` (salta respuestas y otras).
    fn notification(&mut self, method: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "sin notificación {method}");
            let mut line = String::new();
            if self.out.read_line(&mut line).unwrap() == 0 {
                panic!("el proxy MCP terminó");
            }
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["method"] == method {
                return v;
            }
        }
    }
}

impl Drop for McpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_stdio_lists_generated_tools_and_calls_them() {
    let db = tempdb();
    let _ws = Workspace(db.clone());
    ok(&db, &["project", "create", "API"]);
    ok(&db, &["task", "create", "--project", "API", "--title", "x"]);
    let token = register(&db, "claude-1");

    let mut child = Command::new(BIN)
        .arg("--db")
        .arg(&db)
        .args(["mcp", "--stdio"])
        .env("TASKCRAFT_AGENT_TOKEN", &token)
        .env("TASKCRAFT_DAEMON_BIN", BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut mcp = McpProc { child, out };

    mcp.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" } }
    }));
    let init = mcp.response(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "taskcraft");
    mcp.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    mcp.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = mcp.response(2)["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    let next = tools.iter().find(|t| t["name"] == "task_next").unwrap();
    assert_eq!(
        next["inputSchema"]["properties"]["claim"]["type"],
        "boolean"
    );

    mcp.send(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "task_next", "arguments": { "claim": true } }
    }));
    let got = mcp.response(3)["result"].clone();
    assert_eq!(got["isError"], false);
    assert_eq!(got["structuredContent"]["task"]["key"], "API-1");

    mcp.send(json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": { "name": "agent_register", "arguments": { "name": "otro", "interface": "claude-1" } }
    }));
    let denied = mcp.response(4)["result"].clone();
    assert_eq!(denied["isError"], true);
    assert_eq!(denied["structuredContent"]["code"], "FORBIDDEN");
    assert!(denied["structuredContent"]["hint"].as_str().unwrap().len() > 10);
}

/// RF-MCP-10: un cliente suscrito a una tarea recibe el aviso cuando otro
/// proceso la cambia (pasando por el daemon).
#[test]
fn mcp_stdio_notifies_resource_subscribers() {
    let db = tempdb();
    let _ws = Workspace(db.clone());
    ok(&db, &["project", "create", "API"]);
    ok(
        &db,
        &["task", "create", "--project", "API", "--title", "uno"],
    );
    ok(
        &db,
        &["task", "create", "--project", "API", "--title", "dos"],
    );
    let token = register(&db, "claude-1");
    let mut child = Command::new(BIN)
        .arg("--db")
        .arg(&db)
        .args(["mcp", "--stdio"])
        .env("TASKCRAFT_AGENT_TOKEN", &token)
        .env("TASKCRAFT_DAEMON_BIN", BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut mcp = McpProc { child, out };
    mcp.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" } }
    }));
    let init = mcp.response(1);
    assert_eq!(
        init["result"]["capabilities"]["resources"]["subscribe"],
        true
    );
    mcp.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    mcp.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "resources/subscribe",
        "params": { "uri": "taskcraft://task/API-2" }
    }));
    assert!(mcp.response(2).get("error").is_none());
    mcp.send(json!({
        "jsonrpc": "2.0", "id": 3, "method": "resources/subscribe",
        "params": { "uri": "taskcraft://nada" }
    }));
    assert!(mcp.response(3).get("error").is_some(), "URI desconocida");

    ok(&db, &["task", "update", "API-1", "--title", "otra"]);
    ok(&db, &["task", "update", "API-2", "--title", "cambiada"]);
    let note = mcp.notification("notifications/resources/updated");
    assert_eq!(note["params"]["uri"], "taskcraft://task/API-2");
}

/// RF-MOD-19: un diagrama creado por la CLI se lee como recurso MCP
/// `taskcraft://diagram/API-D1`, con las tareas que lo citan.
#[test]
fn diagrams_from_the_cli_are_mcp_resources() {
    let db = tempdb();
    let _ws = Workspace(db.clone());
    ok(&db, &["project", "create", "API"]);
    let created = json_of(
        &db,
        &[
            "diagram",
            "create",
            "--project",
            "API",
            "--title",
            "Flujo",
            "--source",
            "flowchart TD\n  A --> B",
        ],
    );
    assert_eq!(created["key"], "API-D1");
    ok(
        &db,
        &[
            "task",
            "create",
            "--project",
            "API",
            "--title",
            "uno",
            "--description",
            "![[API-D1]]",
        ],
    );
    let token = register(&db, "claude-1");
    let mut child = Command::new(BIN)
        .arg("--db")
        .arg(&db)
        .args(["mcp", "--stdio"])
        .env("TASKCRAFT_AGENT_TOKEN", &token)
        .env("TASKCRAFT_DAEMON_BIN", BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut mcp = McpProc { child, out };
    mcp.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" } }
    }));
    mcp.response(1);
    mcp.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    mcp.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "resources/read",
        "params": { "uri": "taskcraft://diagram/API-D1" }
    }));
    let read = mcp.response(2);
    let text = read["result"]["contents"][0]["text"].as_str().unwrap();
    let diagram: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(diagram["title"], "Flujo");
    assert_eq!(diagram["cited_by"], json!(["API-1"]));
    assert!(diagram["render_error"].is_null(), "{diagram}");
}

/// RF-EXP-08: `migrate` copia todo a un destino vacío y no pisa uno con datos.
#[test]
fn migrate_copies_a_workspace_to_an_empty_destination() {
    let from = tempdb();
    ok(&from, &["--offline", "project", "create", "--key", "MIG"]);
    ok(
        &from,
        &[
            "--offline",
            "task",
            "create",
            "--project",
            "MIG",
            "--title",
            "copiada",
        ],
    );
    let to = tempdb();
    let out = ok(
        &from,
        &[
            "migrate",
            "--from",
            from.to_str().unwrap(),
            "--to",
            to.to_str().unwrap(),
        ],
    );
    assert!(out.contains("Migrated 1 tasks"), "{out}");
    let listed = ok(&to, &["--offline", "task", "list"]);
    assert!(
        listed.contains("MIG-1") && listed.contains("copiada"),
        "{listed}"
    );
    let again = tc(
        &from,
        &[
            "migrate",
            "--from",
            from.to_str().unwrap(),
            "--to",
            to.to_str().unwrap(),
        ],
    );
    assert!(!again.status.success());
}

/// D15 por MCP: una interfaz con un solo token crea sus agentes, elige uno
/// con `agent_use` y actúa como otro con `as_agent`.
#[test]
fn an_interface_switches_agents_over_mcp() {
    let db = tempdb();
    let _ws = Workspace(db.clone());
    ok(&db, &["project", "create", "API"]);
    let token = json_of(
        &db,
        &[
            "interface",
            "register",
            "claude-code",
            "--kind",
            "claude-code",
        ],
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut child = Command::new(BIN)
        .arg("--db")
        .arg(&db)
        .args(["mcp", "--stdio"])
        .env("TASKCRAFT_AGENT_TOKEN", &token)
        .env("TASKCRAFT_DAEMON_BIN", BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut mcp = McpProc { child, out };
    mcp.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" } }
    }));
    mcp.response(1);
    mcp.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    let mut id = 1;
    let mut call = |mcp: &mut McpProc, name: &str, arguments: Value| {
        id += 1;
        mcp.send(json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }));
        mcp.response(id)["result"].clone()
    };

    for name in ["cc-dev", "cc-rev"] {
        let made = call(&mut mcp, "agent_create", json!({ "name": name }));
        assert_eq!(made["isError"], false, "{made}");
    }
    let refused = call(
        &mut mcp,
        "task_create",
        json!({ "project": "API", "title": "x" }),
    );
    assert_eq!(refused["isError"], true);
    let chosen = call(&mut mcp, "agent_use", json!({ "agent": "cc-dev" }));
    assert_eq!(
        chosen["structuredContent"]["acting_as"], "cc-dev",
        "{chosen}"
    );
    let made = call(
        &mut mcp,
        "task_create",
        json!({ "project": "API", "title": "x" }),
    );
    assert_eq!(
        made["structuredContent"]["creator_name"], "cc-dev",
        "{made}"
    );
    let other = call(
        &mut mcp,
        "comment_add",
        json!({ "task": "API-1", "body": "revisado", "as_agent": "cc-rev" }),
    );
    assert_eq!(
        other["structuredContent"]["author_name"], "cc-rev",
        "{other}"
    );
    // Sigue siendo cc-dev para lo demás.
    let mine = call(&mut mcp, "agent_use", json!({}));
    assert_eq!(mine["structuredContent"]["acting_as"], "cc-dev");
    assert_eq!(
        mine["structuredContent"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// Git en un repo temporal, con autor fijo y sin la configuración del usuario.
fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("TASKCRAFT_DAEMON_BIN", BIN)
        .env_remove("TASKCRAFT_AGENT_TOKEN")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(repo: &Path, file: &str, message: &str) {
    std::fs::write(repo.join(file), message).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-q", "-m", message]);
}

/// RF-INT-04 (D16): el hook vincula cada commit y `fixes` pasa a revisión;
/// `git scan` repite sin duplicar.
#[test]
fn the_git_hook_links_commits_and_moves_closed_tasks_to_review() {
    let db = tempdb();
    let _ws = Workspace(db.clone());
    ok(&db, &["project", "create", "API"]);
    ok(
        &db,
        &["task", "create", "--project", "API", "--title", "uno"],
    );
    ok(
        &db,
        &["task", "create", "--project", "API", "--title", "dos"],
    );
    let repo = db.with_file_name("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(
        &repo,
        &["remote", "add", "origin", "git@github.com:org/repo.git"],
    );

    let installed = ok(
        &db,
        &["git", "hook", "install", "--repo", repo.to_str().unwrap()],
    );
    assert!(installed.contains("post-commit"), "{installed}");

    commit(&repo, "a.txt", "Fixes API-1: primer arreglo");
    let api1 = json_of(&db, &["task", "get", "API-1"]);
    assert_eq!(api1["status"], "in_review", "{api1}");
    let link = &api1["git"][0];
    assert_eq!(link["kind"], "commit");
    assert!(
        link["url"]
            .as_str()
            .unwrap()
            .starts_with("https://github.com/org/repo/commit/")
    );

    git(&repo, &["checkout", "-q", "-b", "feat/API-2-login"]);
    commit(&repo, "b.txt", "Empieza el login");
    let api2 = json_of(&db, &["task", "get", "API-2"]);
    assert_eq!(api2["status"], "todo");
    assert_eq!(api2["git"][0]["kind"], "branch");
    assert_eq!(api2["git"][0]["ref"], "feat/API-2-login");

    // Volver a leer todo no duplica nada.
    let scanned = ok(
        &db,
        &[
            "git",
            "scan",
            "--repo",
            repo.to_str().unwrap(),
            "--last",
            "5",
        ],
    );
    assert!(scanned.contains("2 commits"), "{scanned}");
    let links = json_of(&db, &["git", "links", "--project", "API"]);
    assert_eq!(links["items"].as_array().unwrap().len(), 2, "{links}");

    let removed = ok(
        &db,
        &["git", "hook", "uninstall", "--repo", repo.to_str().unwrap()],
    );
    assert!(!removed.is_empty());
    assert!(!repo.join(".git/hooks/post-commit").exists());
}
