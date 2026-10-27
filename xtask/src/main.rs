//! Automatizador del proyecto: `cargo xtask <tarea>`.

use std::collections::BTreeMap;
use std::process::{Command, ExitCode};

use cargo_metadata::MetadataCommand;

mod dist;

const HELP: &str = "\
uso: cargo xtask <tarea>

  layers   valida la pirámide de capas L0–L6
  i18n     comprueba que cada clave usada en el código existe en es y en
  lint     cargo fmt --check + clippy -D warnings
  test     tests del workspace (y PostgreSQL con TASKCRAFT_TEST_PG)
  ci       layers + i18n + lint + test
  bench    rendimiento con 50 000 tareas (RNF-06), en release
  icons    rasteriza el icono SVG a los tamaños de hicolor (dist/icons)
  web      app web (wasm + wasm-bindgen) en dist/web (D14)
  dist linux [--arch x86_64|aarch64]
           compila release (glibc 2.28 con cargo-zigbuild) y genera .tar.gz,
           .deb, .rpm, .AppImage y .flatpak en dist/ (alias: package)
  dist windows
           compila para x86_64-pc-windows-gnu (cargo-zigbuild) y genera el
           .zip y, si está wixl (msitools), el .msi (D19)";

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "layers" => layers(),
        "i18n" => i18n_keys(),
        "lint" => lint(),
        "test" => test(),
        "ci" => layers()
            .and_then(|()| i18n_keys())
            .and_then(|()| lint())
            .and_then(|()| test()),
        "bench" => cargo(&["run", "--release", "-p", "taskcraft", "--example", "bench"]),
        "icons" => dist::icons(),
        "web" => dist::web(),
        "dist" | "package" => {
            let rest: Vec<String> = std::env::args().skip(2).collect();
            match rest.first().map(String::as_str) {
                Some("linux") | None => dist::linux(&rest),
                Some("windows") => dist::windows(),
                Some(other) => Err(format!("`dist {other}`: usa `linux` o `windows`")),
            }
        }
        _ => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask {task}: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Cada crate de `crates/` declara `[package.metadata.craft] layer = N` y solo
/// puede depender de crates internos de capa estrictamente inferior.
fn layers() -> Result<(), String> {
    let meta = MetadataCommand::new()
        .no_deps()
        .exec()
        .map_err(|e| e.to_string())?;
    let crates_dir = meta.workspace_root.join("crates");
    let packages: Vec<_> = meta
        .workspace_packages()
        .into_iter()
        .filter(|p| p.manifest_path.starts_with(&crates_dir))
        .collect();

    let mut errors = Vec::new();
    let mut layer_of: BTreeMap<&str, u64> = BTreeMap::new();
    for pkg in &packages {
        match pkg
            .metadata
            .pointer("/craft/layer")
            .and_then(|v| v.as_u64())
        {
            Some(layer) => {
                layer_of.insert(pkg.name.as_ref(), layer);
            }
            None => errors.push(format!(
                "✗ {} no declara [package.metadata.craft] layer",
                pkg.name.as_ref() as &str
            )),
        }
    }

    for pkg in &packages {
        let name: &str = pkg.name.as_ref();
        let Some(&own) = layer_of.get(name) else {
            continue;
        };
        for dep in &pkg.dependencies {
            if let Some(&theirs) = layer_of.get(dep.name.as_str())
                && theirs >= own
            {
                errors.push(format!(
                    "✗ {name} (L{own}) depende de {} (L{theirs}): solo se permiten capas inferiores",
                    dep.name
                ));
            }
        }
    }

    if !errors.is_empty() {
        return Err(format!("\n{}", errors.join("\n")));
    }
    let mut by_layer: Vec<_> = layer_of.into_iter().collect();
    by_layer.sort_by_key(|&(name, layer)| (layer, name));
    for (name, layer) in by_layer {
        println!("  L{layer}  {name}");
    }
    println!("✓ capas correctas");
    Ok(())
}

