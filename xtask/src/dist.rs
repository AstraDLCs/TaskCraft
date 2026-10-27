//! Empaquetado Linux (RNF-11a, guía Craft §7.4): los iconos se rasterizan
//! desde el SVG y los cinco formatos salen del mismo binario release.
//!
//! - `.tar.gz` y `.deb` se generan aquí mismo (solo hace falta `tar` y `xz`).
//! - `.rpm` usa `cargo-generate-rpm`; `.AppImage`, `appimagetool`; `.flatpak`,
//!   `flatpak-builder`. Si falta una herramienta, ese formato se salta y se dice.
//! - Con `cargo-zigbuild` se compila contra glibc 2.28 (y para aarch64).

use std::path::{Path, PathBuf};
use std::process::Command;

use cargo_metadata::MetadataCommand;

pub const APP_ID: &str = "io.github.astradlcs.TaskCraft";
const BINS: [&str; 2] = ["taskcraft", "taskcraft-cli"];
const SIZES: [u32; 4] = [48, 128, 256, 512];
const GLIBC: &str = "2.28";
const HOMEPAGE: &str = "https://github.com/AstraDLCs/TaskCraft";
const SUMMARY: &str = "Plan, coordinate and review the work of several AI agents";

fn root() -> Result<PathBuf, String> {
    let meta = MetadataCommand::new()
        .no_deps()
        .exec()
        .map_err(|e| e.to_string())?;
    Ok(meta.workspace_root.into_std_path_buf())
}

fn version() -> Result<String, String> {
    let meta = MetadataCommand::new()
        .no_deps()
        .exec()
        .map_err(|e| e.to_string())?;
    meta.packages
        .iter()
        .find(|p| p.name.as_str() == "taskcraft")
        .map(|p| p.version.to_string())
        .ok_or_else(|| "no se encontró el paquete taskcraft".into())
}

fn io(path: &Path, e: std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    }
    std::fs::copy(from, to).map(drop).map_err(|e| io(from, e))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let shown = format!("{cmd:?}");
    let status = cmd.status().map_err(|e| format!("{shown}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{shown} falló ({status})"))
    }
}

