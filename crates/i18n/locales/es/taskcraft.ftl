## TaskCraft — español.
## Cada clave debe existir también en en/taskcraft.ftl con las mismas variables
## (lo verifica el test de paridad del crate i18n).

## Nombres de valores del modelo

status-backlog = Backlog
status-todo = Por hacer
status-in_progress = En progreso
status-in_review = En revisión
status-done = Hecho
status-cancelled = Cancelado

category-pending = Pendiente
category-active = Activa
category-review = En revisión
category-done = Terminada
category-cancelled = Cancelada

priority-urgent = Urgente
priority-high = Alta
priority-medium = Media
priority-low = Baja
priority-none = Ninguna

project-status-active = Activo
project-status-archived = Archivado


agent-status-active = Activo
agent-status-disabled = Desactivado
agent-status-revoked = Revocado

## Errores al leer valores

error-parse-status = Estado inválido `{ $value }`; valores: { $expected }
error-parse-category = Categoría inválida `{ $value }`; valores: { $expected }
error-parse-priority = Prioridad inválida `{ $value }`; valores: { $expected }
error-parse-project-status = Estado de proyecto inválido `{ $value }`; valores: { $expected }
error-parse-agent-status = Estado de agente inválido `{ $value }`; valores: { $expected }
error-parse-action = Acción de historial inválida `{ $value }`; valores: { $expected }
error-parse-entity = Tipo de entidad inválido `{ $value }`; valores: { $expected }
error-parse-actor = Actor inválido `{ $value }`; usa supervisor, system, agent:<ulid> o integration:<nombre>

## Reglas del dominio

rule-project-key = Clave de proyecto inválida `{ $key }`: 2–10 caracteres A–Z o 0–9, empezando por letra
rule-amount = Importe inválido `{ $value }`: un número sin signo con hasta 6 decimales (0.42), de 0 a 1 000 000
rule-currency = Moneda inválida `{ $value }`: un código de tres letras como USD o EUR
rule-title = El título no puede estar vacío ni superar 200 caracteres
rule-color = Color inválido `{ $color }`: usa el formato #rrggbb
rule-label-name = Nombre de etiqueta inválido: 1–40 caracteres, sin comas
rule-agent-name = Nombre de agente inválido `{ $name }`: 1–40 caracteres a–z, 0–9, punto, guion o guion bajo
rule-comment-body = El comentario no puede estar vacío ni superar 10 000 caracteres
rule-transition = Transición no permitida: { $from } -> { $to }; desde { $from } se puede ir a: { $allowed }

## Almacenamiento

store-backend = Error del almacenamiento: { $detail }
store-corrupt = Dato corrupto en el almacenamiento: { $detail }
store-newer-schema = El workspace usa el esquema v{ $found }, más nuevo que el soportado (v{ $supported }); actualiza TaskCraft
store-older-schema = El workspace está en el esquema v{ $found } y esta versión usa la v{ $supported }: lo tiene abierto un daemon anterior; detenlo con `taskcraft-cli serve --stop`
store-io = Error de archivo: { $detail }

## Errores generales de comandos

error-unknown-command = Comando desconocido `{ $id }`; `commands` lista los disponibles
error-missing-arg = Falta el argumento `{ $name }`
error-invalid-arg = Argumento `{ $name }` inválido: { $reason }
error-panicked = El comando { $id } falló inesperadamente; no se guardó ningún cambio

arg-not-number = `{ $value }` no es un número entero
arg-bad-date = Fecha inválida `{ $value }`: usa 2026-11-01, 2026-11-01T17:00 o 2026-11-01T17:00-05:00[America/Lima] ({ $detail })
arg-not-bool = `{ $value }` no es true/false
arg-bad-task-ref = `{ $value }` no es una clave (API-42) ni un ULID
arg-bad-id = `{ $value }` no es un ID válido (ULID de 26 caracteres)
arg-out-of-range = `{ $value }` está fuera de rango
arg-range = Debe estar entre { $min } y { $max }
arg-empty = No puede estar vacío
arg-unknown = Argumento desconocido; válidos: { $valid }
arg-length = Debe tener entre { $min } y { $max } caracteres

## Proyectos, tareas y relaciones

notfound-project = No existe el proyecto `{ $project }`; `project.list` muestra los disponibles
notfound-task = No existe la tarea `{ $task }`
notfound-task-project = Falta el proyecto de la tarea
notfound-parent = Falta la tarea padre { $id }
notfound-label = No existe la etiqueta `{ $label }`
notfound-dependency = Ese bloqueo no existe
notfound-checklist-item = No existe el paso de checklist `{ $item }` (hay { $count })
notfound-criterion = No existe el criterio `{ $item }` (hay { $count })
task-in-trash = La tarea `{ $task }` está en la papelera; `task.restore` la recupera
task-archived = La tarea `{ $task }` está archivada; `task.unarchive` la reactiva
task-new-status = Una tarea nueva empieza en backlog o todo
hierarchy-corrupt = Jerarquía de tareas corrupta (ciclo)
project-key-taken = Ya existe un proyecto con clave { $key }
project-archived = El proyecto { $project } está archivado; `project.unarchive` lo reactiva
label-taken = Ya existe la etiqueta `{ $name }`
parent-other-project = La tarea padre pertenece a otro proyecto
parent-is-descendant = Una tarea no puede colgar de sí misma ni de una de sus subtareas
depth-exceeded = Se superaría la profundidad máxima de subtareas ({ $max }); `workspace.configure max_depth=N` la cambia
dependency-cycle = La dependencia crearía un ciclo: { $path }
archive-open-tasks = { $count ->
    [one] Solo se archivan tareas cerradas; sigue abierta: { $tasks }
   *[other] Solo se archivan tareas cerradas; siguen abiertas: { $tasks }
}
not-archived = La tarea no está archivada
parent-archived = La tarea padre también está archivada; desarchívala primero
not-in-trash = La tarea no está en la papelera
parent-in-trash = La tarea padre también está en la papelera; restáurala primero

## Agentes, sesiones y comentarios

notfound-agent = No existe el agente `{ $agent }`; `agent.list` muestra los registrados
agent-name-taken = Ya existe un agente llamado `{ $name }`
agent-disabled = El agente `{ $agent }` está desactivado y no puede actuar; el supervisor puede reactivarlo con `agent.enable`
agent-revoked = El agente `{ $agent }` está revocado y no puede actuar
agent-unknown = El agente { $id } no está registrado; el supervisor debe registrarlo con `agent.register`
agent-revoked-final = El agente `{ $agent }` está revocado; la revocación es definitiva
supervisor-only = Solo el supervisor puede hacer esto
session-agent-only = Solo un agente puede abrir o usar una sesión
notfound-session = No existe la sesión `{ $session }`
session-other-agent = La sesión pertenece a otro agente

## Descripciones de comandos

