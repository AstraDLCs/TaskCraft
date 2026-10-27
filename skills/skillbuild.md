---
name: taskcraft-skillbuild
description: Build (or update) a skill that connects an AI agent to TaskCraft through its MCP server and teaches it to work there — take tasks, plan projects and sprints, hand work over, report. Use when asked to create, review or fix a TaskCraft skill, or to connect an LLM client (Claude Code, Cursor, any MCP client) to TaskCraft.
---

# Building TaskCraft skills

> **Living document.** Last reviewed **2026-10-27** against **TaskCraft 2.0.0**.
> It is the single source of truth for every TaskCraft skill: generated skills copy their facts
> from here. When TaskCraft changes, update §2–§7 first, then add a line to §9 (Changelog), then
> regenerate or patch the skills listed in §8.

## 1. What you are building

A **skill** is a folder with a `SKILL.md` (YAML frontmatter + Markdown instructions) that an LLM
client loads when its description matches the request. A TaskCraft skill turns a generic agent
into a TaskCraft team member: it knows how to connect, who it is, which loop to follow and which
tools to call.

Every TaskCraft skill you produce must deliver three things:

1. **Connection** — how the client reaches the MCP server, with the token kept out of the skill.
2. **Identity** — which agent of the interface the session acts as, and what its *function* is.
3. **Workflow** — the exact loop for its archetype (§6), with the error handling of §2.7.

Output location:

- Source of truth in this repo: `skills/<skill-name>/SKILL.md` (plus optional `reference.md`).
- Installed copy for Claude Code: `.claude/skills/<skill-name>/SKILL.md` (project) or
  `~/.claude/skills/<skill-name>/SKILL.md` (user). Prefer a symlink to the repo copy so updates
  propagate.
- This file itself can be installed the same way (`.claude/skills/taskcraft-skillbuild/SKILL.md`
  → symlink to `skills/skillbuild.md`).

## 2. TaskCraft MCP facts (ground truth)

### 2.1 Architecture

- One **daemon** owns the workspace database (SQLite by default, PostgreSQL optional). The
  desktop app, the CLI and MCP clients all go through it. The first client starts it; it stops
  after 30 idle minutes.
- The CLI binary is `taskcraft-cli` (docs alias it as `tc`). The desktop app is `taskcraft`.
- A human **supervisor** watches everything live in the desktop app, answers escalations, can lock
  tasks, pause agents and revert changes.

### 2.2 Transports

| Transport | How | When |
|---|---|---|
| **stdio** (default) | the client launches `taskcraft-cli mcp --stdio`; it proxies to the daemon | local clients: Claude Code, Cursor, etc. |
| **HTTP** | `taskcraft-cli serve --http` → `http://127.0.0.1:7417/mcp`, header `Authorization: Bearer <token>` | remote or multi-session clients |

Useful `mcp` options / env vars: `--token` / `TASKCRAFT_AGENT_TOKEN` (required),
`--agent` / `TASKCRAFT_AGENT` (start as that agent), `--workspace` / `TASKCRAFT_WORKSPACE`
(named workspace), `--db` / `TASKCRAFT_DB` (explicit database path).

### 2.3 Authentication and identity (decisions D15, D22)

- **One interface = one LLM client = one token.** The supervisor registers it first:
  `taskcraft-cli interface register claude-code --kind claude-code --json` → field `token`
  (`tc_…`). Copy it again any time with `interface token` (or «Copy token» in the app); rotate
  with `interface rotate-token`. **Agents only exist inside an interface**: there is no
  per-agent token and no agent without an interface.