/// ¿Está esta herramienta en el PATH?
fn have(tool: &str, probe: &[&str]) -> bool {
    Command::new(tool)
        .args(probe)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `dist/icons/hicolor/<N>x<N>/apps/<id>.png` y `scalable/apps/<id>.svg`.
pub fn icons() -> Result<(), String> {
    let root = root()?;
    let svg = root.join(format!("packaging/linux/common/{APP_ID}.svg"));
    let out = root.join("dist/icons/hicolor");
    let data = std::fs::read(&svg).map_err(|e| io(&svg, e))?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())
        .map_err(|e| format!("{}: {e}", svg.display()))?;
    for size in SIZES {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
            .ok_or_else(|| format!("tamaño de icono inválido: {size}"))?;
        let scale = size as f32 / tree.size().width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let path = out.join(format!("{size}x{size}/apps/{APP_ID}.png"));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
        }
        pixmap
            .save_png(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    copy(&svg, &out.join(format!("scalable/apps/{APP_ID}.svg")))?;
    println!("✓ iconos en {}", out.display());
    Ok(())
}

/// `cargo xtask dist linux [--arch x86_64|aarch64]`.
pub fn linux(args: &[String]) -> Result<(), String> {
    let arch = match args.iter().position(|a| a == "--arch") {
        Some(i) => args.get(i + 1).map(String::as_str).unwrap_or("x86_64"),
        None => std::env::consts::ARCH,
    };
    if !matches!(arch, "x86_64" | "aarch64") {
        return Err(format!("arquitectura no soportada: {arch}"));
    }
    let root = root()?;
    let version = version()?;
    let triple = format!("{arch}-unknown-linux-gnu");
    build(&root, arch, &triple)?;
    icons()?;
    let dist = root.join("dist");
    let stage = stage(&root, &dist, arch, &triple)?;
    let mut made = vec![tarball(&dist, &stage, &version, arch)?];
    made.push(deb(&dist, &stage, &version, arch)?);
    let optional = [
        rpm(&root, &dist, &triple, &version, arch),
        appimage(&root, &dist, &stage, &version, arch),
        flatpak(&root, &dist, &stage, &version, arch),
    ];
    for result in optional {
        match result {
            Ok(Some(path)) => made.push(path),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }
    println!("\nPaquetes en {}:", dist.display());
    for p in made {
        println!("  {}", p.display());
    }
    Ok(())
}

/// `cargo xtask dist windows` (RNF-11b, D19): `.zip` siempre y `.msi` si
/// está `wixl` (msitools). Se compila con `cargo-zigbuild` para
/// `x86_64-pc-windows-gnu`.
pub fn windows() -> Result<(), String> {
    let root = root()?;
    let version = version()?;
    let triple = "x86_64-pc-windows-gnu";
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    if !have(&cargo, &["zigbuild", "--help"]) {
        return Err("compilar para Windows necesita cargo-zigbuild (y zig)".into());
    }
    let mut cmd = Command::new(&cargo);
    cmd.current_dir(&root)
        .args(["zigbuild", "--release", "--target", triple]);
    for bin in BINS {
        cmd.args(["-p", bin]);
    }
    run(&mut cmd)?;

    let dist = root.join("dist");
    let stage = dist.join("stage-windows");
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| io(&stage, e))?;
    }
    let release = root.join("target").join(triple).join("release");
    for bin in BINS {
        let exe = format!("{bin}.exe");
        copy(&release.join(&exe), &stage.join(&exe))?;
    }
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copy(&root.join(license), &stage.join(format!("{license}.txt")))?;
    }
    copy(
        &root.join("packaging/windows/README.txt"),
        &stage.join("README.txt"),
    )?;

    let mut made = vec![zip_dir(&dist, &stage, &version)?];
    if have("wixl", &["--version"]) {
        let out = dist.join(format!("taskcraft-{version}-x86_64.msi"));
        run(Command::new("wixl")
            .arg("--arch")
            .arg("x64")
            .arg("-D")
            .arg(format!("Version={version}"))
            .arg("-D")
            .arg(format!("SourceDir={}", stage.display()))
            .arg("-o")
            .arg(&out)
            .arg(root.join("packaging/windows/taskcraft.wxs")))?;
        made.push(out);
    } else {
        println!(
            "· sin wixl (msitools): se salta el .msi; en Windows, `wix build packaging/windows/taskcraft.wxs`"
        );
    }
    println!("\nPaquetes en {}:", dist.display());
    for p in made {
        println!("  {}", p.display());
    }
    Ok(())
}

/// `taskcraft-<v>-windows-x86_64.zip` con todo dentro de `taskcraft-<v>/`.
fn zip_dir(dist: &Path, stage: &Path, version: &str) -> Result<PathBuf, String> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let out = dist.join(format!("taskcraft-{version}-windows-x86_64.zip"));
    let file = std::fs::File::create(&out).map_err(|e| io(&out, e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut entries: Vec<_> = std::fs::read_dir(stage)
        .map_err(|e| io(stage, e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let data = std::fs::read(&path).map_err(|e| io(&path, e))?;
        zip.start_file(format!("taskcraft-{version}/{name}"), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| io(&path, e))?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Release de las dos apps. Con `cargo-zigbuild`, contra glibc 2.28 para que
/// corra en distros antiguas; sin él, solo para la arquitectura del host.
fn build(root: &Path, arch: &str, triple: &str) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.current_dir(root);
    if have(&cargo, &["zigbuild", "--help"]) {
        cmd.args([
            "zigbuild",
            "--release",
            "--target",
            &format!("{triple}.{GLIBC}"),
        ]);
    } else if arch == std::env::consts::ARCH {
        println!("· sin cargo-zigbuild: el binario pide la glibc de este sistema");
        cmd.args(["build", "--release", "--target", triple]);
    } else {
        return Err(format!("compilar para {arch} necesita cargo-zigbuild"));
    }
    for bin in BINS {
        cmd.args(["-p", bin]);
    }
    run(&mut cmd)
}

/// Árbol `usr/` común a todos los formatos.
fn stage(root: &Path, dist: &Path, arch: &str, triple: &str) -> Result<PathBuf, String> {
    let stage = dist.join(format!("stage-{arch}"));
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| io(&stage, e))?;
    }
    let release = root.join("target").join(triple).join("release");
    for bin in BINS {
        copy(&release.join(bin), &stage.join("usr/bin").join(bin))?;
    }
    let common = root.join("packaging/linux/common");
    copy(
        &common.join(format!("{APP_ID}.desktop")),
        &stage.join(format!("usr/share/applications/{APP_ID}.desktop")),
    )?;
    copy(
        &common.join(format!("{APP_ID}.metainfo.xml")),
        &stage.join(format!("usr/share/metainfo/{APP_ID}.metainfo.xml")),
    )?;
    let icons = dist.join("icons/hicolor");
    for size in SIZES {
        let rel = format!("{size}x{size}/apps/{APP_ID}.png");
        copy(
            &icons.join(&rel),
            &stage.join("usr/share/icons/hicolor").join(&rel),
        )?;
    }
    let svg = format!("scalable/apps/{APP_ID}.svg");
    copy(
        &icons.join(&svg),
        &stage.join("usr/share/icons/hicolor").join(&svg),
    )?;
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copy(
            &root.join(license),
            &stage.join("usr/share/doc/taskcraft").join(license),
        )?;
    }
    Ok(stage)
}