cmd-workspace-get = Ver el workspace y su configuración
cmd-workspace-configure = Configurar el workspace: max_depth (1–20), trash_days (1–3650), stale_weeks (semanas para envejecer), urgent_days (días para la matriz)
cmd-project-create = Crear proyecto: key (API), name, description
cmd-project-list = Listar proyectos; archived=true incluye los archivados
cmd-project-get = Ver un proyecto con el recuento de tareas por estado
cmd-project-update = Editar proyecto: name, description, close_policy
cmd-project-archive = Archivar un proyecto
cmd-project-unarchive = Reactivar un proyecto archivado
cmd-task-create = Crear tarea: project, title, description, priority, status, parent, start, due, labels
cmd-task-get = Ver el detalle de una tarea (clave API-42 o ULID)
cmd-task-list = Listar tareas: project, status (lista), parent (clave o root), label, archived
cmd-task-update = Editar tarea: title, description, priority, start, due (vacío borra)
cmd-task-move = Pasar una tarea a otra columna: task, status (estado base o columna) y note (mensaje para quien siga). Una columna con dueño se la asigna a ese agente; un agente que la pasa a en progreso la toma
cmd-task-set_parent = Mover una tarea bajo otra (parent) o a la raíz (parent=none)
cmd-task-assign = Dejar un único asignado: to = supervisor, agent:<id>, el nombre de un agente o none (para sumar sin quitar, task.add_assignee)
cmd-task-label = Etiquetar tarea: add y remove (listas separadas por comas)
cmd-task-link = Crear bloqueo: blocker bloquea a blocked (claves o IDs)
cmd-task-unlink = Quitar bloqueo: blocker, blocked
cmd-task-archive = Archivar una tarea terminada o cancelada (con sus subtareas)
cmd-task-unarchive = Reactivar una tarea archivada
cmd-task-auto_archive = Archivar las tareas cerradas sin cambios en `days` días (por defecto 30)
cmd-task-delete = Mover una tarea (con sus subtareas) a la papelera
cmd-task-restore = Recuperar una tarea de la papelera
cmd-trash-list = Ver la papelera
cmd-trash-purge = Borrar definitivamente lo que lleva más de `days` días en la papelera (por defecto, el plazo del workspace)
cmd-checklist-add = Añadir paso al checklist: task, text
cmd-checklist-check = Marcar paso del checklist: task, item (número o ID), done (por defecto true)
cmd-checklist-remove = Quitar paso del checklist: task, item
cmd-criteria-add = Añadir criterio de aceptación: task, text
cmd-criteria-check = Marcar criterio cumplido: task, item (número o ID), done (por defecto true)
cmd-criteria-remove = Quitar criterio de aceptación: task, item
cmd-label-create = Crear etiqueta: name, color (#rrggbb)
cmd-label-list = Listar etiquetas del workspace
cmd-agent-register = Añadir un agente a una interfaz (solo supervisor; el token es de la interfaz): name, interface, model, description (su función), color, capabilities, level
cmd-agent-list = Listar agentes y si están conectados
cmd-agent-get = Ver un agente: agent (nombre o ID)
cmd-agent-update = Editar agente (solo supervisor): agent, interface (moverlo a otra), model, description (su función), color, level, scope, capabilities
cmd-agent-disable = Desactivar un agente temporalmente (solo supervisor)
cmd-agent-enable = Reactivar un agente desactivado (solo supervisor)
cmd-agent-revoke = Revocar un agente para siempre; su historial se conserva (solo supervisor)
cmd-session-open = Abrir una sesión del agente que llama: transport
cmd-session-heartbeat = Renovar el latido de una sesión: session
cmd-session-close = Cerrar una sesión: session
cmd-activity-list = Ver el historial: task, project, actor, limit, before
cmd-comment-add = Comentar una tarea: task, body (Markdown)
cmd-comment-list = Ver los comentarios de una tarea: task
cmd-test-boom = Fallo simulado (solo tests)

## Parámetros de comandos

param-max_depth = Profundidad máxima de subtareas (1–20)
param-trash_days = Días que lo eliminado se puede restaurar (1–3650)
param-key = Clave corta del proyecto, por ejemplo API
param-name = Nombre
param-description = Descripción en Markdown
param-archived = Incluir archivados (true/false)
param-project = Proyecto: clave (API) o ID
param-title = Título (1–200 caracteres)
param-priority = Prioridad: urgent, high, medium, low o none
param-status = Estado: backlog, todo, in_progress, in_review, done, cancelled o una etapa del proyecto
param-parent = Tarea padre: clave (API-42), ID, o none/root según el comando
param-start = Fecha de inicio (vacío la borra)
param-due = Fecha de vencimiento (vacío la borra)
param-labels = Etiquetas separadas por comas
param-task = Tarea: clave (API-42) o ID
param-label = Nombre de etiqueta
param-revision = Revisión esperada; si la tarea cambió desde entonces se rechaza con CONFLICT
param-to = Asignado: supervisor, agent:<id>, integration:<nombre> o none
param-add = Etiquetas a añadir, separadas por comas
param-remove = Etiquetas a quitar, separadas por comas
param-text = Texto del paso o criterio
param-item = Número (1 = primero) o ID del ítem
param-done = Marcado (true) o desmarcado (false); por defecto true
param-color = Color #rrggbb
param-blocker = Tarea que bloquea
param-blocked = Tarea bloqueada
param-agent = Agente: nombre o ID
param-model = Modelo del agente o del trabajo reportado (texto libre)
param-transport = Transporte de la sesión: cli, stdio, http…
param-session = ID de la sesión
param-actor = Actor: supervisor, system, nombre de agente o integration:<nombre>
param-action = Acciones separadas por comas: created, updated, deleted, linked, unlinked
param-entity = Tipo de entidad: workspace, project, task, label, dependency, comment, agent o inbox
param-limit = Máximo de resultados
param-before = Cursor: solo entradas anteriores a este ID
param-body = Texto en Markdown

## Coordinación

conflict-revision = La tarea { $task } cambió (revisión { $current }, esperabas { $expected }); vuelve a leerla con `task.get` y reintenta
conflict-changed-by-other = Otro actor (normalmente el supervisor) cambió { $task } desde tu última escritura; revisa el estado actual y reintenta con revision={ $current }
locked-project = El proyecto de { $task } está bloqueado por el supervisor; puedes leerlo pero no modificarlo
locked-task = { $task } está bloqueada por el supervisor; puedes leerla pero no modificarla
needs-human-pending = { $task } espera una respuesta del supervisor; elige otra tarea con `task.next`
claimed-by-other = { $task } la tiene tomada { $agent } hasta { $until }; elige otra con `task.next`
agent-only = Solo un agente puede hacer esto
not-claimable = { $task } está en estado { $status } y no se puede tomar
not-ready-blocked = { $task } no está lista: la bloquean { $blockers }
agent-wip-limit = Ya tienes { $limit } tarea(s) en progreso ({ $tasks }); termínala, entrégala o libérala antes de tomar otra
not-your-claim = No tienes tomada { $task }; tómala con `task.claim`
answer-no-question = { $task } no tiene ninguna pregunta pendiente
lock-target-required = Indica task= o project= (uno de los dos)
plan-invalid = El plan no es válido: { $detail }
plan-duplicate-ref = La referencia `{ $ref }` aparece más de una vez en el plan
plan-unresolved = No se pudieron resolver estos padres (no existen o forman un ciclo): { $refs }
notfound-inbox-item = No existe el hallazgo `{ $item }`
inbox-not-open = El hallazgo `{ $item }` ya fue convertido o descartado
error-parse-inbox-kind = Tipo de hallazgo inválido `{ $value }`; valores: { $expected }
error-parse-inbox-status = Estado de hallazgo inválido `{ $value }`; valores: { $expected }
inbox-kind-idea = Idea
inbox-kind-bug = Bug
inbox-kind-debt = Deuda técnica
inbox-status-open = Abierto
inbox-status-converted = Convertido
inbox-status-discarded = Descartado
activity-lease-expired = Lease expirado: { $target } vuelve a estar disponible
cmd-task-claim = Tomar una tarea (solo agentes): queda asignada con un lease que hay que renovar
cmd-task-heartbeat = Renovar el lease de una tarea tomada
cmd-task-release = Liberar una tarea tomada; los agentes deben dar un motivo (reason)
cmd-system-expire_leases = Liberar las tareas cuyo lease venció (lo ejecuta el sistema)
cmd-task-next = Siguiente tarea para ti: primero la de tus columnas, luego la que tienes asignada, luego la de mayor prioridad; claim=true la toma en el mismo paso
cmd-task-submit_review = Entregar: pasa a En revisión con summary (lo hecho) y criteria (criterios cumplidos); nadie tiene que aprobarla
cmd-task-escalate = Pedir ayuda al supervisor: question; la tarea queda bloqueada para agentes
cmd-task-answer = Responder a una tarea escalada (solo supervisor): answer
cmd-lock-set = Bloquear una tarea o proyecto para los agentes (solo supervisor)
cmd-lock-clear = Quitar el bloqueo humano de una tarea o proyecto (solo supervisor)
cmd-plan-create = Crear un árbol de tareas en una sola operación: items (JSON con ref, title, parent, blocked_by…)
cmd-inbox-add = Dejar un hallazgo en la bandeja: title, body, kind (idea, bug, debt)
cmd-inbox-list = Ver la bandeja de hallazgos (por defecto, los abiertos)
cmd-inbox-convert = Convertir un hallazgo en tarea de un proyecto (solo supervisor)
cmd-inbox-discard = Descartar un hallazgo (solo supervisor)
param-reason = Motivo, para el historial
param-summary = Resumen de lo hecho (Markdown)
param-criteria = Criterios cumplidos: números o IDs separados por comas
param-question = Pregunta para el supervisor
param-answer = Respuesta para el agente
param-claim = true para tomar la tarea en el mismo paso
param-items = Lista JSON de elementos: ref, title, description, priority, status, parent, blocked_by, labels, checklist, criteria, start, due
param-kind = Tipo: idea, bug o debt (bandeja); branch, commit o pr (git); claude-code, cursor… (interfaz); checklist o criteria (task.check); epic o milestone; task o project (plantillas); campos: text, number, select…
param-lease_minutes = Minutos que dura un claim sin latido (1–1440)
param-agent_wip_limit = Tareas en progreso a la vez por agente (1–50)
param-auto_archive_days = Días sin cambios para archivar solas las tareas cerradas (1–3650)

cmd-label-update = Editar etiqueta: label (nombre actual), name, color
cmd-checklist-edit = Cambiar el texto de un paso del checklist: task, item, text
cmd-criteria-edit = Cambiar el texto de un criterio de aceptación: task, item, text
cmd-task-reject = Rechazar una entrega (solo supervisor): reason; keep_assignee=true la devuelve al mismo agente
param-keep_assignee = true para devolverla al mismo agente
reject-not-in-review = { $task } no está en revisión

## Historial

activity-created = { $actor } creó { $target }
activity-updated = { $actor } cambió { $fields } en { $target }
activity-deleted = { $actor } eliminó { $target }
activity-linked = { $actor } enlazó { $target }
activity-commented = { $actor } comentó en { $target }
activity-unlinked = { $actor } desenlazó { $target }

## Nombres de campos en el historial (opcionales: sin traducción se muestra el nombre técnico)

field-title = título
field-description = descripción
field-status = estado
field-priority = prioridad
field-start = inicio
field-due = vencimiento
field-labels = etiquetas
field-assignee = asignado
field-parent_id = tarea padre
field-checklist = checklist
field-criteria = criterios
field-archived_at = archivado
field-deleted_at = papelera
field-name = nombre
field-model = modelo
field-color = color
field-close_policy = política de cierre
field-settings = configuración
field-key = clave
field-body = texto
field-claim = toma
field-needs_human = pregunta al supervisor
field-locked = bloqueo humano
field-revision = revisión
field-kind = tipo
field-task_id = tarea

## App de escritorio

ui-projects = Proyectos
ui-no-projects = Aún no hay proyectos
ui-new-project = Nuevo proyecto
ui-key-hint = Prefijo de las tareas
ui-key-explain = Identifica sus tareas: con WEB se llaman WEB-1, WEB-2… (2–10 letras o dígitos)
ui-name-hint = Nombre
ui-create-project = Crear proyecto
ui-empty-hint = Crea un proyecto a la izquierda, o desde la terminal:
ui-new-task-hint = Nueva tarea…
ui-create-task = Crear tarea
ui-no-tasks = Sin tareas.
ui-col-key = Clave
ui-col-title = Título
ui-col-status = Estado
ui-col-priority = Prioridad
ui-col-labels = Etiquetas
ui-col-checklist = Checklist
ui-blocked = bloqueada
ui-blocked-hint = Tiene bloqueadores abiertos (task.get muestra cuáles)
ui-review = revisar
ui-no-description = (sin descripción)
ui-ready = Listo
ui-command-ok = Hecho: { $id }
ui-delete-task = Eliminar tarea
ui-task-deleted = { $task } eliminada
ui-undo = Deshacer
ui-activity = Actividad
ui-filter-all = Todos
ui-filter-actor = Quién
ui-filter-project = Proyecto
ui-filter-event = Tipo
ui-event-created = Creaciones
ui-event-updated = Cambios
ui-event-deleted = Eliminaciones
ui-event-links = Dependencias
ui-event-comments = Comentarios
ui-no-activity = Todavía no hay actividad
ui-activity-count = { $count ->
    [one] 1 cambio
   *[other] { $count } cambios
}
ui-language = Idioma
ui-open-failed = No se pudo abrir el workspace: { $error }

ui-menu-file = Archivo
ui-menu-view = Ver
ui-menu-project = Proyecto
ui-menu-workspace = Workspace
ui-menu-help = Ayuda
ui-new-task = Nueva tarea
ui-palette = Paleta de comandos
ui-palette-hint = Busca un comando…
ui-run = Ejecutar
ui-quit = Salir
ui-view-list = Lista
ui-view-board = Tablero
ui-view-calendar = Calendario
ui-view-review = Revisión
ui-view-inbox = Bandeja
ui-view-agents = Agentes
ui-view-trash = Papelera
ui-hide-closed = Ocultar cerradas
ui-show-archived = Mostrar archivadas
ui-theme = Tema
ui-theme-system = Del sistema
ui-theme-light = Claro
ui-theme-dark = Oscuro
ui-edit-project = Editar proyecto
ui-lock = Bloquear para agentes
ui-unlock = Quitar bloqueo
ui-archive = Archivar
ui-unarchive = Reactivar
ui-settings = Configuración
ui-labels = Etiquetas
ui-shortcuts = Atajos de teclado
ui-all-projects = Todos los proyectos
ui-lanes-none = Sin carriles
ui-lanes-assignee = Carriles por asignado
ui-lanes-priority = Carriles por prioridad
ui-lanes-project = Carriles por proyecto
ui-roots-only = Solo tareas principales
ui-pick-project = Elige un proyecto a la izquierda para crear tareas
ui-search-hint = Buscar…
ui-any-status = todos
ui-any-priority = Cualquier prioridad
ui-any-label = Cualquier etiqueta
ui-anyone = Cualquiera
ui-unassigned = Sin asignar
ui-clear-filters = Quitar filtros
ui-col-assignee = Asignado
ui-col-due = Vence
ui-col-start = Inicio
ui-col-progress = Progreso
ui-expand = Desplegar
ui-collapse = Plegar
ui-add-subtask = Añadir subtarea
ui-new-subtask-hint = Título de la subtarea (Enter para crear)
ui-subtask-of = sub de { $parent }
ui-lease-left = vence en { $n } min
ui-ago-now = ahora
ui-ago-min = hace { $n } min
ui-ago-hours = hace { $n } h
ui-ago-days = hace { $n } d
ui-locked-hint = Bloqueada por el supervisor: los agentes no pueden modificarla
ui-no-date = sin fecha
ui-today = Hoy
ui-clear-date = Quitar fecha
ui-previous = Anterior
ui-next = Siguiente
ui-month = Mes
ui-week = Semana
ui-wd-mon = lun
ui-wd-tue = mar
ui-wd-wed = mié
ui-wd-thu = jue
ui-wd-fri = vie
ui-wd-sat = sáb
ui-wd-sun = dom
ui-month-1 = enero
ui-month-2 = febrero
ui-month-3 = marzo
ui-month-4 = abril
ui-month-5 = mayo
ui-month-6 = junio
ui-month-7 = julio
ui-month-8 = agosto
ui-month-9 = septiembre
ui-month-10 = octubre
ui-month-11 = noviembre
ui-month-12 = diciembre
ui-new-task-on-day = Nueva tarea este día
ui-more = +{ $n } más
ui-unscheduled = Sin fecha
ui-unscheduled-hint = Arrástralas a un día para programarlas
ui-unscheduled-open-hint = Ábrelas para ponerles fecha
ui-review-questions = Preguntas de agentes
ui-review-submissions = Entregas por revisar
ui-review-none = Nada pendiente
ui-answer = Responder
ui-answer-hint = Tu respuesta para el agente…
ui-approve = Aprobar
ui-reject = Rechazar
ui-reject-hint = Qué falta o qué corregir…
ui-reject-keep = Devolverla al mismo agente
ui-cancel = Cancelar
ui-criteria = Criterios de aceptación
ui-checklist = Checklist
ui-inbox-empty = La bandeja está vacía
ui-convert = Convertir en tarea
ui-discard = Descartar
ui-agent-edit = Editar agente
ui-agents-connected = { $n } de { $total } agentes conectados
ui-agents-empty = Aún no hay interfaces ni agentes
ui-connected = conectado
ui-idle = inactivo
ui-last-seen = visto
ui-agent-no-work = No tiene tareas tomadas
ui-edit = Editar
ui-disable = Desactivar
ui-enable = Reactivar
ui-revoke = Revocar
ui-trash-hint = Lo eliminado se puede restaurar durante { $days } días
ui-trash-empty = La papelera está vacía
ui-trash-empty-now = Vaciar papelera
ui-restore = Restaurar
ui-close = Cerrar
ui-claimed-since = la tomó { $ago }
ui-release = Liberar
ui-release-reason = Motivo (opcional)
ui-needs-human = Necesita una respuesta tuya
ui-remove = Quitar
ui-add-label = Añadir etiqueta
ui-new-label-hint = Nueva etiqueta…
ui-add = Añadir
ui-parent = Tarea padre
ui-root = Ninguna (tarea principal)
ui-created-by = Creada por
ui-description = Descripción
ui-save = Guardar
ui-markdown-hint = Admite Markdown
ui-add-item-hint = Añadir y pulsar Enter…
ui-item-removed = Quitado: { $item }
ui-subtasks = Subtareas
ui-dependencies = Dependencias
ui-blocked-by = Bloqueada por
ui-blocks = Bloquea a
ui-add-blocker = Añadir bloqueador…
ui-unlink = Quitar dependencia
ui-unlinked = { $blocker } ya no bloquea a { $blocked }
ui-comments = Comentarios
ui-comment = Comentar
ui-comment-hint = Escribe un comentario (Markdown)…
ui-history = Historial
ui-confirm = Confirmar
ui-confirm-revoke = ¿Revocar a { $agent } para siempre? Su historial se conserva, pero no podrá volver a actuar.
ui-shortcut-palette = Paleta de comandos
ui-shortcut-new-task = Escribir una tarea nueva
ui-shortcut-delete = Eliminar la tarea seleccionada (con aviso para deshacer)
ui-shortcut-escape = Cerrar diálogo o deseleccionar

## CLI

cli-about = Ejecuta comandos de TaskCraft desde la terminal
cli-db-help = Base de datos del workspace (por defecto TASKCRAFT_DB o el directorio de datos del usuario)
cli-as-help = Actuar como supervisor (por defecto)
cli-agent-help = Actuar como el agente registrado con este nombre (también TASKCRAFT_AGENT)
cli-lang-help = Idioma de la salida: es o en (por defecto, el del sistema; inglés para agentes)
cli-commands-help = Lista los comandos registrados
cli-where-help = Muestra la ruta de la base de datos en uso
cli-run-help = Ejecuta un comando: run task.create project=API title="Corregir leak"
cli-run-id-help = ID del comando (dominio.accion)
cli-run-args-help = Argumentos clave=valor
cli-bad-pair = Argumento inválido `{ $pair }`: se esperaba clave=valor
cli-open-failed = No se pudo abrir { $path }: { $error }

# Tamaño del texto
ui-text-size = Tamaño del texto
ui-text-small = Pequeño
ui-text-normal = Normal
ui-text-large = Grande
ui-text-huge = Muy grande

# Subtareas: cambiar el padre
ui-open = Abrir
ui-search-task = Buscar por clave o título…
ui-no-matches = Ninguna tarea coincide
ui-make-subtask-of = Hacer subtarea de { $parent }
ui-outdent = Subir un nivel
ui-make-root = Convertir en tarea principal
ui-sidebar-expand = Mostrar la barra lateral
ui-sidebar-collapse = Plegar la barra lateral
ui-tasks = Tareas
ui-back-to = Volver a { $view }
ui-filters = Filtros
ui-filters-count = Filtros ({ $count })
ui-show-activity = Mostrar u ocultar la actividad
ui-move-under = Mover dentro de…
ui-move-up = Subir
ui-move-down = Bajar
ui-open-link = Abrir el enlace
ui-drag-to-nest = Arrastra sobre otra tarea para convertirla en su subtarea
ui-drop-to-make-root = Suelta aquí para convertirla en tarea principal
ui-shortcut-indent = Hacer subtarea de la tarea anterior
ui-shortcut-outdent = Subir la subtarea un nivel
ui-detach-subtask = Sacar de esta tarea (sube un nivel)
ui-adopt-subtask = Añadir tarea existente…

# M2: daemon, MCP y CLI
arg-one-of = `{ $value }` no es válido; usa uno de: { $valid }
agent-token-unavailable = El sistema no pudo generar un token seguro; inténtalo de nuevo
cmd-task-search = Buscar tareas por texto en título, descripción y comentarios: query, project
cmd-task-check = Marcar un paso del checklist o un criterio: kind (checklist o criteria), item, done
param-cursor = Cursor de paginación devuelto como next_cursor
param-fields = Campos a devolver de cada elemento, separados por comas
param-query = Texto a buscar
agent-bad-token = Token inválido, o su interfaz está revocada o desactivada; el supervisor puede emitir otro con `interface.rotate_token`
cli-token-help = Token del agente (también TASKCRAFT_AGENT_TOKEN); identifica al agente sin nombre
cli-json-help = Imprime la respuesta en JSON
cli-offline-help = Abre la base en este proceso, sin daemon (para depurar)
cli-status-help = Indica si el daemon de este workspace está en marcha
cli-serve-help = Arranca el daemon del workspace (dueño único del engine)
cli-serve-http-help = Sirve también MCP por HTTP en esta dirección (por defecto 127.0.0.1:7417)
cli-serve-idle-help = Se detiene tras estos minutos sin clientes
cli-serve-stop-help = Detiene el daemon en marcha
cli-mcp-help = Servidor MCP para agentes
cli-mcp-stdio-help = Por stdin/stdout, como proxy del daemon (por defecto)
cli-domain-help = Comandos de { $domain }
cli-missing-action = Falta el subcomando; usa --help para ver los disponibles
cli-more = Mostrando { $shown } de { $total }; siguientes con --cursor { $cursor }
cli-daemon-running = Daemon en marcha (PID { $pid }) en { $socket }
cli-daemon-not-running = No hay daemon en { $socket }; se arranca solo al usar cualquier comando
cli-daemon-stopped = Daemon detenido
cli-daemon-listening = Daemon escuchando en { $socket }
cli-mcp-http = MCP por HTTP en { $url } (Authorization: Bearer <token de agente>)
serve-already-running = Ya hay un daemon para { $db }
serve-io = Error de entrada/salida del daemon: { $detail }
client-unavailable = No se pudo conectar con el daemon ({ $socket }): { $detail }
client-rejected = El daemon rechazó la conexión: { $detail }
client-io = Se perdió la conexión con el daemon: { $detail }
client-protocol = Respuesta inesperada del daemon: { $detail }
agent-token-required = Este daemon exige token para actuar como agente (TASKCRAFT_AGENT_TOKEN)
ui-local-mode = Sin daemon, trabajando en este proceso: { $reason }
ui-workspace-busy = Otro proceso tiene abierto { $db } y no responde ({ $reason }); detenlo con `taskcraft-cli serve --stop` y vuelve a abrir TaskCraft

# M4: exportación e importación
cmd-workspace-export = Exportar el workspace (o algunos proyectos) a un archivo .taskcraft: path, projects
cmd-workspace-import_preview = Ver qué crearía una importación, qué colisiona y qué errores hay, sin tocar nada: path, mode
cmd-workspace-import = Importar un .taskcraft en una sola transacción: mode workspace (vacío), project (como proyectos nuevos) o merge (fusión por ULID)
param-path = Ruta del archivo
param-projects = Claves de proyectos separadas por comas (vacío = todo el workspace)
param-mode = workspace (en un workspace vacío, con los mismos IDs), project (como proyectos nuevos) o merge (actualiza por ULID lo más reciente y crea lo que falta)
exchange-not-taskcraft = No es un archivo .taskcraft válido: { $detail }
exchange-newer-format = El archivo es del formato { $found } y esta versión solo entiende hasta el { $supported }; actualiza TaskCraft
exchange-invalid = El archivo tiene { $count } problemas; el primero: { $first }
exchange-target-not-empty = Para importar como workspace hace falta un workspace vacío; usa mode=project para añadirlo como proyectos nuevos
exchange-io = No se pudo leer o escribir el archivo: { $detail }
ui-export = Exportar…
ui-import = Importar…
ui-export-only = Solo el proyecto { $project }
ui-export-done = Exportadas { $tasks } tareas a { $path }
ui-import-workspace = En este workspace (vacío)
ui-import-projects = Como proyectos nuevos
ui-import-preview = Vista previa
ui-import-done = Importación hecha
ui-import-hint = Pulsa «Vista previa» para ver qué se importaría
ui-import-counts = { $projects } proyectos, { $tasks } tareas, { $comments } comentarios, { $agents } agentes, { $activity } entradas de historial

# M5: modelo ampliado
estimate-unit-hours = Horas
estimate-unit-points = Puntos
estimate-unit-size = Talla (S/M/L/XL)
field-kind-text = Texto
field-kind-number = Número
field-kind-select = Selección
field-kind-multiselect = Selección múltiple
field-kind-bool = Sí/no
field-kind-date = Fecha
field-kind-url = Enlace
epic-kind-epic = Épica
epic-kind-milestone = Hito
epic-status-open = Abierta
epic-status-closed = Cerrada
template-kind-task = Tarea
template-kind-project = Proyecto
relation-relates = Relacionada
relation-duplicates = Duplicada de
error-parse-estimate-unit = Unidad de estimación inválida `{ $value }`; valores: { $expected }
error-parse-field-kind = Tipo de campo inválido `{ $value }`; valores: { $expected }
error-parse-epic-kind = Tipo inválido `{ $value }`; valores: { $expected }
error-parse-epic-status = Estado de épica inválido `{ $value }`; valores: { $expected }
error-parse-template-kind = Tipo de plantilla inválido `{ $value }`; valores: { $expected }
error-parse-relation = Relación inválida `{ $value }`; valores: { $expected }
rule-unknown-status = El estado `{ $key }` no existe en este proyecto; usa uno de: { $valid }
rule-stage-key = Nombre de etapa inválido `{ $name }`: usa letras y números (hasta 40)
rule-stage-exists = Ya existe una columna `{ $key }` en el flujo
rule-estimate = Estimación inválida `{ $value }` para la unidad { $unit }: un número (horas, puntos) o S, M, L, XL (tallas)
rule-field-key = Nombre de campo inválido `{ $name }`
rule-field-exists = Ya existe un campo `{ $key }` en este proyecto
rule-field-options = Las opciones son obligatorias en los campos de selección (y solo en ellos), sin comas ni repetidas
rule-unknown-field = El campo `{ $key }` no existe; campos: { $valid }
rule-field-value = Valor inválido `{ $value }` para el campo { $field } ({ $kind })
notfound-epic = No existe la épica o el hito `{ $epic }`
epic-ambiguous = Hay varias épicas llamadas `{ $epic }`; usa su ID o indica el proyecto
notfound-status = No existe la columna `{ $status }`; columnas: { $valid }
status-base-permanent = `{ $status }` es un estado base: se puede renombrar o mover, pero no borrar
template-invalid = Definición de plantilla inválida: { $detail }
template-name-taken = Ya existe una plantilla llamada `{ $name }`
notfound-template = No existe la plantilla `{ $template }`
attachment-bad-base64 = El contenido no es base64 válido
attachment-source = Indica el contenido con data (base64) o con path (archivo local), pero no ambos
attachment-too-big = El adjunto ocupa { $size } KB y el máximo es { $max } MB (workspace.configure max_attachment_mb)
notfound-attachment = No existe el adjunto `{ $attachment }`
relation-self = Una tarea no se puede relacionar consigo misma
notfound-relation = Esas tareas no tienen esa relación
locked-epic = { $task } está en { $epic }, bloqueada por el supervisor: los agentes pueden leerla pero no modificarla
cmd-status-list = Columnas del flujo de un proyecto: estados base y etapas, con su número de tareas
cmd-status-create = Crear una columna en el flujo de un proyecto: name, base (estado del que cuelga), owner (agente dueño: su cola o su mesa de trabajo) y position
cmd-status-update = Renombrar, mover o cambiar de dueño una columna: status, name, owner, position
cmd-status-delete = Borrar una columna propia (nivel planificar); sus tareas se quedan en el estado base
cmd-field-list = Campos personalizados de un proyecto
cmd-field-create = Crear un campo personalizado (solo supervisor): name, kind, options
cmd-field-update = Cambiar el nombre o las opciones de un campo (solo supervisor)
cmd-field-delete = Borrar un campo y sus valores (solo supervisor)
cmd-task-set_field = Dar valor a un campo personalizado de una tarea (vacío lo quita): field, value
cmd-epic-create = Crear una épica o un hito: project, name, kind, description, target
cmd-epic-update = Editar una épica o un hito: name, description, target
cmd-epic-close = Cerrar una épica o un hito
cmd-epic-reopen = Reabrir una épica o un hito
cmd-epic-delete = Borrar una épica o un hito (solo supervisor); sus tareas se quedan sin él
cmd-epic-list = Épicas e hitos con su progreso: project, kind, status
cmd-epic-get = Una épica o un hito con su progreso y sus tareas
cmd-task-set_epic = Poner una tarea en una épica (none la quita)
cmd-task-set_milestone = Poner una tarea en un hito (none la quita)
cmd-template-create = Crear una plantilla (solo supervisor): name, kind, definition (JSON con items y, en proyectos, project)
cmd-template-from_task = Guardar una tarea con sus subtareas como plantilla (solo supervisor): task, name
cmd-template-list = Plantillas disponibles
cmd-template-get = Una plantilla con su definición
cmd-template-delete = Borrar una plantilla (solo supervisor)
cmd-template-instantiate = Crear tareas (o un proyecto nuevo) a partir de una plantilla: template, project, parent, key
cmd-attachment-add = Adjuntar un archivo a una tarea: data (base64) o path, name, mime
cmd-attachment-list = Adjuntos de una tarea
cmd-attachment-get = Descargar un adjunto: en base64 o guardado en path
cmd-attachment-remove = Quitar un adjunto
cmd-task-relate = Relacionar dos tareas: kind relates (relacionada) o duplicates (la tarea es duplicada de other y se cancela)
cmd-task-unrelate = Quitar una relación entre dos tareas
param-base = Estado base: backlog, todo, in_progress, in_review, done o cancelled
param-position = Posición en el flujo (1 = primera columna)
param-options = Opciones separadas por comas (campos de selección)
param-field = Campo personalizado: clave o nombre
param-value = Valor (vacío lo quita)
param-epic = Épica: nombre o ID (none la quita)
param-milestone = Hito: nombre o ID (none lo quita)
param-estimate = Estimación en la unidad del proyecto: número (horas, puntos) o S, M, L, XL
param-estimate_unit = Unidad de estimación: hours, points o size
param-target = Fecha objetivo
param-template = Plantilla: nombre o ID
param-definition = JSON de la plantilla: "items" (los elementos de un plan.create) y, en proyectos, "project" (nombre, descripción, flujo, campos y unidad)
param-other = La otra tarea: clave (API-42) o ID
param-data = Contenido en base64
param-mime = Tipo MIME (si no, por la extensión)
param-attachment = ID del adjunto
param-max_attachment_mb = Tamaño máximo de un adjunto en MB (1–1024)
ui-any-epic = Cualquier épica
ui-attach = Adjuntar
ui-attach-drop = También puedes soltar archivos sobre la ventana. «Guardar» los deja en { $home }.
ui-attach-hint = Ruta de un archivo
ui-attachments = Adjuntos
ui-base-status = estado base
ui-col-project = Proyecto
ui-duplicated-by = Duplicadas de esta
ui-epic = Épica
ui-epic-close = Cerrar
ui-epic-name-hint = Nombre de la épica o el hito
ui-epic-progress = { $done } de { $total } hechas
ui-epic-reopen = Reabrir
ui-epic-tasks = Ver sus tareas
ui-estimate = Estimación
ui-estimate-unit = Unidad de estimación
ui-fields = Campos propios
ui-lanes-epic = Por épica
ui-links = Vínculos
ui-mentioned-by = Mencionada en
ui-mention-hint = Escribe [[API-42]] en una descripción o comentario para enlazar otra tarea.
ui-milestone = Hito
ui-new-field-hint = Campo nuevo
ui-new-stage-hint = Etapa nueva
ui-no-epic = Sin épica
ui-no-epics = Todavía no hay épicas ni hitos
ui-none = Ninguno
ui-no-templates = Todavía no hay plantillas: guarda una tarea con sus subtareas desde abajo.
ui-planning = Planificación
ui-related = Relacionadas
ui-relate-with = Relacionar con…
ui-save-as = Guardar
ui-template-apply = Aplicar plantilla
ui-templates = Plantillas…
ui-template-save = Guardar { $task } como plantilla:
ui-view-epics = Épicas e hitos
ui-workflow = Flujo
ui-workflow-fields = Flujo y campos…
ui-workflow-hint = Renombra o reordena las columnas y añade etapas propias; cada etapa cuelga de un estado base, que es lo que entienden los agentes.
ui-delete = Eliminar
field-stage = etapa
field-fields = campos
field-estimate = estimación
field-epic_id = épica
field-milestone_id = hito
field-workflow = flujo
field-estimate_unit = unidad de estimación
field-target = fecha objetivo
field-max_attachment_mb = tamaño máximo de adjuntos

# M6: permisos y supervisión
agent-level-read = Solo lectura
agent-level-execute = Ejecutar
agent-level-plan = Planificar
agent-level-admin = Administrar
error-parse-agent-level = Nivel inválido `{ $value }`; valores: { $expected }
agent-paused = El supervisor pausó a { $agent }: puede leer, pero no escribir hasta que lo reanude
workspace-read-only = El workspace está en solo lectura para los agentes; el supervisor lo reactivará
agent-level = { $agent } tiene nivel { $level } y { $command } necesita { $needed }
agent-out-of-scope = { $agent } no puede escribir en el proyecto { $project } (fuera de su alcance)
wip-full = La columna { $column } ya tiene { $limit } tareas (su límite)
notfound-activity = No existe la entrada de historial `{ $entry }`
revert-conflict = No se puede revertir { $entry }: { $fields } cambió después
revert-unsupported = No se puede revertir una acción { $action } sobre { $entity }
revert-gone = Lo que cambió esa entrada ({ $entity }) ya no existe
idempotency-key-reused = La clave de idempotencia `{ $key }` ya se usó con { $command }
cmd-agent-pause = Pausar a un agente: puede leer pero no escribir (solo supervisor)
cmd-agent-resume = Reanudar a un agente pausado (solo supervisor)
cmd-activity-revert = Revertir una entrada del historial, o todo su comando con whole_command=true (solo supervisor)
cmd-activity-summary = Qué pasó desde un instante: terminadas, en revisión, escaladas, creadas, hallazgos y leases vencidos
cmd-system-housekeeping = Limpieza periódica del sistema (claves de idempotencia antiguas)
param-level = Nivel de permisos: read, execute, plan o admin
param-scope = Proyectos en los que puede escribir, separados por comas (vacío = todos)
param-capabilities = Capacidades del agente (etiquetas) separadas por comas
param-read_only = Solo lectura para todos los agentes (true/false)
param-wip = Límite de tareas de la columna (0 lo quita)
param-wip_reject = Con el límite lleno, rechazar la entrada (true) o solo avisar (false)
param-entry = ID de la entrada del historial
param-whole_command = Revertir todo lo que hizo el mismo comando
param-since = Desde cuándo (fecha y hora, por ejemplo 2026-10-26T08:00:00Z)
param-idempotency_key = Clave para que un reintento no duplique lo creado
field-level = nivel
field-scope = alcance
field-capabilities = capacidades
field-paused = pausado
ui-agent-capabilities = Capacidades
ui-agent-level = Nivel de permisos
ui-agent-scope = Alcance
ui-agent-scope-all = (todos los proyectos)
ui-agent-token-hint = Configúralo en el cliente MCP como TASKCRAFT_AGENT_TOKEN (stdio) o Authorization: Bearer (HTTP). Lo comparten todos los agentes de la interfaz; puedes volver a copiarlo con «Copiar token».
ui-copy = Copiar
ui-new-token = Nuevo token
ui-notice-question = { $task }: { $agent } pregunta: { $question }
ui-notice-review = { $task } está lista para revisión ({ $agent })
ui-notice-review-plain = { $task } está lista para revisión
ui-notice-question-plain = { $task } pregunta: { $question }
ui-os-notify = Notificaciones del sistema
ui-pause = Pausar
ui-paused = Pausado
ui-read-only = Solo lectura para agentes
ui-read-only-on = Agentes en solo lectura
ui-resume = Reanudar
ui-revert-command = Revertir todo lo que hizo ese comando
ui-revert-entry = Revertir este cambio
ui-summary = Desde tu última visita
ui-summary-counts = { $created } tareas creadas, { $comments } comentarios, { $findings } hallazgos, { $expired } leases vencidos
ui-summary-done = Terminadas
ui-summary-escalated = Preguntas pendientes
ui-summary-review = En revisión
ui-summary-since = Lo que pasó desde tu última visita ({ $ago }):
ui-wip-reject = rechazar

# M7: tiempo y métricas
time-source-timer = Cronómetro
time-source-manual = Manual
error-parse-time-source = Origen de tiempo inválido `{ $value }`; valores: { $expected }
timer-not-running = No tienes ningún cronómetro en marcha
cmd-timer-start = Poner en marcha el cronómetro en una tarea (para el que hubiera)
cmd-timer-stop = Parar tu cronómetro y guardar el tiempo
cmd-time-log = Anotar tiempo a mano: task, minutes, date, note
cmd-time-report = Tiempo por actor y por tarea (automático del historial y anotado): project, task, actor, from, to
cmd-time-sheet = Hoja de horas por día, semana o mes: period, project, actor, from, to
cmd-metrics-burndown = Burndown de un proyecto o una épica: pendientes y hechas al final de cada día
cmd-metrics-cfd = Flujo acumulado de un proyecto: tareas por estado al final de cada día
cmd-metrics-agents = Rendimiento por agente: entregas, aprobadas, rechazadas, ciclo medio y minutos
param-minutes = Minutos (1–1440)
param-date = Fecha
param-note = Nota
param-owner = Agente dueño de la columna (su cola o su mesa de trabajo); vacío o none lo quita
param-from = Desde (fecha)
param-to = Hasta (fecha)
param-period = Agrupar por day, week o month
param-days = Antigüedad mínima en días, o días hacia atrás en métricas
field-minutes = minutos
field-running = en marcha
field-note = nota
field-source = origen
field-started_at = inicio
ui-agent-performance = Rendimiento por agente
ui-burndown = Burndown de { $project }
ui-cfd = Flujo acumulado
ui-col-agent = Agente
ui-col-task = Tarea
ui-loading = Calculando…
ui-metrics-charts = Gráficos
ui-metrics-days = Últimos { $n } días
ui-metrics-hours = Hojas de horas
ui-metrics-pick-project = Elige un proyecto en la barra lateral para ver su burndown y su flujo acumulado.
ui-no-time = No hay tiempo registrado en este periodo
ui-perf-approved = Aprobadas
ui-perf-cycle = Ciclo medio
ui-perf-rejected = Rechazadas
ui-perf-rejection-rate = % rechazo
ui-perf-submitted = Entregas
ui-perf-time = Tiempo
ui-period = Periodo
ui-period-day = Por día
ui-period-month = Por mes
ui-period-week = Por semana
ui-refresh = Refrescar
ui-remaining = Pendientes
ui-time = Tiempo
ui-time-log = Anotar
ui-time-logged = Anotado
ui-timer-elsewhere = Tu cronómetro corre en { $task }
ui-timer-running = Cronómetro en marcha
ui-timer-start = Iniciar cronómetro
ui-timer-stop = Parar cronómetro
ui-total = Total
ui-view-metrics = Métricas

## M8 · Vistas (RF-VIS-05/08/10/12/14, D12)
cmd-view-save = Guardar una vista con nombre: name, filter (lenguaje D12), layout, columns, project
cmd-view-list = Vistas guardadas
cmd-view-get = Una vista guardada y sus tareas
cmd-view-delete = Borrar una vista guardada
cmd-today-mark = Marcar una tarea «para hoy»
cmd-today-unmark = Quitar la marca «para hoy»
cmd-today-list = Hoy: atrasadas, las que vencen hoy y las marcadas
param-filter = Filtro: status:todo priority>=high -label:docs due<=today+3d (Y por defecto, OR, NOT, paréntesis)
param-view = Vista guardada: nombre o ID
param-layout = Forma de mostrarla: list, board, table, today o gantt
param-columns = Columnas de la tabla, separadas por comas
notfound-view = No existe la vista `{ $view }`
view-name-empty = La vista necesita un nombre
rule-filter = Filtro mal escrito en la posición { $position }: se esperaba { $expected }
ui-col-epic = Épica
ui-col-estimate = Estimación
ui-col-milestone = Hito
ui-col-updated = Cambiada
ui-columns = Columnas
ui-filter-help = Texto libre o filtro: status:todo priority>=high -label:docs due<=today+3d (OR, NOT y paréntesis)
ui-gantt-empty = Ninguna tarea tiene fechas de inicio o vencimiento
ui-gantt-milestones = Hitos
ui-gantt-undated = { $count } tareas sin fechas no aparecen en la línea de tiempo
ui-shortcut-activity = Mostrar u ocultar la actividad
ui-shortcut-clear = Quitar el atajo
ui-shortcut-conflict = Hay atajos repetidos: solo funcionará uno de ellos
ui-shortcut-mark-today = Marcar o desmarcar «para hoy» la tarea seleccionada
ui-shortcut-press = Pulsa una combinación… (Esc cancela)
ui-shortcut-reset = Restaurar los predeterminados
ui-shortcut-save-view = Guardar la vista
ui-shortcut-search = Ir al buscador
ui-today-due = Vencen hoy
ui-today-empty = Nada para hoy: ni atrasadas, ni vencimientos, ni tareas marcadas
ui-today-mark = Para hoy
ui-today-marked = Elegidas para hoy
ui-today-overdue = Atrasadas
ui-today-unmark = Quitar de hoy
ui-view-delete = Borrar esta vista guardada
ui-view-gantt = Gantt
ui-view-name = Nombre de la vista
ui-view-save = Guardar vista
ui-view-table = Tabla
ui-view-today = Hoy
ui-views-empty = Aún no hay vistas guardadas
ui-views-none = Vistas guardadas
ui-zoom = Zoom

## M9 · Integraciones y datos (RF-INT-02/03, RF-ALM-06/08, RF-EXP-03b/06/07)
cmd-workspace-backup = Copia de seguridad .taskcraft en la carpeta del workspace (o dir), conservando las últimas backup_keep
cmd-workspace-export_markdown = Exportar a Markdown: un .md por tarea con frontmatter YAML, en carpetas por proyecto: dir, projects
cmd-task-export_csv = Exportar a CSV las tareas de una vista o un filtro: path, view, filter, columns
cmd-automation-create = Crear una automatización (solo supervisor): name, trigger, condition (filtro), command, args (JSON con {"{"}task{"}"}, {"{"}parent{"}"}, {"{"}project{"}"}), o preset=parent_review
cmd-automation-list = Automatizaciones
cmd-automation-update = Activar, desactivar o cambiar la condición de una automatización
cmd-automation-delete = Borrar una automatización
cmd-task-review_parent = Si todas las subtareas están cerradas, pasar el padre a revisión
cmd-webhook-create = Crear un webhook (solo supervisor): url y triggers; devuelve el secreto de firma una sola vez
cmd-webhook-list = Webhooks (sin secretos)
cmd-webhook-delete = Borrar un webhook y su cola
cmd-webhook-deliveries = Últimos envíos de los webhooks y su estado
cmd-webhook-record = Anotar el resultado de un envío (lo usa el daemon)
param-dir = Carpeta de destino
param-auto = Solo si toca según backup_hours (lo usa el daemon)
param-backup_hours = Horas entre copias automáticas (0 = desactivadas)
param-backup_keep = Copias que se conservan
param-trigger = Evento: created, status_changed, completed, assigned, commented o escalated
param-triggers = Eventos que se envían, separados por comas (vacío = todos)
param-condition = Filtro (D12) que debe cumplir la tarea
param-command = Comando que se ejecuta (ID del registro)
param-args = Argumentos en JSON; {"{"}task{"}"}, {"{"}parent{"}"} y {"{"}project{"}"} se sustituyen
param-preset = Regla ya hecha: parent_review (cuando se cierran todas las hijas, el padre pasa a revisión)
param-automation = Automatización: nombre o ID
param-enabled = Activada (true/false)
param-url = URL de destino (http o https)
param-webhook = Webhook: ID o URL
param-delivery = ID del envío
param-ok = Si el envío salió bien
param-error = Motivo del fallo
arg-json = JSON inválido: { $detail }
automation-command = No existe el comando `{ $command }`
webhook-url = La URL debe empezar por http:// o https://
notfound-automation = No existe la automatización `{ $automation }`
notfound-webhook = No existe el webhook `{ $webhook }`
notfound-delivery = No existe el envío `{ $delivery }`
error-parse-trigger = Evento desconocido `{ $value }`; válidos: { $expected }
error-parse-delivery-status = Estado de envío desconocido `{ $value }`; válidos: { $expected }
cli-workspace-help = Workspace por nombre (en el directorio de datos); --db manda sobre él
cli-workspaces-help = Lista los workspaces (* = el actual)
ui-backup-now = Copia de seguridad ahora
ui-import-merge = Fusionar (por ULID)
ui-import-updates = Se actualizan { $projects } proyectos y { $tasks } tareas
ui-workspaces = Workspaces…
ui-workspace-new = Abrir o crear otro workspace
ui-workspace-open = Abrir
ui-workspace-hint = Se abre en una ventana nueva; cada workspace tiene su propio daemon y su archivo.
cli-migrate-help = Copia un workspace a otro backend (ruta SQLite o postgres://…); el destino debe estar vacío
cli-migrate-from-help = Origen: ruta de workspace.db o cadena postgres://…
cli-migrate-to-help = Destino vacío: ruta de workspace.db o cadena postgres://…
cli-migrate-busy = El destino { $db } tiene un daemon abierto: deténlo antes de migrar
cli-migrate-done = Migradas { $tasks } tareas a { $to }

## M10 · App web (D14)
cmd-workspace-web_token = Emitir el token web del supervisor (se muestra una vez; revoke=true lo anula)
param-revoke = Anular el token en lugar de emitir uno nuevo
web-token-home = Este workspace no tiene carpeta donde guardar el token web
cli-serve-web-help = Sirve la app web desde esta carpeta (cargo xtask web la deja en dist/web)
cli-serve-tls-cert-help = Certificado TLS en PEM (obligatorio fuera de 127.0.0.1)
cli-serve-tls-key-help = Clave privada TLS en PEM
web-tasks = Tareas
web-activity = Actividad
web-logout = Salir
web-login = Entrar
web-login-hint = Pega el token web del supervisor (taskcraft-cli workspace web-token) o el de un agente
web-remember = Recordar en este navegador
web-review-empty = No hay nada esperando revisión ni preguntas pendientes
web-question = Pregunta pendiente para el supervisor
web-answer = Responder
web-approve = Aprobar
web-reject = Rechazar
web-reject-reason = Motivo del rechazo
web-apply = Aplicar
web-pause = Pausar
web-resume = Reanudar
web-activity-line = { $at } · { $actor } · { $action } · { $target }

## M11 · Interfaces y relevo (D15)
cmd-interface-register = Registrar una interfaz (cliente LLM: Claude Code, Cursor…) con su token (solo supervisor); después se le añaden agentes: name, kind, description, color, level, scope, max_agents
cmd-interface-list = Interfaces y sus agentes
cmd-interface-get = Una interfaz y sus agentes
cmd-interface-update = Cambiar nombre, color, tipo, descripción, nivel máximo, alcance o máximo de agentes de una interfaz
cmd-interface-pause = Pausar una interfaz: sus agentes leen pero no escriben
cmd-interface-resume = Reanudar una interfaz pausada
cmd-interface-disable = Desactivar una interfaz (su token deja de valer)
cmd-interface-enable = Reactivar una interfaz
cmd-interface-revoke = Revocar una interfaz para siempre
cmd-interface-rotate_token = Emitir un token nuevo para una interfaz e invalidar el anterior
cmd-agent-create = Crear un agente de tu interfaz (sin token): name, model, description (su función: qué hace y cuándo pasarle trabajo), capabilities, level
cmd-agent-mine = Quién eres y qué agentes de tu interfaz puedes usar
cmd-task-add_assignee = Añadir un agente a una tarea; puede tener tantos como haga falta: task, agent
cmd-task-remove_assignee = Quitar un agente de una tarea: task, agent
param-interface = Interfaz: nombre o ID
param-max_agents = Máximo de agentes de la interfaz
param-comment = Comentario (obligatorio al rechazar)
param-as_agent = Actuar como este agente de tu interfaz solo en esta llamada
notfound-interface = No existe la interfaz `{ $interface }`
interface-name-taken = Ya existe una interfaz llamada { $name }
interface-needs-agent = La interfaz { $interface } solo puede leer y crear agentes: elige uno con agent_use (o as_agent) para escribir
interface-only = Solo una interfaz (o uno de sus agentes) puede hacer esto
interface-max-agents = La interfaz { $interface } ya tiene { $max } agentes
agent-other-interface = { $agent } no es un agente de tu interfaz
agent-no-interface = { $agent } no pertenece a ninguna interfaz
task-other-assignees = { $task } es de { $agents }: busca otra con task_next
ui-interface-register = Nueva interfaz
ui-interface-kind = Tipo de cliente
ui-interface-level = Nivel máximo de sus agentes
ui-interface-hint = La interfaz es el cliente LLM (Claude Code, Cursor…): tiene el único token. Sus agentes se añaden aquí o los crea ella por MCP con agent_create.
ui-agent-add = Añadir agente
ui-no-interface = Sin interfaz
ui-assignees = Asignados
ui-pick-agent = Elegir agente
ui-agent-function = Función
ui-agent-function-hint = Qué hace este agente y cuándo pasarle trabajo (p. ej. «Revisa el código y los tests»)
ui-assign-agents = Asignar agentes…
ui-column-no-owner = Sin dueño
ui-column-owner-hint = Agente dueño de la columna: lo que entra en ella pasa a estar asignado a él
ui-add-column = Columna
ui-add-column-hint = Añadir una columna al flujo del proyecto, también la cola o la mesa de trabajo de un agente

# Extras M12: coste por agente (RF-HIS-06)
cmd-cost-report = Reportar los tokens y el coste de tu trabajo en una tarea: task, tokens_in, tokens_out, amount, currency, model, note
cmd-cost-list = Costes reportados (una tarea incluye sus subtareas): task, epic, project, actor, from, to
cmd-cost-summary = Coste total y agrupado por agente, modelo, tarea o día: task, epic, project, actor, from, to, group
cost-empty = El reporte no tiene tokens ni importe: indica tokens_in, tokens_out o amount
param-tokens_in = Tokens de entrada consumidos
param-tokens_out = Tokens de salida generados
param-amount = Importe, con hasta 6 decimales (0.42)
param-currency = Moneda: código de tres letras (USD por defecto)
param-group = Agrupar por agent, model, task o day
ui-cost = Coste
ui-cost-tokens = { $tokens } tokens
ui-cost-hint = Lo reportan los agentes con cost.report; incluye las subtareas.
ui-cost-by-agent = Coste por agente
ui-cost-empty = Ningún agente ha reportado coste en el periodo (cost.report).
ui-cost-reports = Reportes

# Extras M12: integración con git (RF-INT-04, D16)
cmd-git-link = Vincular una tarea a una rama, un commit o un PR: task, kind (branch, commit o pr), ref, title, url
cmd-git-unlink = Quitar un vínculo con git: task, kind, ref (un commit acepta su SHA corto)
cmd-git-links = Vínculos con git de una tarea, de un proyecto o de todo: task, project, kind
cmd-git-ingest = Registrar un commit: vincula las tareas que menciona y pasa a revisión las que cierra (fixes API-42): sha, message, branch, remote
error-parse-git-kind = Tipo de vínculo inválido `{ $value }`; valores: { $expected }
git-kind-branch = Rama
git-kind-commit = Commit
git-kind-pr = PR
git-bad-sha = SHA inválido `{ $value }`: hacen falta los 40 (o 64) caracteres hexadecimales del commit
param-ref = Referencia: nombre de la rama, SHA del commit o URL del PR
param-sha = SHA completo del commit
param-message = Mensaje completo del commit
param-branch = Rama en la que se hizo el commit
param-remote = URL del remoto (origin), para enlazar el commit en la web
cli-git-repo-help = Carpeta del repositorio git (por defecto, la actual)
cli-git-hook-help = Hook de git que vincula cada commit con sus tareas
cli-git-hook-install-help = Instalar el hook post-commit en el repositorio
cli-git-hook-uninstall-help = Quitar el hook post-commit de TaskCraft
cli-git-scan-help = Leer commits del repositorio y vincularlos con sus tareas (fixes API-42 la pasa a revisión)
cli-git-since-help = Commits desde esta revisión hasta HEAD
cli-git-last-help = Los últimos N commits (20 por defecto)
cli-git-quiet-help = Sin resumen (para el hook)
cli-git-not-repo = { $repo } no es un repositorio git: { $detail }
cli-git-hook-installed = Hook instalado en { $path }
cli-git-hook-removed = Hook de TaskCraft quitado
cli-git-hook-missing = No hay un hook de TaskCraft que quitar
cli-git-hook-exists = Ya hay otro hook en { $path }; añade a mano la línea `taskcraft-cli git scan --last 1 --quiet`
cli-git-hook-db-env = El workspace es una cadena de conexión: el hook la tomará de TASKCRAFT_DB, que no se guarda en el repo
cli-git-scanned = { $commits } commits leídos. Vinculadas: { $linked }. A revisión: { $moved }
ui-git = Git
ui-git-hint = Los commits que mencionan la tarea se vinculan solos con `taskcraft-cli git hook install`.

# Extras M13: tareas recurrentes (RF-MOD-18, D17)
cmd-task-set_recurrence = Hacer que una tarea se repita al terminarla: task, every (day, week, month, year o 2w), interval, on (mon,thu), day (del mes), o after (3d tras terminarla)
cmd-task-clear_recurrence = Quitar la repetición de una tarea: task
rule-recurrence = Regla de repetición inválida `{ $value }`: usa every=week on=mon,thu, every=month day=1, every=2d o after=3d
error-parse-repeat-unit = Unidad inválida `{ $value }`; valores: { $expected }
repeat-unit-day = día
repeat-unit-week = semana
repeat-unit-month = mes
repeat-unit-year = año
recurrence-every-day = { $n ->
    [one] Cada día
   *[other] Cada { $n } días
}
recurrence-every-week = { $n ->
    [one] Cada semana
   *[other] Cada { $n } semanas
}
recurrence-every-month = { $n ->
    [one] Cada mes
   *[other] Cada { $n } meses
}
recurrence-every-year = { $n ->
    [one] Cada año
   *[other] Cada { $n } años
}
recurrence-on-days = { $base } ({ $days })
recurrence-on-day = { $base }, el día { $day }
recurrence-after = { $n ->
    [one] 1 día después de terminarla
   *[other] { $n } días después de terminarla
}
weekday-1 = lun
weekday-2 = mar
weekday-3 = mié
weekday-4 = jue
weekday-5 = vie
weekday-6 = sáb
weekday-7 = dom
param-every = Cada cuánto: day, week, month, year, o con número (2w = cada 2 semanas)
param-interval = Cada cuántas unidades (1 por defecto)
param-on = Días de la semana: mon,thu (o lun,jue)
param-day = Día del mes (1–31; en meses más cortos, el último)
param-after = Días tras terminarla: 3d, 2w
ui-recurrence = Se repite
ui-recurrence-none = No se repite
ui-recurrence-every = Cada…
ui-recurrence-after = Tras terminarla
ui-recurrence-month-day = Día del mes
ui-recurrence-month-day-hint = 0 = el mismo día que el vencimiento; 31 = el último del mes.
ui-recurrence-days-after = días después de terminarla
ui-recurrence-hint = Al pasarla a hecho se crea la siguiente con la fecha que toca.

# Extras M13: tareas envejecidas (RF-VIS-16)
cmd-task-stale = Tareas pendientes sin cambios desde hace semanas: project, weeks (por defecto, las del workspace)
cmd-task-someday = Pasar una tarea a la etapa «Algún día» del Backlog (se crea si falta): task
param-weeks = Semanas
param-stale_weeks = Semanas sin cambios tras las que una tarea pendiente está envejecida (1–520; 4 por defecto)
param-urgent_days = Días hasta el vencimiento en los que una tarea es urgente en la matriz (1–365; 3 por defecto)
stage-someday = Algún día
ui-stale-hint = Sin cambios desde hace { $weeks } semanas: ¿pasarla a «Algún día»? (menú contextual)
ui-someday-move = Pasar a «Algún día»

# Extras M13: grafo, ruta crítica y matriz (RF-REL-05, RF-REL-06, RF-VIS-15)
cmd-project-graph = Grafo de dependencias de un proyecto: nodos con su capa y aristas, con la ruta crítica marcada; format=dot para Graphviz: project, closed, format
cmd-project-critical_path = Ruta crítica: la cadena de dependencias abiertas más pesada (por estimación, días o 1 por tarea), que fija cuándo puede terminar el proyecto: project
cmd-task-matrix = Matriz de Eisenhower: tareas abiertas por cuadrante (importante = prioridad alta o urgente; urgente = vence pronto): project
param-closed = Incluir también las tareas cerradas
param-format = Formato: json o dot
quadrant-do = Hacer ya
quadrant-schedule = Planificar
quadrant-delegate = Delegar
quadrant-drop = Descartar
ui-view-matrix = Matriz
ui-view-graph = Grafo
ui-matrix-hint = Importante = prioridad alta o urgente. Urgente = vence en { $days } días o menos. Arrastra entre filas para cambiar la prioridad.
ui-graph-closed = Mostrar cerradas
ui-graph-loose = Sin dependencias
ui-graph-empty = No hay dependencias entre estas tareas. Se crean con «Bloqueada por» en el detalle o con task.link.
ui-graph-critical = Ruta crítica (en rojo): { $n } tareas, peso { $total } (estimación, días o 1 por tarea).

# Extras M13: ciclos o sprints (RF-INT-05)
cmd-cycle-create = Crear un ciclo (sprint) en un proyecto; por defecto empieza tras el último, dura lo mismo (14 días el primero) y se numera solo: project, name, start, end, days
cmd-cycle-list = Ciclos de un proyecto con su estado y progreso: project, closed
cmd-cycle-get = Un ciclo con sus tareas; acepta su nombre, current o next: cycle, project
cmd-cycle-update = Cambiar nombre o fechas de un ciclo: cycle, project, name, start, end
cmd-cycle-close = Cerrar un ciclo: lo que quedó abierto y no está en otro ciclo abierto pasa también al siguiente (se crea si falta): cycle, project
cmd-cycle-delete = Borrar un ciclo; sus tareas conservan los demás ciclos: cycle, project
cmd-cycle-rollover = Cerrar los ciclos vencidos y traspasar lo abierto (lo hace el daemon cada día)
cmd-task-set_cycle = Dejar una tarea en un solo ciclo, en lugar de los que tenía (nombre, current, next o none): task, cycle
cmd-task-add_cycle = Planificar una tarea también en otro ciclo abierto, sin quitarla de los que tiene: task, cycle
cmd-task-remove_cycle = Sacar una tarea de un ciclo (abierto o cerrado): task, cycle
notfound-cycle = No existe el ciclo `{ $cycle }`
cycle-bad-dates = Fechas de ciclo inválidas: { $start } a { $end } (el fin no puede ser anterior al inicio ni durar más de un año)
cycle-overlap = El ciclo se solapa con { $other }, que sigue abierto
cycle-name-taken = Ya hay un ciclo llamado { $name } en el proyecto
cycle-closed = El ciclo { $cycle } ya está cerrado
cycle-other-project = El ciclo es de otro proyecto
param-cycle = Ciclo: nombre, current (en curso), next (siguiente) o none
param-end = Último día (incluido)
ui-cycle = Ciclo
ui-cycle-new = Nuevo ciclo
ui-cycles-empty = Sin ciclos abiertos. «Nuevo ciclo» propone el siguiente, con la misma duración que el anterior (14 días si es el primero).
ui-cycle-tasks = Ver tareas
ui-cycle-close = Cerrar ciclo
ui-cycle-close-hint = Lo que quede abierto pasa al ciclo siguiente (se crea si no existe). El daemon cierra solos los vencidos.
ui-cycle-planned = Planificado
ui-cycle-active = En curso
ui-cycle-overdue = Vencido
ui-cycle-closed = Cerrado
ui-view-cycles = Ciclos / sprints
ui-cycles-hint = Un ciclo (o sprint) es un periodo con inicio y fin. Al cerrarlo, lo que quede abierto pasa al siguiente.
ui-cycle-edit = Editar ciclo
ui-cycle-delete = Borrar ciclo
ui-cycle-delete-confirm = ¿Borrar «{ $cycle }»? Sus tareas no se borran: se quedan sin ciclo.
ui-cycle-name = Nombre
ui-cycle-start = Inicio
ui-cycle-end = Fin (incluido)
ui-cycle-days = { $days ->
    [one] 1 día
   *[other] { $days } días
}
ui-cycles-closed = Cerrados ({ $count })
ui-any-cycle = Cualquier ciclo
ui-project = Proyecto
ui-sidebar-tasks = Tareas
ui-sidebar-views = Vistas
ui-sidebar-epics = Épicas
ui-sidebar-milestones = Hitos
ui-sidebar-no-cycles = Sin ciclos abiertos
ui-cycles = Sprints
ui-cycles-plan = Planificar en sprints…
ui-saved-views = Vistas guardadas
ui-archived-count = Archivados ({ $count })
ui-epic-new = Nueva épica o hito…

# Extras M15: diagramas Mermaid (RF-MOD-19, D21)
cmd-diagram-create = Crear un diagrama Mermaid en un proyecto; recibe la clave API-D3 y se cita en descripciones y comentarios con [[API-D3]] (enlace) o ![[API-D3]] sola en una línea (incrustado). Si la fuente no se puede dibujar se guarda igual y devuelve render_error: project, title, source, description
cmd-diagram-list = Diagramas de un proyecto (o de todos), sin la fuente: project
cmd-diagram-get = Un diagrama con su fuente, render_error y las tareas que lo citan; svg=true añade el SVG dibujado (theme light o dark): diagram, project, svg, theme
cmd-diagram-update = Cambiar título, fuente o descripción de un diagrama: diagram, project, title, source, description
cmd-diagram-delete = Borrar un diagrama; su número no se reutiliza y las citas que queden dejan de apuntar a nada: diagram, project
param-diagram = Diagrama: clave (API-D3) o ID; con project, también su número (D3)
param-source = Fuente Mermaid (flowchart, sequenceDiagram, classDiagram, stateDiagram, erDiagram, gantt, mindmap…)
param-svg = true para incluir el SVG dibujado
param-theme = Tema del SVG: light o dark
notfound-diagram = No existe el diagrama `{ $diagram }`
arg-bad-diagram-ref = `{ $value }` no es una clave de diagrama (API-D3) ni un ID
arg-too-large = Demasiado grande: como mucho { $max } bytes
diagram-error-empty = El diagrama está vacío
diagram-error-too-large = La fuente pasa de { $max } bytes
diagram-error-syntax = No se puede dibujar: { $detail }
diagram-error-render = Error al dibujar: { $detail }
ui-view-diagrams = Diagramas
ui-diagrams-hint = Diagramas Mermaid del proyecto. Una tarea los cita con [[API-D3]] (enlace) o los incrusta con ![[API-D3]] sola en una línea de su descripción o de un comentario.
ui-diagrams-empty = Sin diagramas
ui-diagrams-none = Elige un diagrama o crea uno nuevo.
ui-diagram-new = Nuevo diagrama
ui-diagram-new-in = Nuevo diagrama en { $project }
ui-diagram-edit = Editar diagrama
ui-diagram-delete = Borrar diagrama
ui-diagram-delete-confirm = ¿Borrar { $diagram }? Su número no se reutiliza y las citas que queden no mostrarán nada.
ui-diagram-copy = Copiar cita
ui-diagram-copy-hint = Copia ![[clave]] para pegarlo en la descripción de una tarea
ui-diagram-title = Título
ui-diagram-description = Descripción (Markdown)
ui-diagram-source = Fuente Mermaid
ui-diagram-preview = Vista previa
ui-diagram-cited-by = Citado en ({ $count })
ui-diagram-not-cited = Ninguna tarea lo cita todavía. Escribe [[{ $key }]] o ![[{ $key }]] en una descripción.
ui-diagram-embed = Incrustar diagrama…
ui-diagram-cite-hint = [[API-D3]] enlaza un diagrama; ![[API-D3]] sola en una línea lo muestra dibujado.

# D22: interfaces primero y token copiable (RF-ACT-07b)
rule-interface-name = Nombre de interfaz inválido `{ $name }`: 1–60 caracteres; letras, números, espacios, punto, guion o guion bajo, empezando por letra o número
cmd-interface-token = El token guardado de una interfaz, para copiarlo (solo supervisor)
interface-token-unknown = La interfaz `{ $interface }` es anterior a que se guardaran los tokens: genera uno nuevo para poder copiarlo
ui-interfaces-first = Primero registra una interfaz: el cliente LLM (Claude Code, Cursor…) que se conecta con un token. Después añade dentro sus agentes, cada uno con su función.
ui-interface-edit = Editar interfaz
ui-interface-name = Nombre
ui-interface-name-rule = 1–60 caracteres: letras, números, espacios, punto, guion o guion bajo
ui-interface-agents = { $n }/{ $max } agentes
ui-interface-no-agents = Sin agentes todavía: «Añadir agente».
ui-interface-full = La interfaz ya tiene su máximo de agentes
ui-interface-token = Token de la interfaz
ui-interface-token-of = Token de { $interface }:
ui-copy-token = Copiar token
ui-copy-token-hint = Copia el token de la interfaz al portapapeles
ui-token-copied = Token de { $interface } copiado
ui-confirm-new-token = ¿Generar un token nuevo para { $interface }? El actual deja de valer para todos sus agentes.
ui-confirm-interface-revoke = ¿Revocar { $interface }? Su token deja de valer y sus agentes dejan de trabajar. No se puede deshacer.
ui-no-interface-hint = Agentes de antes de las interfaces: usa Editar para moverlos a una.
ui-agent-name-rule = 1–40 caracteres a–z, 0–9, punto, guion o guion bajo
ui-agent-no-token-hint = El agente no tiene token: se conecta con el de su interfaz.

# Barra de menús por tema (RF-VIS-17)
ui-menu-agents = Agentes
ui-menu-preferences = Preferencias
ui-menu-pick-project = Elige un proyecto en la barra lateral
ui-menu-search = Buscar
ui-menu-interfaces-agents = Interfaces y agentes
ui-about = Acerca de TaskCraft
ui-about-text = Planificador de tareas para varios agentes de IA bajo supervisión humana. Todo es local: la app, la CLI y el servidor MCP trabajan sobre el mismo workspace.
ui-agent-interface = Interfaz
ui-agent-model = Modelo

# D23: gestión de la información (RF-ALM-09)
cmd-project-delete = Eliminar definitivamente un proyecto con todas sus tareas, comentarios, ciclos, épicas, diagramas, adjuntos e historial (solo supervisor). Antes guarda una copia .taskcraft: project, confirm (la clave del proyecto), dir
cmd-workspace-wipe = Vaciar el workspace: proyectos, tareas, etiquetas, agentes, interfaces, plantillas e historial; queda solo su nombre y configuración (solo supervisor). Antes guarda una copia .taskcraft: confirm (el nombre del workspace), dir
cmd-activity-prune = Borrar el historial anterior a una fecha (solo supervisor). Antes guarda una copia .taskcraft: before, dir
param-confirm = Confirmación: repite la clave del proyecto o el nombre del workspace
param-backup = true para guardar antes una copia .taskcraft
data-confirm-mismatch = La confirmación no coincide: escribe `{ $expected }`
data-prune-future = `{ $before }` está en el futuro: solo se puede borrar historial pasado
ui-menu-data = Información
ui-delete-data = Eliminar información
ui-delete-trash = Vaciar la papelera
ui-delete-history = Historial antiguo
ui-delete-project = Un proyecto completo
ui-delete-workspace = Todo el workspace
ui-delete-trash-hint = Borra definitivamente las tareas que están en la papelera, con sus comentarios y adjuntos.
ui-delete-history-hint = Borra las entradas del historial anteriores a la fecha elegida. El resto de la información no cambia.
ui-delete-project-hint = Borra definitivamente el proyecto con todas sus tareas, comentarios, ciclos, épicas, diagramas, adjuntos e historial.
ui-delete-workspace-hint = Deja el workspace vacío: proyectos, tareas, etiquetas, agentes, interfaces, plantillas e historial. Solo se conservan su nombre y su configuración.
ui-delete-trash-count = { $count } tareas en la papelera
ui-delete-history-before = Borrar lo anterior a
ui-delete-pick-project = Elige el proyecto
ui-delete-type-to-confirm = Para confirmar, escribe «{ $text }»:
ui-delete-backup-first = Antes de eliminar se guarda siempre una copia .taskcraft en la carpeta de copias; se restaura con Importar.
ui-delete-done = Eliminado. Copia previa en { $path }
