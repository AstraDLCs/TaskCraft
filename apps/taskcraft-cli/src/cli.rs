//! Línea de comandos generada desde el registro (RF-INT-01): cada comando
//! `dominio.accion` es `taskcraft-cli dominio accion --param valor`, con la
//! ayuda en el idioma elegido.

use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command};
use engine::{Kind, Registry};
use i18n::{Lang, Msg, t, tr};
use protocol::ArgMap;

/// Variable de entorno con el nombre del agente que ejecuta los comandos.
const AGENT_ENV: &str = "TASKCRAFT_AGENT";

/// Opciones comunes a todos los subcomandos.
pub struct Global {
    pub db: Option<PathBuf>,
    /// Workspace por nombre (RF-ALM-08); `--db` manda sobre él.
    pub workspace: Option<String>,
    pub agent: Option<String>,
    pub token: Option<String>,
    pub lang: Lang,
    pub json: bool,
    pub offline: bool,
}

pub enum Invocation {
    Commands,
    Where,
    Workspaces,
    Status,
    Serve {
        http: Option<String>,
        idle_exit: Option<u64>,
        stop: bool,
        /// App web (D14): carpeta con sus archivos.
        web: Option<PathBuf>,
        tls: Option<(PathBuf, PathBuf)>,
    },
    Mcp,
    /// Copia un workspace a otro backend (RF-EXP-08).
    Migrate {
        from: PathBuf,
        to: PathBuf,
    },
    Run {
        command: String,
        args: ArgMap,
    },
    /// `git hook install|uninstall` (RF-INT-04, D16).
    GitHook {
        install: bool,
        repo: PathBuf,
    },
    /// `git scan`: los commits del repo por `git.ingest`.
    GitScan {
        repo: PathBuf,
        range: crate::git::Range,
        quiet: bool,
    },
}

/// Subcomandos fijos; el resto son dominios del registro.
const FIXED: &[&str] = &[
    "commands",
    "where",
    "workspaces",
    "serve",
    "mcp",
    "migrate",
    "run",
];

/// Idioma antes de parsear (la ayuda ya debe salir traducida): `--lang`, y si
/// no, inglés para agentes y el del sistema para personas.
pub fn pre_lang() -> Lang {
    let args: Vec<String> = std::env::args().collect();
    let explicit = args
        .iter()
        .enumerate()
        .find_map(|(i, a)| match a.strip_prefix("--lang") {
            Some(rest) if rest.starts_with('=') => Lang::from_tag(&rest[1..]),
            Some("") => args.get(i + 1).and_then(|v| Lang::from_tag(v)),
            _ => None,
        });
    let is_agent = args.iter().any(|a| {
        a == "--agent" || a.starts_with("--agent=") || a == "--token" || a.starts_with("--token=")
    }) || std::env::var_os(AGENT_ENV).is_some()
        || std::env::var_os(client::TOKEN_ENV).is_some();
    explicit.unwrap_or(if is_agent { Lang::En } else { Lang::detect() })
}

/// `lease_minutes` → `--lease-minutes`.
fn flag(param: &str) -> String {
    param.replace('_', "-")
}