/// `taskcraft-<v>-<arch>.tar.gz` con `bin/` y `share/` dentro de una carpeta.
fn tarball(dist: &Path, stage: &Path, version: &str, arch: &str) -> Result<PathBuf, String> {
    let name = format!("taskcraft-{version}-{arch}");
    let out = dist.join(format!("{name}.tar.gz"));
    run(Command::new("tar")
        .arg("-C")
        .arg(stage)
        .args(["--owner=0", "--group=0", "--sort=name"])
        .arg(format!("--transform=s,^usr,{name},"))
        .arg("-czf")
        .arg(&out)
        .arg("usr"))?;
    Ok(out)
}

fn deb_arch(arch: &str) -> &'static str {
    if arch == "aarch64" { "arm64" } else { "amd64" }
}

/// Tamaño instalado en KiB (campo `Installed-Size`).
fn size_kib(dir: &Path) -> u64 {
    let mut total = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match entry.metadata() {
                Ok(m) if m.is_dir() => pending.push(entry.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total.div_ceil(1024)
}

/// `.deb` sin herramientas externas: un `ar` con `debian-binary`,
/// `control.tar.xz` y `data.tar.xz`.
fn deb(dist: &Path, stage: &Path, version: &str, arch: &str) -> Result<PathBuf, String> {
    let work = dist.join(format!("deb-{arch}"));
    if work.exists() {
        std::fs::remove_dir_all(&work).map_err(|e| io(&work, e))?;
    }
    std::fs::create_dir_all(&work).map_err(|e| io(&work, e))?;
    let maintainer = std::env::var("TASKCRAFT_MAINTAINER")
        .unwrap_or_else(|_| "TaskCraft contributors <taskcraft@localhost>".into());
    let control = format!(
        "Package: taskcraft\n\
         Version: {version}\n\
         Architecture: {}\n\
         Maintainer: {maintainer}\n\
         Installed-Size: {}\n\
         Depends: libc6 (>= {GLIBC}), libxkbcommon0, libxkbcommon-x11-0, libx11-6, libxcursor1, libxrandr2, libxi6, libwayland-client0, libegl1\n\
         Recommends: libvulkan1\n\
         Section: utils\n\
         Priority: optional\n\
         Homepage: {HOMEPAGE}\n\
         Description: {SUMMARY}\n \
         Local task planner where several AI agents work on the same projects\n \
         through MCP while a person supervises.\n",
        deb_arch(arch),
        size_kib(&stage.join("usr")),
    );
    let ctl = work.join("control");
    std::fs::create_dir_all(&ctl).map_err(|e| io(&ctl, e))?;
    std::fs::write(ctl.join("control"), control).map_err(|e| io(&ctl, e))?;
    let tar_xz = |dir: &Path, out: &Path| {
        run(Command::new("tar")
            .arg("-C")
            .arg(dir)
            .args(["--owner=0", "--group=0", "--sort=name", "-cJf"])
            .arg(out)
            .arg("."))
    };
    tar_xz(&ctl, &work.join("control.tar.xz"))?;
    tar_xz(stage, &work.join("data.tar.xz"))?;
    let read = |name: &str| {
        let p = work.join(name);
        std::fs::read(&p).map_err(|e| io(&p, e))
    };
    let out = dist.join(format!("taskcraft_{version}_{}.deb", deb_arch(arch)));
    let members = [
        ("debian-binary", b"2.0\n".to_vec()),
        ("control.tar.xz", read("control.tar.xz")?),
        ("data.tar.xz", read("data.tar.xz")?),
    ];
    std::fs::write(&out, ar(&members)).map_err(|e| io(&out, e))?;
    Ok(out)
}

/// Archivo `ar` (formato común) como el que espera `dpkg`.
fn ar(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            name,
            mtime,
            0,
            0,
            "100644",
            data.len()
        );
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
    out
}

/// `.rpm` con `cargo-generate-rpm` (metadatos en `apps/taskcraft/Cargo.toml`).
fn rpm(
    root: &Path,
    dist: &Path,
    triple: &str,
    version: &str,
    arch: &str,
) -> Result<Option<PathBuf>, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    if !have(&cargo, &["generate-rpm", "--version"]) {
        println!("· .rpm omitido: instala cargo-generate-rpm");
        return Ok(None);
    }
    run(Command::new(&cargo)
        .current_dir(root)
        .args([
            "generate-rpm",
            "-p",
            "apps/taskcraft",
            "--target",
            triple,
            "-o",
        ])
        .arg(dist))?;
    let prefix = format!("taskcraft-{version}-");
    let suffix = format!("{arch}.rpm");
    let found = std::fs::read_dir(dist)
        .map_err(|e| io(dist, e))?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(&suffix))
        });
    Ok(found)
}