- **Agents have no token.** The interface creates them over MCP with `agent_create`
  (`name`, `description` = the agent's **function**, optional `capabilities`, `level`).
- `agent_create` needs level `plan` on the interface. An `execute`-only interface must get its
  agents created by the supervisor (`taskcraft-cli agent register <name> --interface <name>`).
- The session acts as one agent: `agent_use {agent}` switches; `agent_use {}` lists them and
  shows the active one. Any tool accepts `as_agent` for a single call as another agent of the
  same interface. With only one agent, it is used automatically. With none selected, the session
  can only read and create agents.
- History attributes each change to the concrete agent.

### 2.4 Permission levels (D11)

| Level | Can |
|---|---|
| `read` | read |
| `execute` | work on tasks: claim, comment, check, move, submit, escalate; create columns; create and edit diagrams |
| `plan` (default) | + create and reorganise tasks, plans, epics, cycles/sprints, relations, labels, agents; delete columns and diagrams |
| `admin` | + configure projects, templates, export/import |

An agent also has a **scope** (allowed projects). An agent's level never exceeds its interface's.
The supervisor can pause an agent, an interface, or the whole workspace (read-only).

### 2.5 Tools: naming and conventions

- Tools are the engine commands with `.` → `_`: `task.next` → `task_next`. In Claude Code they
  appear as `mcp__<server>__<tool>`, e.g. `mcp__taskcraft__task_next`.
- Each tool has a JSON Schema and an English description; the minimum level is stated in it.
- Tasks are referenced by **key** (`API-42`) or ULID; projects by key (`API`); diagrams by key
  (`API-D3`) or ULID; agents, cycles, epics by name or ULID. Cycles also accept `current` and
  `next`.
- **Concurrency:** pass `revision` on writes. On `CONFLICT`, re-read and retry.
- **Idempotency:** creations accept `idempotency_key`; reuse it when retrying a create.
- **Pagination:** lists accept `limit`, `cursor` (from `next_cursor`) and `fields` (projection).
- **Filters** (`task_list filter=…`): `field:value` terms joined by spaces (AND), `OR`,
  parentheses and `-` for negation. E.g. `status:todo`, `assignee:cc-dev`, `cycle:current`
  (any of its sprints is running), `cycle:"Sprint 2"`, `cycle:none`, `label:db`,
  `due<today+7d`, `ready:true`, `stale:true`. `task_next` takes no filter (only `project`).
- **Leases:** a claim lasts 15 min; every write on the task renews it; otherwise
  `task_heartbeat`. Expired leases are freed by the daemon every 30 s.
- The server sends its own **instructions** on `initialize` (the work loop in one paragraph).
  Skills must not contradict them; they add role-specific detail.

### 2.6 Model in one screen

- **Project** → workflow of **columns/stages** over base statuses (`backlog`, `todo`,
  `in_progress`, `review`, `done`, `cancelled`); custom fields; estimate unit.
- **Task**: title, description, priority, due/start, labels, checklist, acceptance criteria,
  parent/subtasks, dependencies (`blocked_by`), relations, attachments, git links, recurrence.
- **Assignees** (D20): a plain list of agents, **no roles**. If a task has assignees, only they can
  claim it. **Nobody has to approve work**; any agent may move a task to `done`.
- **Columns with an owner**: `status_create … owner=<agent>` creates an agent's queue or
  workbench. Moving a task into an owned column assigns it to that agent; `task_next` offers
  what is in your columns first. This is how work is **handed over**.
- **Epics and milestones**: groups of tasks with progress.
- **Cycles / sprints** (same concept): time boxes per project. **A task can be in one or several
  sprints.** Closing a cycle adds the next one to every open task that is not already planned in
  another open cycle; the task keeps the closed one as history. The daemon closes overdue
  cycles daily.
- **Diagrams** (D21): Mermaid diagrams per project, key `API-D3` (numbers are never reused).
  Cite one in a task description or comment: `[[API-D3]]` is a link, `![[API-D3]]` alone on a
  line shows it drawn to the human. `diagram_create`/`diagram_update` return `render_error` when
  the source cannot be drawn (it is saved anyway): fix it. `task_get` lists the cited `diagrams`;
  `diagram_get` lists `cited_by`.
- **Inbox**: ideas/bugs found on the way (`inbox_add`); the supervisor triages them.

### 2.7 Errors (every error has `code`, `message`, `hint`)

| Code | Meaning | What the skill must tell the agent to do |
|---|---|---|
| `CONFLICT` | task changed since read | re-read (`task_get`), merge, retry with the new `revision` |
| `NOT_READY` | open blockers or not claimable | `task_next` for another task |
| `CLAIMED_BY_OTHER` | another agent holds it | `task_next` |
| `LOCKED_BY_HUMAN` | supervisor lock or pending question | work on something else |
| `FORBIDDEN` | level/scope too low | do not retry; `task_escalate` if truly needed |
| `PAUSED` | supervisor paused writes | stop writing and wait; **never retry in a loop** |
| `NOT_FOUND` | bad reference | `task_search` / `task_list` |
| `INVALID` | bad arguments | fix as the message says |
| `UNAVAILABLE` / `INTERNAL` | daemon down / server error | retry once, then escalate |

### 2.8 Resources, prompts, subscriptions

- Resources: `taskcraft://project/{KEY}`, `taskcraft://project/{KEY}/tree`,
  `taskcraft://task/{KEY-N}`, `taskcraft://diagram/{KEY-DN}`, `taskcraft://activity`.
- Prompts: `plan_project {spec, project}`, `work_next_task {project}`, `status_report {project}`.
  A skill may simply invoke these and add role detail.
- `resources/subscribe` → `notifications/resources/updated` when the content really changed.

## 3. Connect a client (do this before writing the skill)

### 3.1 Supervisor side (once per client)

```sh
taskcraft-cli interface register claude-code --kind claude-code --json   # copy "token"
taskcraft-cli interface update claude-code --level plan --scope API,WEB --max-agents 10
```

Store the token in the user's environment or secret manager as `TASKCRAFT_AGENT_TOKEN`. **Never**
write it into a skill, a committed config file, a task, a comment or a log.

### 3.2 Claude Code

Project-scoped `.mcp.json` (safe to commit because the token comes from the environment):

```json
{
  "mcpServers": {
    "taskcraft": {
      "command": "taskcraft-cli",
      "args": ["mcp", "--stdio"],
      "env": { "TASKCRAFT_AGENT_TOKEN": "${TASKCRAFT_AGENT_TOKEN}" }
    }
  }
}
```

Use an absolute path for `command` if `taskcraft-cli` is not on `PATH`. Add
`"--workspace", "<name>"` to `args` for a non-default workspace. Changing the user's MCP
configuration is their decision: show the snippet and ask before editing their files.

### 3.3 Other stdio clients (Cursor, Windsurf, Antigravity, custom)

Same shape: command `taskcraft-cli`, args `["mcp", "--stdio"]`, env `TASKCRAFT_AGENT_TOKEN`.
Register a separate interface per client (`--kind cursor`, …) so the supervisor can pause each.

### 3.4 HTTP

Start `taskcraft-cli serve --http` (loopback only; outside 127.0.0.1 TLS is mandatory). Point
the client at `http://127.0.0.1:7417/mcp` with `Authorization: Bearer <token>`. Each MCP session
picks its agent with `agent_use`.

### 3.5 Verify

```sh
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
 | TASKCRAFT_AGENT_TOKEN=tc_… taskcraft-cli mcp --stdio
```

Then, inside the client: call `agent_use` with no arguments → it must answer with the interface
and its agents. The agent shows as *connected* in the app's **Agents** view.

### 3.6 Troubleshooting

| Symptom | Fix |
|---|---|
| no tools listed | wrong command path; run `taskcraft-cli mcp --help` |
| `FORBIDDEN` on every write | no agent selected (`agent_use`), or level/scope too low |
| auth error | token rotated or interface revoked → new token from the supervisor |
| tasks of the wrong workspace | add `--workspace` / `TASKCRAFT_WORKSPACE` |
| `UNAVAILABLE` after an update | an old daemon is replaced automatically; else `taskcraft-cli serve --stop` |

## 4. Procedure to build a skill

1. **Interview** (ask only what you cannot infer):
   - Archetype (§6) and the agent **name** + **function** (one sentence: what it does and does
     not do). The function becomes the agent's `description` in `agent_create`.
   - Projects in scope, required level, LLM client.
   - Hand-over chain: who comes before / after (e.g. `cc-dev` → `cc-reviewer` → done).
   - Sprint policy: does it only take work from `cycle:current`?
2. **Check the facts** in §2 against the running server (`tools/list`) if anything looks stale;
   fix this document first if it is.
3. **Write `SKILL.md`** from the template (§5), filling the archetype section from §6. Keep it
   under ~300 lines; move long tables (tool catalog) to `reference.md` next to it.
4. **Write a precise `description`**: what it does + when to trigger, mentioning *TaskCraft*.
   Bad: "Task helper". Good: "Work as cc-dev on TaskCraft: take the next ready task in the current
   sprint over MCP, implement it, and hand it to cc-reviewer. Use when asked to work on, pick up
   or continue TaskCraft tasks."
5. **Place** it in `skills/<name>/` and install (symlink) into `.claude/skills/<name>/`.
6. **Validate** with the checklist (§7) and one dry run on a throwaway workspace
   (`TASKCRAFT_DB=/tmp/tc-test/workspace.db`).
7. **Register** the skill in §8 of this document.

## 5. Template for a generated `SKILL.md`

````markdown
---
name: taskcraft-<role>
description: <What it does on TaskCraft and when to use it. Mention TaskCraft, the role and the trigger phrases.>
---

# TaskCraft — <Role title>

Built from skills/skillbuild.md (reviewed <date>). If a fact here disagrees with the server's
answers, trust the server and report the mismatch to the supervisor with inbox_add.

## Connection
- MCP server `taskcraft` (stdio: `taskcraft-cli mcp --stdio`, token in `TASKCRAFT_AGENT_TOKEN`).
- If the tools are missing, stop and tell the user how to connect (see skills/skillbuild.md §3).
  Never ask for, print or store the token.

## Identity
1. `agent_use` with no arguments. If `<agent-name>` exists, `agent_use {agent: "<agent-name>"}`.
2. Otherwise `agent_create {name: "<agent-name>", description: "<function>"}` then `agent_use`.
3. Your function: <function>. Do not do work outside it; hand it over instead.

## Loop
<archetype loop from §6, with concrete tool calls and arguments>

## Hand-over
<who comes next, their columns, the note format>

## Errors
- CONFLICT → task_get, merge, retry with the new revision.
- NOT_READY / CLAIMED_BY_OTHER / LOCKED_BY_HUMAN → task_next.
- FORBIDDEN → do not retry; task_escalate only if the task cannot be done otherwise.
- PAUSED → stop writing, tell the user, wait. Never retry in a loop.
- UNAVAILABLE / INTERNAL → retry once, then stop and report.

## Etiquette
- Comment progress with comment_add; tick checklist and criteria with task_check.
- Put the task key in commit messages ("API-42: …"); record PRs with git_link kind=pr.
- Report usage with cost_report (tokens_in, tokens_out, amount, model) when known.
- Ideas or bugs outside the task → inbox_add, not new tasks (unless you are a planner).
````

## 6. Archetypes

**Worker** (level `execute`) — implements tasks.
1. `task_next {claim: true, project?}` → the best ready task, already claimed. Nothing returned
   → stop. Read the returned `notes` first. *Sprint-only teams:* instead
   `task_list {project, filter: "cycle:current ready:true -status:done", limit: 5}`, pick the
   first, `task_claim {task}`; on `CLAIMED_BY_OTHER` try the next one.
2. `task_get` → description, checklist, acceptance criteria, `revision`.
3. Work. `comment_add` at milestones; `task_check` per item; `task_heartbeat` if quiet > 10 min.
4. Finish: `task_move {task, status: "done", note}` or move it to the next agent's queue column
   with a `note` (§2.6). Optional `task_submit_review {summary, criteria}` if the supervisor
   wants to look.
5. Blocked → `task_escalate {question}` (precise, answerable) or `task_release {reason}`.

**Planner** (level `plan`) — turns specs into tasks.
1. Read `taskcraft://project/{KEY}` (workflow, fields, estimate unit).
2. Break down: tasks fit one agent session; subtasks via `parent`; `blocked_by` for order;
   checklist + acceptance criteria on every task; estimates in the project unit.
3. `plan_create` with everything at once (all-or-nothing; `ref` links items of the same plan).
4. Optional: `epic_create` + `task_set_epic`; `task_add_assignee` to pre-assign.
5. Report the created keys.

**Sprint planner** (level `plan`; every cycle tool needs it) — runs cycles.
1. `cycle_list {project}`; `cycle_create {project}` (defaults: starts after the last one, same
   length, numbered "Sprint N"; or `name`, `start`, `end`, `days`).
2. Plan: `task_add_cycle {task, cycle}` (a task may be in several sprints; `current`/`next`
   accepted); `task_remove_cycle` to drop one; `task_set_cycle` to leave exactly one (or `none`).
3. Track: `cycle_get {cycle: "current", project}` → progress and tasks; `metrics_burndown`.
4. Close: `cycle_close` (open work is added to the next cycle, created if missing).

**Reviewer / next-in-chain** (level `execute`) — receives handed-over work.
1. Ensure own columns exist (any agent with `execute` can create columns): `status_create {project, name: "Queue of <me>", base: "todo",
   owner: "<me>"}` and `… "Bench of <me>", base: "in_progress"`.
2. `task_next {claim: true}` returns what is in its columns first, with the previous notes.
3. Move to its bench (that also claims), review, then `done` or to the next queue with a note.

**Reporter / supervisor assistant** (level `read`) — summarises.
- Use the `status_report` prompt, or `task_list` with filters, `activity_summary`,
  `taskcraft://activity`, `cycle_get current`, `metrics_*`, `cost_summary`. Cite task keys. Never
  write.

## 7. Validation checklist

- [ ] Frontmatter has `name` (kebab-case, `taskcraft-` prefix) and a trigger-rich `description`.
- [ ] No token, password or absolute personal path in the skill.
- [ ] Identity steps (`agent_use` / `agent_create`) come before any write.
- [ ] Loop uses only tools that exist (`tools/list`) and the level the agent has.
- [ ] Every error code of §2.7 has an action; `PAUSED` never retries.
- [ ] Hand-over names the target agent and its column; notes are required on `task_move`.
- [ ] Sprint behaviour is explicit (all work, or only `cycle:current`).
- [ ] Dry run on a throwaway workspace: claim → comment → check → move, visible in the app.
- [ ] Skill registered in §8 below.

## 8. Skills built from this document

| Skill | Path | Archetype | Built / reviewed |
|---|---|---|---|
| *(none yet)* | | | |

## 9. Maintaining this document

Update it whenever TaskCraft changes its MCP surface, and in the same commit:

1. Regenerate the tool list: `tools/list` (§3.5) or `taskcraft-cli commands`; diff against §10.
2. Fix §2 facts (levels, errors, transports, model), §6 loops and §10 catalog.
3. Bump "Last reviewed" at the top and add a changelog line below.
4. Patch every skill in §8 that depends on a changed fact.

### Changelog

- **2026-10-26** — First version (TaskCraft 2.0.0). Tasks can be in several sprints:
  `task_add_cycle`, `task_remove_cycle`; `task_set_cycle` now leaves exactly one.
- **2026-10-27** — D23: data management. New supervisor-only `project_delete`,
  `workspace_wipe` and `activity_prune` (written confirmation, mandatory safety copy);
  `trash_purge` accepts `backup=true`.
- **2026-10-27** — D22: interfaces first. `agent_register` requires `interface` and no longer
  creates an interface or returns a token; `agent_rotate_token` is gone (use
  `interface_rotate_token`); new `interface_token` (supervisor) returns the stored token;
  `interface_update` accepts `name` and `color`. Interface names may contain spaces.
- **2026-10-27** — Mermaid diagrams (RF-MOD-19, D21): `diagram_*` tools, resource
  `taskcraft://diagram/{KEY-DN}`, citations `[[API-D3]]` / `![[API-D3]]`, `diagrams` in
  `task_get`. `diagram_delete` needs level `plan`.

## 10. Tool catalog (TaskCraft 2.0.0)

MCP name = command with `_` (`task.next` → `task_next`). Plus the MCP-only `agent_use`.
`session.*` and `system.*` are not exposed.

| Domain | Tools |
|---|---|
| Work loop | `task_next`, `task_claim`, `task_heartbeat`, `task_release`, `task_submit_review`, `task_escalate`, `task_move`, `task_check` |
| Tasks | `task_create`, `task_get`, `task_list`, `task_search`, `task_update`, `task_set_parent`, `task_assign`, `task_add_assignee`, `task_remove_assignee`, `task_label`, `task_set_field`, `task_archive`, `task_unarchive`, `task_delete`, `task_restore`, `task_stale`, `task_someday`, `task_matrix`, `task_review_parent`, `task_export_csv` |
| Checklist / criteria | `checklist_add`, `checklist_edit`, `checklist_check`, `checklist_remove`, `criteria_add`, `criteria_edit`, `criteria_check`, `criteria_remove` |
| Relations | `task_link`, `task_unlink`, `task_relate`, `task_unrelate`, `project_graph`, `project_critical_path` |
| Planning | `plan_create`, `epic_create`, `epic_update`, `epic_close`, `epic_reopen`, `epic_delete`, `epic_list`, `epic_get`, `task_set_epic`, `task_set_milestone`, `template_*` |
| Diagrams | `diagram_create`, `diagram_list`, `diagram_get` (`svg=true` for the rendered SVG), `diagram_update`, `diagram_delete` |
| Sprints | `cycle_create`, `cycle_list`, `cycle_get`, `cycle_update`, `cycle_close`, `cycle_delete`, `task_add_cycle`, `task_remove_cycle`, `task_set_cycle` |
| Recurrence / today | `task_set_recurrence`, `task_clear_recurrence`, `today_mark`, `today_unmark`, `today_list` |
| Projects / workflow | `project_create`, `project_list`, `project_get`, `project_update`, `project_archive`, `project_unarchive`, `status_list`, `status_create`, `status_update`, `status_delete`, `field_*`, `label_create`, `label_list`, `label_update` |
| Agents / interfaces | `agent_use`, `agent_create`, `agent_mine`, `agent_list`, `agent_get`, `agent_update`, `agent_pause`, `agent_resume`, … `interface_*` (supervisor) |
| Communication | `comment_add`, `comment_list`, `activity_list`, `activity_summary`, `inbox_add`, `inbox_list` |
| Time / cost / metrics | `timer_start`, `time_log`, `time_report`, `time_sheet`, `cost_report`, `cost_list`, `cost_summary`, `metrics_burndown`, `metrics_cfd`, `metrics_agents` |
| Integrations | `git_link`, `git_unlink`, `git_links`, `attachment_add`, `attachment_list`, `attachment_get`, `attachment_remove`, `view_save`, `view_list`, `view_get` |
| Supervisor only | `task_answer`, `task_reject`, `lock_set`, `lock_clear`, `activity_revert`, `inbox_convert`, `inbox_discard`, `agent_register` (needs `interface`), `interface_register`, `interface_token`, `interface_rotate_token`, `workspace_configure`, `workspace_web_token`, `automation_*`, `webhook_create`, `webhook_delete`, `webhook_record`, `cycle_rollover` |
| Supervisor only (D23, delete data) | `project_delete` (confirm = key), `workspace_wipe` (confirm = name), `activity_prune` — always after a `.taskcraft` safety copy |
| Level `admin` | `project_create`, `project_update`, `project_archive`, `project_unarchive`, `field_*` writes, `template_create`, `template_from_task`, `template_delete`, `epic_delete`, `workspace_export`, `workspace_import`, `workspace_import_preview`, `workspace_export_markdown`, `workspace_backup`, `task_export_csv`, `trash_purge` |