pub fn command(lang: Lang) -> Command {
    let mut cmd = Command::new("taskcraft-cli")
        .about(t(lang, "cli-about"))
        .subcommand_required(true)
        .arg(
            Arg::new("db")
                .long("db")
                .global(true)
                .value_name("PATH")
                .help(t(lang, "cli-db-help")),
        )
        .arg(
            Arg::new("workspace")
                .long("workspace")
                .short('w')
                .global(true)
                .env(store_sqlite::WORKSPACE_ENV)
                .value_name("NAME")
                .help(t(lang, "cli-workspace-help")),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .global(true)
                .env(AGENT_ENV)
                .help(t(lang, "cli-agent-help")),
        )
        .arg(
            Arg::new("token")
                .long("token")
                .global(true)
                .env(client::TOKEN_ENV)
                .hide_env_values(true)
                .help(t(lang, "cli-token-help")),
        )
        .arg(
            Arg::new("lang")
                .long("lang")
                .global(true)
                .help(t(lang, "cli-lang-help")),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help(t(lang, "cli-json-help")),
        )
        .arg(
            Arg::new("offline")
                .long("offline")
                .global(true)
                .action(ArgAction::SetTrue)
                .help(t(lang, "cli-offline-help")),
        )
        .subcommand(Command::new("commands").about(t(lang, "cli-commands-help")))
        .subcommand(Command::new("where").about(t(lang, "cli-where-help")))
        .subcommand(Command::new("workspaces").about(t(lang, "cli-workspaces-help")))
        .subcommand(
            Command::new("serve")
                .about(t(lang, "cli-serve-help"))
                .arg(
                    Arg::new("http")
                        .long("http")
                        .value_name("ADDR")
                        .num_args(0..=1)
                        .default_missing_value(mcp::http::DEFAULT_ADDR)
                        .help(t(lang, "cli-serve-http-help")),
                )
                .arg(
                    Arg::new("web")
                        .long("web")
                        .value_name("DIR")
                        .requires("http")
                        .help(t(lang, "cli-serve-web-help")),
                )
                .arg(
                    Arg::new("tls-cert")
                        .long("tls-cert")
                        .value_name("PEM")
                        .requires_all(["http", "tls-key"])
                        .help(t(lang, "cli-serve-tls-cert-help")),
                )
                .arg(
                    Arg::new("tls-key")
                        .long("tls-key")
                        .value_name("PEM")
                        .requires_all(["http", "tls-cert"])
                        .help(t(lang, "cli-serve-tls-key-help")),
                )
                .arg(
                    Arg::new("idle-exit")
                        .long("idle-exit")
                        .value_name("MIN")
                        .value_parser(clap::value_parser!(u64))
                        .help(t(lang, "cli-serve-idle-help")),
                )
                .arg(
                    Arg::new("stop")
                        .long("stop")
                        .action(ArgAction::SetTrue)
                        .help(t(lang, "cli-serve-stop-help")),
                )
                .arg(
                    Arg::new("status")
                        .long("status")
                        .action(ArgAction::SetTrue)
                        .help(t(lang, "cli-status-help")),
                ),
        )
        .subcommand(
            Command::new("migrate")
                .about(t(lang, "cli-migrate-help"))
                .arg(
                    Arg::new("from")
                        .long("from")
                        .required(true)
                        .value_name("DB")
                        .help(t(lang, "cli-migrate-from-help")),
                )
                .arg(
                    Arg::new("to")
                        .long("to")
                        .required(true)
                        .value_name("DB")
                        .help(t(lang, "cli-migrate-to-help")),
                ),
        )
        .subcommand(
            Command::new("mcp").about(t(lang, "cli-mcp-help")).arg(
                Arg::new("stdio")
                    .long("stdio")
                    .action(ArgAction::SetTrue)
                    .help(t(lang, "cli-mcp-stdio-help")),
            ),
        )
        .subcommand(
            Command::new("run")
                .about(t(lang, "cli-run-help"))
                .arg(
                    Arg::new("id")
                        .required(true)
                        .help(t(lang, "cli-run-id-help")),
                )
                .arg(
                    Arg::new("args")
                        .num_args(0..)
                        .help(t(lang, "cli-run-args-help")),
                ),
        );
    for (domain, actions) in domains() {
        let mut sub = Command::new(domain.clone())
            .about(tr(
                lang,
                &Msg::new("cli-domain-help").arg("domain", &domain),
            ))
            .subcommand_required(true);
        for c in actions {
            sub = sub.subcommand(action_command(c, lang));
        }
        if domain == "git" {
            sub = git_local(sub, lang);
        }
        cmd = cmd.subcommand(sub);
    }
    cmd
}

/// `--repo`, común a los subcomandos locales de git.
fn repo_arg(lang: Lang) -> Arg {
    Arg::new("repo")
        .long("repo")
        .value_name("DIR")
        .default_value(".")
        .help(t(lang, "cli-git-repo-help"))
}

/// Lo que corre en la máquina del repo, no en el engine (D16).
fn git_local(sub: Command, lang: Lang) -> Command {
    sub.subcommand(
        Command::new("hook")
            .about(t(lang, "cli-git-hook-help"))
            .subcommand_required(true)
            .subcommand(
                Command::new("install")
                    .about(t(lang, "cli-git-hook-install-help"))
                    .arg(repo_arg(lang)),
            )
            .subcommand(
                Command::new("uninstall")
                    .about(t(lang, "cli-git-hook-uninstall-help"))
                    .arg(repo_arg(lang)),
            ),
    )
    .subcommand(
        Command::new("scan")
            .about(t(lang, "cli-git-scan-help"))
            .arg(repo_arg(lang))
            .arg(
                Arg::new("since")
                    .long("since")
                    .value_name("REV")
                    .conflicts_with("last")
                    .help(t(lang, "cli-git-since-help")),
            )
            .arg(
                Arg::new("last")
                    .long("last")
                    .value_name("N")
                    .value_parser(clap::value_parser!(usize))
                    .help(t(lang, "cli-git-last-help")),
            )
            .arg(
                Arg::new("quiet")
                    .long("quiet")
                    .action(ArgAction::SetTrue)
                    .help(t(lang, "cli-git-quiet-help")),
            ),
    )
}

