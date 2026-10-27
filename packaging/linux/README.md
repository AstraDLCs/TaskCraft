# Empaquetado Linux

`cargo xtask dist linux [--arch x86_64|aarch64]` genera todo en `dist/` a partir del mismo
binario release (guía Craft §7.4, RNF-11a). Dónde se define cada formato:

| Formato | Definición |
|---|---|
| comunes | `common/`: `.desktop`, AppStream (`.metainfo.xml`) e icono SVG (`cargo xtask icons` lo rasteriza) |
| `.tar.gz` | `xtask/src/dist.rs` (`tarball`) |
| `.deb` | `xtask/src/dist.rs` (`deb`): el `control` y el archivo `ar`, sin herramientas externas |
| `.rpm` | `[package.metadata.generate-rpm]` en `apps/taskcraft/Cargo.toml` (`cargo-generate-rpm`) |
| `.AppImage` | `appimage/AppRun`; necesita `appimagetool` |
| `.flatpak` | `flatpak/io.github.astradlcs.TaskCraft.yml` con `flatpak-builder`, o las órdenes de bajo nivel de `flatpak` con el runtime instalado |
