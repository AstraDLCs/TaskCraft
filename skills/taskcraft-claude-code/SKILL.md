---
name: taskcraft-claude-code
description: Work on TaskCraft from Claude Code through its MCP server, as three agents of the "Claude code" interface — cc-planner turns specs into tasks, epics, sprints and Mermaid diagrams; cc-dev takes the next ready task, implements it in this repository and hands it to review; cc-revisor reviews code and UI quality (optimal, safe, simple, readable, no duplication) and closes or returns it. Use when asked to plan, organise or break down work in TaskCraft, to create TaskCraft tasks, sprints or diagrams, to pick up, continue, work on or finish TaskCraft tasks, or to review delivered work (e.g. "toma la siguiente tarea", "planifica esto en TaskCraft", "revisa TC-2", "TC-12").
---

# TaskCraft — Claude Code (planner + developer + reviewer)

Built from skills/skillbuild.md (reviewed 2026-10-27). If a fact here disagrees with the server's
answers, trust the server and report the mismatch to the supervisor with `inbox_add`.

## Connection

- MCP server `taskcraft` (stdio: `taskcraft-cli mcp --stdio`, token in `TASKCRAFT_AGENT_TOKEN`).
  In Claude Code the tools are named `mcp__taskcraft__<tool>`, e.g. `mcp__taskcraft__task_next`.
  Below they are written without the prefix.
- If the tools are missing, stop and tell the user how to connect (skills/skillbuild.md §3.2):
  `claude mcp add taskcraft -e TASKCRAFT_AGENT_TOKEN=… -- taskcraft-cli mcp --stdio`, then a new
  session. **Never ask for, print, store or commit the token.**
- The server's own `initialize` instructions describe the work loop; this skill only adds detail.

## Identity

The token belongs to the interface **"Claude code"** (level admin, all projects, up to 5
agents). Agents have no token. Pick the agent before any write:

| Agent | Use it when the user asks to… | Function (its `description`) |
|---|---|---|
| `cc-planner` | plan, specify, break down, organise, create tasks/epics/sprints/diagrams | Planifica: convierte especificaciones en tareas de TaskCraft con checklist, criterios de aceptación, dependencias, épicas, sprints y diagramas Mermaid. No implementa código. |
| `cc-dev` | work on, pick up, continue, implement or finish tasks | Implementa en el repositorio agents-planner las tareas listas de TaskCraft: código Rust, tests y documentación, con `cargo xtask ci` en verde antes de cerrarlas. No planifica. |
| `cc-revisor` | review, check quality, audit delivered work | Reviews code and UI quality before work counts as done (its full Spanish function is stored in TaskCraft): optimal and why, safe, simpler or more readable, no duplicated code, UI that fits every window size. Does not implement or plan. Created by cc-planner (agents need level plan to create agents). |

1. `agent_use` with no arguments → lists the interface's agents and the active one.
2. If the agent you need is missing: `agent_create {name, description: <function above>, level}`
   with `level: "plan"` for cc-planner and `level: "execute"` for cc-dev and cc-revisor
   (create them as cc-planner: `as_agent: "cc-planner"`). Then
   `agent_use {agent: <name>}`.
3. Switch with `agent_use {agent}` when the request changes role. For a single call as the other
   agent, pass `as_agent`.
4. Stay inside the function: a planner never implements, a developer never re-plans. When work
   falls outside it, hand it to the other agent (see Hand-over).

## Loop — cc-planner

1. Read the project: resource `taskcraft://project/TC` (or `project_get {project}`) for its
   workflow, fields and estimate unit; `task_list {project, fields: "key,title,status,parent"}`
   to avoid duplicates. The default project is **TC · TaskCraft**; ask if the user means another.
2. Break the spec down:
   - each task fits one cc-dev session;
   - subtasks through `parent`, order through `blocked_by`;
   - every task gets a checklist and acceptance criteria;
   - estimates in the project's unit.
3. Create everything at once with `plan_create {project, items}`. It is all-or-nothing, and
   `ref` links items of the same plan.
4. Optional:
   - `epic_create` and `task_set_epic` to group tasks;
   - `cycle_create {project}` and `task_add_cycle {task, cycle}` for sprints;
   - `task_add_assignee {task, agent: "cc-dev"}` to pre-assign.
5. **Diagrams:** for flows, architecture or sequences, use
   `diagram_create {project, title, source}` with Mermaid source. If `render_error` comes back,
   fix it with `diagram_update`. Cite it in the task description: `[[TC-D1]]` (link) or
   `![[TC-D1]]` alone on a line (drawn for the human).
6. Report the created keys (tasks, epics, diagrams) to the user.
7. To see where work gets stuck before planning more: `metrics_flow {project, days}` gives lead
   and cycle time, handoffs, returns, WIP and time per agent and column. Many returns or long
   queues point to tasks that need clearer criteria or smaller scope.

## Loop — cc-dev