/// `TaskCraft-<v>-<arch>.AppImage` con `appimagetool`.
fn appimage(
    root: &Path,
    dist: &Path,
    stage: &Path,
    version: &str,
    arch: &str,
) -> Result<Option<PathBuf>, String> {
    let appdir = dist.join(format!("TaskCraft-{arch}.AppDir"));
    if appdir.exists() {
        std::fs::remove_dir_all(&appdir).map_err(|e| io(&appdir, e))?;
    }
    let common = root.join("packaging/linux/common");
    for bin in BINS {
        copy(
            &stage.join("usr/bin").join(bin),
            &appdir.join("usr/bin").join(bin),
        )?;
    }
    copy(
        &common.join(format!("{APP_ID}.desktop")),
        &appdir.join(format!("{APP_ID}.desktop")),
    )?;
    copy(
        &dist.join(format!("icons/hicolor/256x256/apps/{APP_ID}.png")),
        &appdir.join(format!("{APP_ID}.png")),
    )?;
    copy(
        &root.join("packaging/linux/appimage/AppRun"),
        &appdir.join("AppRun"),
    )?;
    if !have("appimagetool", &["--version"]) {
        println!(
            "· .AppImage omitido: instala appimagetool (el AppDir queda en {})",
            appdir.display()
        );
        return Ok(None);
    }
    let out = dist.join(format!("TaskCraft-{version}-{arch}.AppImage"));
    run(Command::new("appimagetool")
        .env("ARCH", arch)
        .arg("-u")
        .arg(format!(
            "gh-releases-zsync|AstraDLCs|TaskCraft|latest|TaskCraft-*-{arch}.AppImage.zsync"
        ))
        .arg(&appdir)
        .arg(&out))?;
    Ok(Some(out))
}

/// Versión del runtime `org.freedesktop.Platform` (`TASKCRAFT_FLATPAK_RUNTIME`).
fn flatpak_runtime() -> String {
    std::env::var("TASKCRAFT_FLATPAK_RUNTIME").unwrap_or_else(|_| "25.08".into())
}

/// Permisos del sandbox: los mismos que el manifiesto de `packaging/linux/flatpak`.
const FLATPAK_FINISH: [&str; 6] = [
    "--share=ipc",
    "--share=network",
    "--socket=x11",
    "--socket=wayland",
    "--device=dri",
    "--talk-name=org.freedesktop.Notifications",
];

