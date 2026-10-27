TaskCraft para Windows
======================

taskcraft.exe       La app de escritorio. El daemon arranca solo al abrirla.
taskcraft-cli.exe   La línea de comandos y el servidor MCP para los agentes.

Primeros pasos
--------------
1. Abre taskcraft.exe y crea un proyecto.
2. Registra la interfaz de tu cliente LLM y guarda el token (se muestra una vez):
     taskcraft-cli interface register claude-code --json
3. Configura el servidor MCP en tu cliente:
     comando:   taskcraft-cli mcp --stdio
     entorno:   TASKCRAFT_AGENT_TOKEN=<el token>

Los datos se guardan en %APPDATA%\taskcraft\<workspace>\workspace.db.
El daemon escucha en una tubería con nombre (\\.\pipe\taskcraft-...) que solo
puede usar tu usuario. `taskcraft-cli serve --stop` lo detiene.

Documentación completa: https://github.com/AstraDLCs/TaskCraft
Licencia: MIT o Apache-2.0, a tu elección.