/// Claves i18n: todo literal del código con forma `prefijo-…` (cuyo prefijo
/// usa algún mensaje del catálogo) debe existir en todos los catálogos.
fn i18n_keys() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("sin raíz del workspace")?
        .to_path_buf();
    let locales = root.join("crates/i18n/locales");
    let mut catalogs: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for entry in std::fs::read_dir(&locales).map_err(|e| e.to_string())? {
        let dir = entry.map_err(|e| e.to_string())?.path();
        let file = dir.join("taskcraft.ftl");
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let keys = text
            .lines()
            .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
            .filter_map(|l| l.split_once(" =").map(|(k, _)| k.trim().to_owned()))
            .collect();
        let lang = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_owned();
        catalogs.insert(lang, keys);
    }
    let prefixes: std::collections::BTreeSet<String> = catalogs
        .values()
        .flatten()
        .filter_map(|k| k.split_once('-').map(|(p, _)| p.to_owned()))
        .collect();

    let mut used: BTreeMap<String, String> = BTreeMap::new();
    let mut stack = vec![root.join("crates"), root.join("apps")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                for literal in text.split('"').skip(1).step_by(2) {
                    let is_key = literal.contains('-')
                        && literal.chars().all(|c| {
                            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                        })
                        && literal
                            .split_once('-')
                            .is_some_and(|(p, _)| prefixes.contains(p));
                    if is_key {
                        let at = path
                            .strip_prefix(&root)
                            .unwrap_or(&path)
                            .display()
                            .to_string();
                        used.entry(literal.to_owned()).or_insert(at);
                    }
                }
            }
        }
    }

    // Prefijos de etiquetas de `text_enum!` (`project-status` → `project-status-active`):
    // son literales del código pero no claves. Se leen de sus declaraciones
    // `("error-parse-…", "prefijo")` para no confundirlos con claves que faltan.
    let mut enum_prefixes = std::collections::BTreeSet::new();
    let mut stack = vec![root.join("crates/model/src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            for line in text.lines().filter(|l| l.contains("(\"error-parse-")) {
                if let Some(prefix) = line.split('"').nth(3) {
                    enum_prefixes.insert(prefix.to_owned());
                }
            }
        }
    }
    used.retain(|key, _| !enum_prefixes.contains(key));

    let mut errors = Vec::new();
    for (key, file) in &used {
        for (lang, keys) in &catalogs {
            if !keys.contains(key) {
                errors.push(format!("✗ `{key}` ({file}) falta en {lang}"));
            }
        }
    }
    if !errors.is_empty() {
        return Err(format!("\n{}", errors.join("\n")));
    }
    println!(
        "✓ {} claves usadas, presentes en {} idiomas",
        used.len(),
        catalogs.len()
    );
    Ok(())
}

fn lint() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ])?;
    windows_check()
}

/// El transporte de Windows (D19) compilado de forma cruzada, si el target
/// está instalado. Solo los crates sin C: el resto lo compila
/// `cargo xtask dist windows` con zig.
fn windows_check() -> Result<(), String> {
    const TARGET: &str = "x86_64-pc-windows-gnu";
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l == TARGET)
        });
    if !installed {
        println!("· Windows omitido: `rustup target add {TARGET}` para comprobar su transporte");
        return Ok(());
    }
    cargo(&[
        "clippy", "--target", TARGET, "-p", "protocol", "-p", "client", "--", "-D", "warnings",
    ])?;
    println!("✓ transporte de Windows (protocol, client) para {TARGET}");
    Ok(())
}

/// Con `TASKCRAFT_TEST_PG`, la batería del engine y del daemon corre también
/// contra PostgreSQL (D13); sin ella se avisa de que se salta.
fn test() -> Result<(), String> {
    cargo(&["test", "--workspace"])?;
    match std::env::var("TASKCRAFT_TEST_PG") {
        Ok(_) => println!("✓ batería también contra PostgreSQL (TASKCRAFT_TEST_PG)"),
        Err(_) => {
            println!("· PostgreSQL omitido: define TASKCRAFT_TEST_PG=postgres://… para probarlo")
        }
    }
    Ok(())
}

fn cargo(args: &[&str]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args(args)
        .status()
        .map_err(|e| format!("no se pudo ejecutar {cargo}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` falló", args.join(" ")))
    }
}