/// `.flatpak` a partir del binario ya compilado. Con `flatpak-builder` usa el
/// manifiesto; si no, las órdenes de bajo nivel de `flatpak` con el runtime
/// instalado (no hace falta el SDK: no se compila nada dentro).
fn flatpak(
    root: &Path,
    dist: &Path,
    stage: &Path,
    version: &str,
    arch: &str,
) -> Result<Option<PathBuf>, String> {
    let runtime = flatpak_runtime();
    let platform = format!("org.freedesktop.Platform//{runtime}");
    if !have("flatpak", &["info", "--arch", arch, &platform]) {
        println!("· .flatpak omitido: instala flatpak y el runtime {platform} ({arch})");
        return Ok(None);
    }
    let build = dist.join(format!("flatpak-{arch}"));
    let repo = dist.join("flatpak-repo");
    for dir in [&build, &repo] {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| io(dir, e))?;
        }
    }
    if have("flatpak-builder", &["--version"]) {
        let manifest = root.join(format!("packaging/linux/flatpak/{APP_ID}.yml"));
        run(Command::new("flatpak-builder")
            .args(["--force-clean", "--arch", arch, "--repo"])
            .arg(&repo)
            .arg(&build)
            .arg(&manifest))?;
    } else {
        run(Command::new("flatpak")
            .args(["build-init", "--arch", arch])
            .arg(&build)
            .args([
                APP_ID,
                "org.freedesktop.Platform",
                "org.freedesktop.Platform",
                &runtime,
            ]))?;
        let files = build.join("files");
        for bin in BINS {
            copy(
                &stage.join("usr/bin").join(bin),
                &files.join("bin").join(bin),
            )?;
        }
        run(Command::new("cp")
            .arg("-r")
            .arg(stage.join("usr/share"))
            .arg(&files))?;
        run(Command::new("flatpak")
            .arg("build-finish")
            .arg(&build)
            .arg("--command=taskcraft")
            .args(FLATPAK_FINISH))?;
        run(Command::new("flatpak")
            .args(["build-export", "--arch", arch])
            .arg(&repo)
            .arg(&build))?;
    }
    let out = dist.join(format!("TaskCraft-{version}-{arch}.flatpak"));
    run(Command::new("flatpak")
        .args(["build-bundle", "--arch", arch])
        .arg(&repo)
        .arg(&out)
        .arg(APP_ID))?;
    Ok(Some(out))
}

/// `cargo xtask web` (D14): la app web a wasm, con `wasm-bindgen`, en `dist/web`.
pub fn web() -> Result<(), String> {
    let root = root()?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    if !have("wasm-bindgen", &["--version"]) {
        return Err(
            "falta wasm-bindgen: cargo install wasm-bindgen-cli --version <la de Cargo.lock>"
                .into(),
        );
    }
    run(Command::new(&cargo).current_dir(&root).args([
        "build",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "taskcraft-web",
    ]))?;
    let out = root.join("dist/web");
    if out.exists() {
        std::fs::remove_dir_all(&out).map_err(|e| io(&out, e))?;
    }
    run(Command::new("wasm-bindgen")
        .args([
            "--target",
            "web",
            "--no-typescript",
            "--out-name",
            "taskcraft_web",
            "--out-dir",
        ])
        .arg(&out)
        .arg(root.join("target/wasm32-unknown-unknown/release/taskcraft-web.wasm")))?;
    copy(
        &root.join("packaging/web/index.html"),
        &out.join("index.html"),
    )?;
    copy(
        &root.join(format!("packaging/linux/common/{APP_ID}.svg")),
        &out.join("icon.svg"),
    )?;
    println!(
        "✓ app web en {}\n  taskcraft-cli serve --http 127.0.0.1:7417 --web {}",
        out.display(),
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ar_members_are_aligned() {
        let out = super::ar(&[("a", b"xyz".to_vec()), ("b", b"12".to_vec())]);
        assert!(out.starts_with(b"!<arch>\n"));
        // 8 + (60 + 3 + 1) + (60 + 2)
        assert_eq!(out.len(), 8 + 64 + 62);
    }
}
