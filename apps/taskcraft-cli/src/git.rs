//! Integración con git del lado de la CLI (RF-INT-04, D16): el hook
//! `post-commit` y la lectura de `git log` que alimenta `git.ingest`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use i18n::{Lang, Msg, t, tr};
use protocol::{ArgMap, Dispatch};
use serde_json::Value;

/// Línea que marca un hook escrito por TaskCraft.
const MARKER: &str = "# TaskCraft (RF-INT-04)";

/// Qué commits leer.
pub enum Range {
    /// Los últimos N de la rama actual.
    Last(usize),
    /// Los que hay desde esa revisión hasta HEAD.
    Since(String),
}

/// Ejecuta git en el repo y devuelve su salida, sin el salto final.
fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("taskcraft-cli: {msg}");
    ExitCode::FAILURE
}

fn not_repo(lang: Lang, repo: &Path, detail: &str) -> ExitCode {
    fail(&tr(
        lang,
        &Msg::new("cli-git-not-repo")
            .arg("repo", repo.display().to_string())
            .arg("detail", detail),
    ))
}

/// Comillas simples de shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Carpeta de hooks del repo (respeta `core.hooksPath`).
fn hooks_dir(repo: &Path) -> Result<PathBuf, String> {
    let dir = PathBuf::from(git(repo, &["rev-parse", "--git-path", "hooks"])?);
    Ok(if dir.is_absolute() {
        dir
    } else {
        repo.join(dir)
    })
}

/// `git hook install|uninstall`: escribe o quita `post-commit`.
pub fn hook(db: &Path, install: bool, repo: &Path, lang: Lang) -> ExitCode {
    let dir = match hooks_dir(repo) {
        Ok(d) => d,
        Err(e) => return not_repo(lang, repo, &e),
    };
    let path = dir.join("post-commit");
    let ours = std::fs::read_to_string(&path).map(|s| s.contains(MARKER));
    if !install {
        return match ours {
            Ok(true) => match std::fs::remove_file(&path) {
                Ok(()) => {
                    println!("{}", t(lang, "cli-git-hook-removed"));
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&e.to_string()),
            },
            _ => fail(&t(lang, "cli-git-hook-missing")),
        };
    }
    if matches!(ours, Ok(false)) {
        return fail(&tr(
            lang,
            &Msg::new("cli-git-hook-exists").arg("path", path.display().to_string()),
        ));
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return fail(&e.to_string()),
    };
    // Una cadena de conexión puede llevar la contraseña: no se escribe en el
    // repo; el hook usará la base del entorno (TASKCRAFT_DB).
    let url = protocol::is_url(db);
    let db_arg = if url {
        String::new()
    } else {
        format!(" --db {}", quote(&db.display().to_string()))
    };
    let script = format!(
        "#!/bin/sh\n{MARKER}: vincula cada commit con sus tareas.\n\
         # `taskcraft-cli git hook uninstall` lo quita.\n\
         {}{db_arg} git scan --last 1 --quiet >/dev/null 2>&1 || true\n",
        quote(&exe.display().to_string()),
    );
    let written = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, script));
    if let Err(e) = written {
        return fail(&e.to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)) {
            return fail(&e.to_string());
        }
    }
    println!(
        "{}",
        tr(
            lang,
            &Msg::new("cli-git-hook-installed").arg("path", path.display().to_string()),
        )
    );
    if url {
        println!("{}", t(lang, "cli-git-hook-db-env"));
    }
    ExitCode::SUCCESS
}

/// Un commit leído de `git log`.
struct Commit {
    sha: String,
    message: String,
}

fn commits(repo: &Path, range: &Range) -> Result<Vec<Commit>, String> {
    // Separadores de unidad y de registro: no aparecen en un mensaje normal.
    let format = "--format=%H%x1f%B%x1e";
    let raw = match range {
        Range::Last(n) => git(repo, &["log", "--reverse", format, "-n", &n.to_string()])?,
        Range::Since(rev) => git(repo, &["log", "--reverse", format, &format!("{rev}..HEAD")])?,
    };
    Ok(raw
        .split('\u{1e}')
        .filter_map(|record| {
            let (sha, message) = record.trim_start_matches('\n').split_once('\u{1f}')?;
            Some(Commit {
                sha: sha.trim().to_owned(),
                message: message.trim().to_owned(),
            })
        })
        .filter(|c| !c.sha.is_empty())
        .collect())
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `git scan`: pasa cada commit del rango por `git.ingest`.
pub fn scan(
    backend: &mut dyn Dispatch,
    repo: &Path,
    range: &Range,
    quiet: bool,
    lang: Lang,
) -> ExitCode {
    let list = match commits(repo, range) {
        Ok(c) => c,
        Err(e) => return not_repo(lang, repo, &e),
    };
    let branch = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .filter(|b| b != "HEAD");
    let remote = git(repo, &["remote", "get-url", "origin"]).ok();
    let mut linked: Vec<String> = Vec::new();
    let mut moved: Vec<String> = Vec::new();
    for c in &list {
        let mut args = ArgMap::new();
        args.insert("sha".into(), c.sha.clone());
        args.insert("message".into(), c.message.clone());
        if let Some(b) = &branch {
            args.insert("branch".into(), b.clone());
        }
        if let Some(r) = &remote {
            args.insert("remote".into(), r.clone());
        }
        match backend.dispatch("git.ingest", &args, lang) {
            Ok(v) => {
                for key in strings(&v, "linked") {
                    if !linked.contains(&key) {
                        linked.push(key);
                    }
                }
                moved.extend(strings(&v, "moved"));
            }
            Err(e) => return fail(&format!("[{}] {}", e.code, e.message)),
        }
    }
    if !quiet {
        println!(
            "{}",
            tr(
                lang,
                &Msg::new("cli-git-scanned")
                    .arg("commits", list.len())
                    .arg("linked", linked.join(", "))
                    .arg("moved", moved.join(", ")),
            )
        );
    }
    ExitCode::SUCCESS
}