/// Comandos del registro agrupados por dominio, sin los internos.
fn domains() -> Vec<(String, Vec<&'static dyn engine::Command>)> {
    // El registro estándar vive lo que dura el proceso: la CLI es de corta vida.
    let registry: &'static Registry = Box::leak(Box::new(Registry::standard()));
    let mut out: Vec<(String, Vec<&'static dyn engine::Command>)> = Vec::new();
    for c in registry.iter() {
        let Some((domain, _)) = c.id().split_once('.') else {
            continue;
        };
        if domain == "session" || FIXED.contains(&domain) {
            continue;
        }
        match out.iter_mut().find(|(d, _)| d == domain) {
            Some((_, list)) => list.push(c),
            None => out.push((domain.to_owned(), vec![c])),
        }
    }
    out
}

fn action_command(c: &dyn engine::Command, lang: Lang) -> Command {
    let action = c.id().split_once('.').map_or(c.id(), |(_, a)| a);
    let mut sub = Command::new(action.replace('_', "-"))
        .about(t(lang, c.label()))
        .long_about(format!("{}\n\n{}", t(lang, c.label()), c.id()));
    // El primer obligatorio también se puede dar sin nombre: `task get API-1`.
    if let Some(first) = c.params().iter().find(|p| p.required) {
        sub = sub.arg(
            Arg::new("__positional")
                .index(1)
                .value_name(first.name.to_uppercase())
                .help(i18n::lookup(lang, &first.help_key()).unwrap_or_default()),
        );
    }
    for p in c.params() {
        let help = i18n::lookup(lang, &p.help_key()).unwrap_or_default();
        let mut arg = Arg::new(p.name).long(flag(p.name)).help(help);
        arg = match p.kind {
            Kind::Bool => arg
                .num_args(0..=1)
                .default_missing_value("true")
                .value_name("BOOL"),
            Kind::Enum(values) => arg.value_name(values.join("|")),
            Kind::Int => arg.value_name("N"),
            Kind::Date => arg.value_name("DATE"),
            Kind::List => arg.value_name("A,B"),
            Kind::Json => arg.value_name("JSON"),
            Kind::Text => arg.value_name(p.name.to_uppercase()),
        };
        sub = sub.arg(arg);
    }
    sub
}

pub fn parse(m: &ArgMatches, pre: Lang) -> Result<(Global, Invocation), String> {
    let lang = m
        .get_one::<String>("lang")
        .and_then(|l| Lang::from_tag(l))
        .unwrap_or(pre);
    let global = Global {
        db: m.get_one::<String>("db").map(PathBuf::from),
        workspace: m.get_one::<String>("workspace").cloned(),
        agent: m.get_one::<String>("agent").cloned(),
        token: m.get_one::<String>("token").cloned(),
        lang,
        json: m.get_flag("json"),
        offline: m.get_flag("offline"),
    };
    let invocation = match m.subcommand() {
        Some(("commands", _)) => Invocation::Commands,
        Some(("where", _)) => Invocation::Where,
        Some(("workspaces", _)) => Invocation::Workspaces,
        Some(("serve", s)) if s.get_flag("status") => Invocation::Status,
        Some(("serve", s)) => Invocation::Serve {
            http: s.get_one::<String>("http").cloned(),
            idle_exit: s.get_one::<u64>("idle-exit").copied(),
            stop: s.get_flag("stop"),
            web: s.get_one::<String>("web").map(PathBuf::from),
            tls: s
                .get_one::<String>("tls-cert")
                .zip(s.get_one::<String>("tls-key"))
                .map(|(c, k)| (PathBuf::from(c), PathBuf::from(k))),
        },
        Some(("mcp", _)) => Invocation::Mcp,
        Some(("migrate", s)) => Invocation::Migrate {
            from: s
                .get_one::<String>("from")
                .map(PathBuf::from)
                .unwrap_or_default(),
            to: s
                .get_one::<String>("to")
                .map(PathBuf::from)
                .unwrap_or_default(),
        },
        Some(("run", s)) => {
            let command = s.get_one::<String>("id").cloned().unwrap_or_default();
            let mut args = ArgMap::new();
            for pair in s.get_many::<String>("args").into_iter().flatten() {
                let Some((k, v)) = pair.split_once('=') else {
                    return Err(tr(lang, &Msg::new("cli-bad-pair").arg("pair", pair)));
                };
                args.insert(k.to_owned(), v.to_owned());
            }
            Invocation::Run { command, args }
        }
        Some(("git", sub)) if matches!(sub.subcommand_name(), Some("hook" | "scan")) => {
            let repo = |a: &ArgMatches| {
                PathBuf::from(a.get_one::<String>("repo").map_or(".", String::as_str))
            };
            match sub.subcommand() {
                Some(("hook", h)) => match h.subcommand() {
                    Some((action, a)) => Invocation::GitHook {
                        install: action == "install",
                        repo: repo(a),
                    },
                    None => return Err(t(lang, "cli-missing-action")),
                },
                Some((_, a)) => Invocation::GitScan {
                    repo: repo(a),
                    range: match a.get_one::<String>("since") {
                        Some(rev) => crate::git::Range::Since(rev.clone()),
                        None => crate::git::Range::Last(
                            a.get_one::<usize>("last").copied().unwrap_or(20),
                        ),
                    },
                    quiet: a.get_flag("quiet"),
                },
                None => return Err(t(lang, "cli-missing-action")),
            }
        }
        Some((domain, sub)) => {
            let Some((action, a)) = sub.subcommand() else {
                return Err(t(lang, "cli-missing-action"));
            };
            let id = format!("{domain}.{}", action.replace('-', "_"));
            let registry = Registry::standard();
            let Some(c) = registry.get(&id) else {
                return Err(tr(lang, &Msg::new("error-unknown-command").arg("id", &id)));
            };
            let mut args = ArgMap::new();
            if let (Some(first), Some(v)) = (
                c.params().iter().find(|p| p.required),
                a.try_get_one::<String>("__positional").ok().flatten(),
            ) {
                args.insert(first.name.to_owned(), v.clone());
            }
            for p in c.params() {
                if let Some(v) = a.get_one::<String>(p.name) {
                    args.insert(p.name.to_owned(), v.clone());
                }
            }
            Invocation::Run { command: id, args }
        }
        None => return Err(t(lang, "cli-missing-action")),
    };
    Ok((global, invocation))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(argv: &[&str]) -> Invocation {
        let m = command(Lang::En).try_get_matches_from(argv).unwrap();
        parse(&m, Lang::En).map(|(_, i)| i).ok().unwrap()
    }

    #[test]
    fn registry_commands_become_subcommands_with_flags() {
        let inv = parse_args(&[
            "taskcraft-cli",
            "task",
            "create",
            "--project",
            "API",
            "--title",
            "Fix leak",
            "--labels",
            "db,rust",
        ]);
        let Invocation::Run { command, args } = inv else {
            panic!("debía ser un comando");
        };
        assert_eq!(command, "task.create");
        assert_eq!(args["project"], "API");
        assert_eq!(args["title"], "Fix leak");
        assert_eq!(args["labels"], "db,rust");
    }

    #[test]
    fn first_required_param_is_positional_and_bools_are_flags() {
        let Invocation::Run { command, args } =
            parse_args(&["taskcraft-cli", "task", "next", "--claim"])
        else {
            panic!();
        };
        assert_eq!(command, "task.next");
        assert_eq!(args["claim"], "true");

        let Invocation::Run { command, args } = parse_args(&[
            "taskcraft-cli",
            "workspace",
            "configure",
            "--lease-minutes",
            "20",
        ]) else {
            panic!();
        };
        assert_eq!(command, "workspace.configure");
        assert_eq!(args["lease_minutes"], "20");

        let Invocation::Run { args, .. } = parse_args(&["taskcraft-cli", "task", "get", "API-1"])
        else {
            panic!();
        };
        assert_eq!(args["task"], "API-1");
    }

    #[test]
    fn raw_run_still_works() {
        let Invocation::Run { command, args } =
            parse_args(&["taskcraft-cli", "run", "task.list", "project=API"])
        else {
            panic!();
        };
        assert_eq!(command, "task.list");
        assert_eq!(args["project"], "API");
    }

    #[test]
    fn help_is_generated_for_every_domain() {
        let cmd = command(Lang::Es);
        for name in [
            "task", "project", "agent", "inbox", "plan", "lock", "status", "epic",
        ] {
            assert!(cmd.find_subcommand(name).is_some(), "falta {name}");
        }
        assert!(cmd.find_subcommand("session").is_none());
    }
}
