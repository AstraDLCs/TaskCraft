# TaskCraft

Planificador de tareas local donde **varios agentes de IA trabajan a la vez** sobre los mismos
proyectos por MCP, mientras **una persona supervisa** desde la app de escritorio o el navegador.

- **Un token por cliente LLM:** Claude Code, Cursor o Antigravity se registran como interfaz;
  dentro crean sus agentes (sin token) y cambian de identidad con `agent_use`.
- **Coordinación sin choques:** cada agente toma tareas con un lease, respeta dependencias y
  límites de trabajo en curso, y entrega a sus revisores, que votan; las notas pasan de uno a
  otro.
- **Supervisión:** cola de revisión, preguntas de los agentes, bloqueos, pausa, permisos por
  agente, historial completo y reversión selectiva.
- **Organización:** lista jerárquica, tablero, tabla editable, calendario, Gantt, vista «Hoy»,
  épicas e hitos, estados y campos propios, plantillas y vistas guardadas con un lenguaje de
  filtros (`status:todo priority>=high due<=today+3d`).
- **Integraciones:** servidor MCP (stdio y HTTP), CLI completa, automatizaciones, webhooks
  firmados, exportación `.taskcraft`, Markdown y CSV.
- **Datos:** SQLite local por defecto o PostgreSQL, copias automáticas y varios workspaces.

## Empezar

```sh
cargo build --release
./target/release/taskcraft                      # la app; el daemon arranca solo
./target/release/taskcraft-cli interface register claude-code --json   # token de la interfaz
./target/release/taskcraft-cli mcp --stdio      # servidor MCP para tu cliente
```

La guía completa está en [md/taskcraft/guia-de-prueba.md](md/taskcraft/guia-de-prueba.md):
conectar agentes por MCP, el ciclo de trabajo, la app web, PostgreSQL y los paquetes.

## Desarrollo

```sh
cargo xtask ci                     # capas, i18n, fmt, clippy y tests
TASKCRAFT_TEST_PG=postgres://… cargo xtask ci   # también contra PostgreSQL
cargo xtask bench                  # rendimiento con 50 000 tareas
cargo xtask web                    # app web (wasm) en dist/web
cargo xtask dist linux             # .tar.gz, .deb, .rpm, .flatpak (y .AppImage) en dist/
cargo xtask dist windows           # .zip (y .msi con wixl) para Windows x86_64
```

La documentación de diseño vive en [`md/`](md/README.md): requisitos, backlog, hojas de ruta,
arquitectura por capas y decisiones (D01–D19).

## Licencia

MIT o Apache-2.0, a tu elección ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