1. `task_next {claim: true, project: "TC"}` → the most urgent ready task, already claimed.
   - Nothing returned → stop and tell the user.
   - Read the returned `notes` first: they are what the previous agent left.
2. `task_get {task}` → description, checklist, acceptance criteria, cited `diagrams`, `revision`.
   For each cited diagram, read its source with `diagram_get`.
3. Work in this repository, following CLAUDE.md and the project rules (md/guias/):
   - dependencies only with `cargo add`;
   - i18n keys in es and en;
   - no `unwrap` outside tests.
4. While working:
   - `comment_add` at each milestone (what changed, what is next);
   - `task_check {task, kind: "checklist"|"criteria", item}` per item done;
   - `task_heartbeat {task}` if more than 10 minutes pass without writing on the task.
5. Before closing, run `cargo xtask ci`. It must pass. If it fails, do not close: fix it, or
   comment the failure and `task_escalate`.
6. Hand it to review: `task_move {task, status: "cola_de_cc_revisor", note}`. The note says
   what was done, the files touched and how it was verified. Only trivial changes (docs, typos)
   may go straight to `done`.
7. Commit only when the user asks. If you do, put the key in the message (`TC-12: …`) and record
   PRs with `git_link {task, kind: "pr", url}`.
8. Blocked:
   - `task_escalate {task, question}`, with a precise question that can be answered;
   - or `task_release {task, reason}` to leave the task for someone else.

## Loop — cc-revisor

1. `task_next {claim: true, project: "TC"}`. It offers the tasks in "Cola de cc-revisor" first,
   with cc-dev's notes, and claims them into its bench "Revisando".
2. Read the task, its criteria and cc-dev's note. Then read **every file it touched**: the
   current code and `git diff` against the last commit. Never review from the note alone.
   `task_timeline {task}` shows its journey (each column, who moved it, how long): a task that
   already came back once deserves a check that the earlier findings were fixed.
3. Answer, with reasons and file:line references, for each change:
   - **Optimal?** Algorithmic cost, allocations, work repeated every frame, caches. Why is it
     (or isn't it) the best option here?
   - **Safe?** No `unwrap`/`expect`/`panic`/indexing outside tests, inputs and sizes bounded,
     errors reported (never swallowed), no secrets in logs, views or exports.
   - **Simpler or more readable?** Dead code, names, comments that say *why*, functions too long.
   - **Duplicated?** Search the repo (`grep`) for an existing helper that does the same.
   - **Interface (if any):** gallery screenshots
     (`TASKCRAFT_GALLERY=dir cargo test -p ui-egui gallery -- --ignored`) at every window size,
     i18n es/en, text that fits, shortcuts and hover hints.
4. Run `cargo xtask ci` yourself. It must pass.
5. `comment_add` with the findings ordered by severity (Bloqueante / Importante / Menor /
   Sugerencia), each one actionable.
   - Bloqueante or Importante findings → `task_move {task, status: "cola_de_cc_dev", note}`,
     with the list to fix.
   - Otherwise → tick the criteria you verified and `task_move {task, status: "done", note}`.
6. Never fix the code yourself: the reviewer only judges and explains.

## Hand-over

- **cc-planner → cc-dev:** tasks created in `todo` (optionally assigned to cc-dev) are what cc-dev
  receives through `task_next`. Put anything cc-dev must know in the description or a comment.
- **cc-dev → cc-revisor:** when implemented and green, `task_move` to `cola_de_cc_revisor` with
  a note. The column belongs to cc-revisor, so the move assigns it.
- **cc-revisor → cc-dev:** changes requested → `task_move` to `cola_de_cc_dev` with the list.
  cc-dev's `task_next` offers it first.
- **cc-revisor → done:** approved, with a note summarising the review.
- **cc-dev → cc-planner:** if a task is too big or underspecified, do not re-plan it. Comment why,
  `task_release {reason}`, and `inbox_add` describing what needs planning.
- **Notes:** every `task_move` carries a `note`.
- **The human supervisor** watches in the app and can answer escalations at any point.

## Errors

- CONFLICT → `task_get`, merge your change into the new state, retry with the new `revision`.
- NOT_READY / CLAIMED_BY_OTHER / LOCKED_BY_HUMAN → `task_next` for another task.
- FORBIDDEN → do not retry. Check `agent_use` (no agent selected?), then `task_escalate` only if
  the task cannot be done otherwise.
- PAUSED → stop writing, tell the user, wait. **Never retry in a loop.**
- NOT_FOUND → `task_search {query}` or `task_list`.
- INVALID → fix the arguments as the message says.
- UNAVAILABLE / INTERNAL → retry once, then stop and report to the user.

## Etiquette

- Write task descriptions and comments in Spanish (the supervisor reads them in the app). Keep
  tool arguments such as statuses and levels in their English codes.
- Ideas or bugs found on the way → `inbox_add`, not new tasks (only cc-planner creates tasks).
- Pass `revision` on writes to a task you read earlier.
- Tool catalog and model facts: skills/skillbuild.md §2 and §10.
