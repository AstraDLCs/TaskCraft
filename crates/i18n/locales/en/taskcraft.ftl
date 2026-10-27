## TaskCraft — English.
## Every key must also exist in es/taskcraft.ftl with the same variables
## (checked by the i18n crate's parity test).

## Model value names

status-backlog = Backlog
status-todo = To do
status-in_progress = In progress
status-in_review = In review
status-done = Done
status-cancelled = Cancelled

category-pending = Pending
category-active = Active
category-review = In review
category-done = Done
category-cancelled = Cancelled

priority-urgent = Urgent
priority-high = High
priority-medium = Medium
priority-low = Low
priority-none = None

project-status-active = Active
project-status-archived = Archived


agent-status-active = Active
agent-status-disabled = Disabled
agent-status-revoked = Revoked

## Value parsing errors

error-parse-status = Invalid status `{ $value }`; valid values: { $expected }
error-parse-category = Invalid category `{ $value }`; valid values: { $expected }
error-parse-priority = Invalid priority `{ $value }`; valid values: { $expected }
error-parse-project-status = Invalid project status `{ $value }`; valid values: { $expected }
error-parse-agent-status = Invalid agent status `{ $value }`; valid values: { $expected }
error-parse-action = Invalid history action `{ $value }`; valid values: { $expected }
error-parse-entity = Invalid entity kind `{ $value }`; valid values: { $expected }
error-parse-actor = Invalid actor `{ $value }`; use supervisor, system, agent:<ulid> or integration:<name>

## Domain rules

rule-project-key = Invalid project key `{ $key }`: 2–10 characters A–Z or 0–9, starting with a letter
rule-amount = Invalid amount `{ $value }`: an unsigned number with up to 6 decimals (0.42), from 0 to 1,000,000
rule-currency = Invalid currency `{ $value }`: a three-letter code such as USD or EUR
rule-title = The title must not be empty or longer than 200 characters
rule-color = Invalid color `{ $color }`: use the #rrggbb format
rule-label-name = Invalid label name: 1–40 characters, no commas
rule-agent-name = Invalid agent name `{ $name }`: 1–40 characters a–z, 0–9, dot, dash or underscore
rule-comment-body = The comment must not be empty or longer than 10,000 characters
rule-transition = Transition not allowed: { $from } -> { $to }; from { $from } you can go to: { $allowed }

## Storage

store-backend = Storage error: { $detail }
store-corrupt = Corrupt data in storage: { $detail }
store-newer-schema = The workspace uses schema v{ $found }, newer than the supported one (v{ $supported }); update TaskCraft
store-older-schema = The workspace is at schema v{ $found } and this version uses v{ $supported }: an older daemon has it open; stop it with `taskcraft-cli serve --stop`
store-io = File error: { $detail }

## General command errors

error-unknown-command = Unknown command `{ $id }`; `commands` lists the available ones
error-missing-arg = Missing argument `{ $name }`
error-invalid-arg = Invalid argument `{ $name }`: { $reason }
error-panicked = Command { $id } failed unexpectedly; no changes were saved

arg-not-number = `{ $value }` is not an integer
arg-bad-date = Invalid date `{ $value }`: use 2026-11-01, 2026-11-01T17:00 or 2026-11-01T17:00-05:00[America/Lima] ({ $detail })
arg-not-bool = `{ $value }` is not true/false
arg-bad-task-ref = `{ $value }` is neither a key (API-42) nor a ULID
arg-bad-id = `{ $value }` is not a valid ID (26-character ULID)
arg-out-of-range = `{ $value }` is out of range
arg-range = Must be between { $min } and { $max }
arg-empty = Must not be empty
arg-unknown = Unknown argument; valid ones: { $valid }
arg-length = Must be between { $min } and { $max } characters long

## Projects, tasks and relations

notfound-project = Project `{ $project }` does not exist; `project.list` shows the available ones
notfound-task = Task `{ $task }` does not exist
notfound-task-project = The task's project is missing
notfound-parent = Parent task { $id } is missing
notfound-label = Label `{ $label }` does not exist
notfound-dependency = That dependency does not exist
notfound-checklist-item = Checklist step `{ $item }` does not exist (there are { $count })
notfound-criterion = Acceptance criterion `{ $item }` does not exist (there are { $count })
task-in-trash = Task `{ $task }` is in the trash; `task.restore` brings it back
task-archived = Task `{ $task }` is archived; `task.unarchive` reactivates it
task-new-status = A new task starts in backlog or todo
hierarchy-corrupt = Corrupt task hierarchy (cycle)
project-key-taken = A project with key { $key } already exists
project-archived = Project { $project } is archived; `project.unarchive` reactivates it
label-taken = Label `{ $name }` already exists
parent-other-project = The parent task belongs to another project
parent-is-descendant = A task cannot be placed under itself or one of its subtasks
depth-exceeded = The maximum subtask depth ({ $max }) would be exceeded; `workspace.configure max_depth=N` changes it
dependency-cycle = The dependency would create a cycle: { $path }
archive-open-tasks = { $count ->
    [one] Only closed tasks can be archived; still open: { $tasks }
   *[other] Only closed tasks can be archived; still open: { $tasks }
}
not-archived = The task is not archived
parent-archived = The parent task is archived too; unarchive it first
not-in-trash = The task is not in the trash
parent-in-trash = The parent task is in the trash too; restore it first

## Agents, sessions and comments

notfound-agent = Agent `{ $agent }` does not exist; `agent.list` shows the registered ones
agent-name-taken = An agent named `{ $name }` already exists
agent-disabled = Agent `{ $agent }` is disabled and cannot act; the supervisor can re-enable it with `agent.enable`
agent-revoked = Agent `{ $agent }` is revoked and cannot act
agent-unknown = Agent { $id } is not registered; the supervisor must register it with `agent.register`
agent-revoked-final = Agent `{ $agent }` is revoked; revocation is permanent
supervisor-only = Only the supervisor can do this
session-agent-only = Only an agent can open or use a session
notfound-session = Session `{ $session }` does not exist
session-other-agent = The session belongs to another agent

## Command descriptions

cmd-workspace-get = Show the workspace and its settings
cmd-workspace-configure = Configure the workspace: max_depth (1–20), trash_days (1–3650), stale_weeks (weeks to go stale), urgent_days (days for the matrix)
cmd-project-create = Create a project: key (API), name, description
cmd-project-list = List projects; archived=true includes archived ones
cmd-project-get = Show a project with its task count per status
cmd-project-update = Edit a project: name, description, close_policy
cmd-project-archive = Archive a project
cmd-project-unarchive = Reactivate an archived project
cmd-task-create = Create a task: project, title, description, priority, status, parent, start, due, labels
cmd-task-get = Show a task in detail (key API-42 or ULID)
cmd-task-list = List tasks: project, status (list), parent (key or root), label, archived
cmd-task-update = Edit a task: title, description, priority, start, due (empty clears)
cmd-task-move = Move a task to another column: task, status (base status or column) and note (a message for whoever comes next). A column with an owner assigns it to that agent; an agent moving it to in progress claims it
cmd-task-set_parent = Move a task under another one (parent) or to the root (parent=none)
cmd-task-assign = Leave a single assignee: to = supervisor, agent:<id>, an agent name or none (to add without removing, task.add_assignee)
cmd-task-label = Label a task: add and remove (comma-separated lists)
cmd-task-link = Create a dependency: blocker blocks blocked (keys or IDs)
cmd-task-unlink = Remove a dependency: blocker, blocked
cmd-task-archive = Archive a done or cancelled task (with its subtasks)
cmd-task-unarchive = Reactivate an archived task
cmd-task-auto_archive = Archive closed tasks unchanged for `days` days (default 30)
cmd-task-delete = Move a task (with its subtasks) to the trash
cmd-task-restore = Restore a task from the trash
cmd-trash-list = Show the trash
cmd-trash-purge = Permanently delete what has been in the trash for more than `days` days (default: the workspace setting)
cmd-checklist-add = Add a checklist step: task, text
cmd-checklist-check = Check a checklist step: task, item (number or ID), done (default true)
cmd-checklist-remove = Remove a checklist step: task, item
cmd-criteria-add = Add an acceptance criterion: task, text
cmd-criteria-check = Mark an acceptance criterion as met: task, item (number or ID), done (default true)
cmd-criteria-remove = Remove an acceptance criterion: task, item
cmd-label-create = Create a label: name, color (#rrggbb)
cmd-label-list = List the workspace labels
cmd-agent-register = Add an agent to an interface (supervisor only; the token belongs to the interface): name, interface, model, description (its function), color, capabilities, level
cmd-agent-list = List agents and whether they are connected
cmd-agent-get = Show an agent: agent (name or ID)
cmd-agent-update = Edit an agent (supervisor only): agent, interface (move it to another), model, description (its function), color, level, scope, capabilities
cmd-agent-disable = Temporarily disable an agent (supervisor only)
cmd-agent-enable = Re-enable a disabled agent (supervisor only)
cmd-agent-revoke = Permanently revoke an agent; its history is kept (supervisor only)
cmd-session-open = Open a session for the calling agent: transport
cmd-session-heartbeat = Renew a session heartbeat: session
cmd-session-close = Close a session: session
cmd-activity-list = Show the history: task, project, actor, limit, before
cmd-comment-add = Comment on a task: task, body (Markdown)
cmd-comment-list = Show a task's comments: task
cmd-test-boom = Simulated failure (tests only)

## Command parameters

param-max_depth = Maximum subtask depth (1–20)
param-trash_days = Days deleted items can be restored (1–3650)
param-key = Short project key, for example API
param-name = Name
param-description = Description in Markdown
param-archived = Include archived items (true/false)
param-project = Project: key (API) or ID
param-title = Title (1–200 characters)
param-priority = Priority: urgent, high, medium, low or none
param-status = Status: backlog, todo, in_progress, in_review, done, cancelled or a project stage
param-parent = Parent task: key (API-42), ID, or none/root depending on the command
param-start = Start date (empty clears it)
param-due = Due date (empty clears it)
param-labels = Comma-separated labels
param-task = Task: key (API-42) or ID
param-label = Label name
param-revision = Expected revision; if the task changed since, the write is rejected with CONFLICT
param-to = Assignee: supervisor, agent:<id>, integration:<name> or none
param-add = Labels to add, comma-separated
param-remove = Labels to remove, comma-separated
param-text = Step or criterion text
param-item = Item number (1 = first) or ID
param-done = Checked (true) or unchecked (false); default true
param-color = Color #rrggbb
param-blocker = Blocking task
param-blocked = Blocked task
param-agent = Agent: name or ID
param-model = Model of the agent or of the reported work (free text)
param-transport = Session transport: cli, stdio, http…
param-session = Session ID
param-actor = Actor: supervisor, system, agent name or integration:<name>
param-action = Comma-separated actions: created, updated, deleted, linked, unlinked
param-entity = Entity type: workspace, project, task, label, dependency, comment, agent or inbox
param-limit = Maximum number of results
param-before = Cursor: only entries older than this ID
param-body = Text in Markdown

## Coordination

conflict-revision = Task { $task } changed (revision { $current }, you expected { $expected }); read it again with `task.get` and retry
conflict-changed-by-other = Another actor (usually the supervisor) changed { $task } since your last write; check its current state and retry with revision={ $current }
locked-project = The project of { $task } is locked by the supervisor; you can read it but not modify it
locked-task = { $task } is locked by the supervisor; you can read it but not modify it
needs-human-pending = { $task } is waiting for an answer from the supervisor; pick another task with `task.next`
claimed-by-other = { $task } is claimed by { $agent } until { $until }; pick another one with `task.next`
agent-only = Only an agent can do this
not-claimable = { $task } is { $status } and cannot be claimed
not-ready-blocked = { $task } is not ready: it is blocked by { $blockers }
agent-wip-limit = You already have { $limit } task(s) in progress ({ $tasks }); finish, submit or release it before claiming another
not-your-claim = You have not claimed { $task }; claim it with `task.claim`
answer-no-question = { $task } has no pending question
lock-target-required = Pass task= or project= (exactly one)
plan-invalid = Invalid plan: { $detail }
plan-duplicate-ref = Reference `{ $ref }` appears more than once in the plan
plan-unresolved = These parents could not be resolved (they do not exist or form a cycle): { $refs }
notfound-inbox-item = Inbox item `{ $item }` does not exist
inbox-not-open = Inbox item `{ $item }` was already converted or discarded
error-parse-inbox-kind = Invalid inbox item kind `{ $value }`; valid values: { $expected }
error-parse-inbox-status = Invalid inbox item status `{ $value }`; valid values: { $expected }
inbox-kind-idea = Idea
inbox-kind-bug = Bug
inbox-kind-debt = Tech debt
inbox-status-open = Open
inbox-status-converted = Converted
inbox-status-discarded = Discarded
activity-lease-expired = Lease expired: { $target } is available again
cmd-task-claim = Claim a task (agents only): it is assigned to you with a lease you must renew
cmd-task-heartbeat = Renew the lease of a claimed task
cmd-task-release = Release a claimed task; agents must give a reason
cmd-system-expire_leases = Release tasks whose lease expired (run by the system)
cmd-task-next = Your next task: first those in your columns, then those assigned to you, then the highest priority; claim=true claims it in the same step
cmd-task-submit_review = Hand in: moves it to In review with summary (what was done) and criteria (met acceptance criteria); nobody has to approve it
cmd-task-escalate = Ask the supervisor for help: question; the task becomes locked for agents
cmd-task-answer = Answer an escalated task (supervisor only): answer
cmd-lock-set = Lock a task or project for agents (supervisor only)
cmd-lock-clear = Remove the human lock from a task or project (supervisor only)
cmd-plan-create = Create a whole task tree in one operation: items (JSON with ref, title, parent, blocked_by…)
cmd-inbox-add = Drop a finding in the inbox: title, body, kind (idea, bug, debt)
cmd-inbox-list = Show the inbox (open items by default)
cmd-inbox-convert = Turn an inbox item into a project task (supervisor only)
cmd-inbox-discard = Discard an inbox item (supervisor only)
param-reason = Reason, for the history
param-summary = Summary of what was done (Markdown)
param-criteria = Met criteria: comma-separated numbers or IDs
param-question = Question for the supervisor
param-answer = Answer for the agent
param-claim = true to claim the task in the same step
param-items = JSON list of items: ref, title, description, priority, status, parent, blocked_by, labels, checklist, criteria, start, due
param-kind = Type: idea, bug or debt (inbox); branch, commit or pr (git); claude-code, cursor… (interface); checklist or criteria (task.check); epic or milestone; task or project (templates); fields: text, number, select…
param-lease_minutes = Minutes a claim lasts without a heartbeat (1–1440)
param-agent_wip_limit = Tasks in progress at once per agent (1–50)
param-auto_archive_days = Days without changes before closed tasks archive themselves (1–3650)

cmd-label-update = Edit a label: label (current name), name, color
cmd-checklist-edit = Change the text of a checklist step: task, item, text
cmd-criteria-edit = Change the text of an acceptance criterion: task, item, text
cmd-task-reject = Reject a submission (supervisor only): reason; keep_assignee=true sends it back to the same agent
param-keep_assignee = true to send it back to the same agent
reject-not-in-review = { $task } is not in review

## History

activity-created = { $actor } created { $target }
activity-updated = { $actor } changed { $fields } on { $target }
activity-deleted = { $actor } deleted { $target }
activity-linked = { $actor } linked { $target }
activity-commented = { $actor } commented on { $target }
activity-unlinked = { $actor } unlinked { $target }

## Field names in the history (optional: untranslated ones show the technical name)

field-title = title
field-description = description
field-status = status
field-priority = priority
field-start = start
field-due = due date
field-labels = labels
field-assignee = assignee
field-parent_id = parent task
field-checklist = checklist
field-criteria = criteria
field-archived_at = archived
field-deleted_at = trash
field-name = name
field-model = model
field-color = color
field-close_policy = close policy
field-settings = settings
field-key = key
field-body = text
field-claim = claim
field-needs_human = question for the supervisor
field-locked = human lock
field-revision = revision
field-kind = kind
field-task_id = task

## Desktop app

ui-projects = Projects
ui-no-projects = No projects yet
ui-new-project = New project
ui-key-hint = Task prefix
ui-key-explain = Names its tasks: with WEB they become WEB-1, WEB-2… (2–10 letters or digits)
ui-name-hint = Name
ui-create-project = Create project
ui-empty-hint = Create a project on the left, or from the terminal:
ui-new-task-hint = New task…
ui-create-task = Create task
ui-no-tasks = No tasks.
ui-col-key = Key
ui-col-title = Title
ui-col-status = Status
ui-col-priority = Priority
ui-col-labels = Labels
ui-col-checklist = Checklist
ui-blocked = blocked
ui-blocked-hint = Has open blockers (task.get shows which)
ui-review = review
ui-no-description = (no description)
ui-ready = Ready
ui-command-ok = Done: { $id }
ui-delete-task = Delete task
ui-task-deleted = { $task } deleted
ui-undo = Undo
ui-activity = Activity
ui-filter-all = All
ui-filter-actor = Who
ui-filter-project = Project
ui-filter-event = Type
ui-event-created = Created
ui-event-updated = Changes
ui-event-deleted = Deletions
ui-event-links = Dependencies
ui-event-comments = Comments
ui-no-activity = No activity yet
ui-activity-count = { $count ->
    [one] 1 change
   *[other] { $count } changes
}
ui-language = Language
ui-open-failed = Could not open the workspace: { $error }

ui-menu-file = File
ui-menu-view = View
ui-menu-project = Project
ui-menu-workspace = Workspace
ui-menu-help = Help
ui-new-task = New task
ui-palette = Command palette
ui-palette-hint = Search a command…
ui-run = Run
ui-quit = Quit
ui-view-list = List
ui-view-board = Board
ui-view-calendar = Calendar
ui-view-review = Review
ui-view-inbox = Inbox
ui-view-agents = Agents
ui-view-trash = Trash
ui-hide-closed = Hide closed
ui-show-archived = Show archived
ui-theme = Theme
ui-theme-system = System
ui-theme-light = Light
ui-theme-dark = Dark
ui-edit-project = Edit project
ui-lock = Lock for agents
ui-unlock = Remove lock
ui-archive = Archive
ui-unarchive = Reactivate
ui-settings = Settings
ui-labels = Labels
ui-shortcuts = Keyboard shortcuts
ui-all-projects = All projects
ui-lanes-none = No lanes
ui-lanes-assignee = Lanes by assignee
ui-lanes-priority = Lanes by priority
ui-lanes-project = Lanes by project
ui-roots-only = Top-level tasks only
ui-pick-project = Pick a project on the left to create tasks
ui-search-hint = Search…
ui-any-status = all
ui-any-priority = Any priority
ui-any-label = Any label
ui-anyone = Anyone
ui-unassigned = Unassigned
ui-clear-filters = Clear filters
ui-col-assignee = Assignee
ui-col-due = Due
ui-col-start = Start
ui-col-progress = Progress
ui-expand = Expand
ui-collapse = Collapse
ui-add-subtask = Add subtask
ui-new-subtask-hint = Subtask title (Enter to create)
ui-subtask-of = sub of { $parent }
ui-lease-left = expires in { $n } min
ui-ago-now = just now
ui-ago-min = { $n } min ago
ui-ago-hours = { $n } h ago
ui-ago-days = { $n } d ago
ui-locked-hint = Locked by the supervisor: agents cannot modify it
ui-no-date = no date
ui-today = Today
ui-clear-date = Clear date
ui-previous = Previous
ui-next = Next
ui-month = Month
ui-week = Week
ui-wd-mon = Mon
ui-wd-tue = Tue
ui-wd-wed = Wed
ui-wd-thu = Thu
ui-wd-fri = Fri
ui-wd-sat = Sat
ui-wd-sun = Sun
ui-month-1 = January
ui-month-2 = February
ui-month-3 = March
ui-month-4 = April
ui-month-5 = May
ui-month-6 = June
ui-month-7 = July
ui-month-8 = August
ui-month-9 = September
ui-month-10 = October
ui-month-11 = November
ui-month-12 = December
ui-new-task-on-day = New task on this day
ui-more = +{ $n } more
ui-unscheduled = No date
ui-unscheduled-hint = Drag them onto a day to schedule them
ui-unscheduled-open-hint = Open them to set a date
ui-review-questions = Questions from agents
ui-review-submissions = Submissions to review
ui-review-none = Nothing pending
ui-answer = Answer
ui-answer-hint = Your answer for the agent…
ui-approve = Approve
ui-reject = Reject
ui-reject-hint = What is missing or needs fixing…
ui-reject-keep = Send it back to the same agent
ui-cancel = Cancel
ui-criteria = Acceptance criteria
ui-checklist = Checklist
ui-inbox-empty = The inbox is empty
ui-convert = Turn into task
ui-discard = Discard
ui-agent-edit = Edit agent
ui-agents-connected = { $n } of { $total } agents connected
ui-agents-empty = No interfaces or agents yet
ui-connected = connected
ui-idle = idle
ui-last-seen = seen
ui-agent-no-work = Has no claimed tasks
ui-edit = Edit
ui-disable = Disable
ui-enable = Re-enable
ui-revoke = Revoke
ui-trash-hint = Deleted items can be restored for { $days } days
ui-trash-empty = The trash is empty
ui-trash-empty-now = Empty trash
ui-restore = Restore
ui-close = Close
ui-claimed-since = claimed it { $ago }
ui-release = Release
ui-release-reason = Reason (optional)
ui-needs-human = Needs your answer
ui-remove = Remove
ui-add-label = Add label
ui-new-label-hint = New label…
ui-add = Add
ui-parent = Parent task
ui-root = None (top-level task)
ui-created-by = Created by
ui-description = Description
ui-save = Save
ui-markdown-hint = Markdown supported
ui-add-item-hint = Add and press Enter…
ui-item-removed = Removed: { $item }
ui-subtasks = Subtasks
ui-dependencies = Dependencies
ui-blocked-by = Blocked by
ui-blocks = Blocks
ui-add-blocker = Add blocker…
ui-unlink = Remove dependency
ui-unlinked = { $blocker } no longer blocks { $blocked }
ui-comments = Comments
ui-comment = Comment
ui-comment-hint = Write a comment (Markdown)…
ui-history = History
ui-confirm = Confirm
ui-confirm-revoke = Revoke { $agent } forever? Its history is kept, but it will never be able to act again.
ui-shortcut-palette = Command palette
ui-shortcut-new-task = Type a new task
ui-shortcut-delete = Delete the selected task (with undo notice)
ui-shortcut-escape = Close dialog or deselect

## CLI

cli-about = Run TaskCraft commands from the terminal
cli-db-help = Workspace database (default: TASKCRAFT_DB or the user data directory)
cli-as-help = Act as the supervisor (default)
cli-agent-help = Act as the registered agent with this name (also TASKCRAFT_AGENT)
cli-lang-help = Output language: es or en (default: the system's; English for agents)
cli-commands-help = List the registered commands
cli-where-help = Show the path of the database in use
cli-run-help = Run a command: run task.create project=API title="Fix leak"
cli-run-id-help = Command ID (domain.action)
cli-run-args-help = key=value arguments
cli-bad-pair = Invalid argument `{ $pair }`: expected key=value
cli-open-failed = Could not open { $path }: { $error }

# Text size
ui-text-size = Text size
ui-text-small = Small
ui-text-normal = Normal
ui-text-large = Large
ui-text-huge = Extra large

# Subtasks: changing the parent
ui-open = Open
ui-search-task = Search by key or title…
ui-no-matches = No matching tasks
ui-make-subtask-of = Make subtask of { $parent }
ui-outdent = Move up one level
ui-make-root = Make top-level task
ui-sidebar-expand = Show the sidebar
ui-sidebar-collapse = Collapse the sidebar
ui-tasks = Tasks
ui-back-to = Back to { $view }
ui-filters = Filters
ui-filters-count = Filters ({ $count })
ui-show-activity = Show or hide the activity
ui-move-under = Move into…
ui-move-up = Move up
ui-move-down = Move down
ui-open-link = Open link
ui-drag-to-nest = Drag onto another task to make it a subtask
ui-drop-to-make-root = Drop here to make it a top-level task
ui-shortcut-indent = Make subtask of the previous task
ui-shortcut-outdent = Move subtask up one level
ui-detach-subtask = Take out of this task (up one level)
ui-adopt-subtask = Add existing task…

# M2: daemon, MCP y CLI
arg-one-of = `{ $value }` is not valid; use one of: { $valid }
agent-token-unavailable = The system could not generate a secure token; try again
cmd-task-search = Search tasks by text in title, description and comments: query, project
cmd-task-check = Check a checklist step or an acceptance criterion: kind (checklist or criteria), item, done
param-cursor = Pagination cursor returned as next_cursor
param-fields = Fields to return for each item, comma-separated
param-query = Text to search for
agent-bad-token = Invalid token, or its interface is revoked or disabled; the supervisor can issue a new one with `interface.rotate_token`
cli-token-help = Agent token (also TASKCRAFT_AGENT_TOKEN); identifies the agent without its name
cli-json-help = Print the response as JSON
cli-offline-help = Open the database in this process, without the daemon (for debugging)
cli-status-help = Tell whether this workspace's daemon is running
cli-serve-help = Start the workspace daemon (sole owner of the engine)
cli-serve-http-help = Also serve MCP over HTTP on this address (default 127.0.0.1:7417)
cli-serve-idle-help = Stop after this many minutes without clients
cli-serve-stop-help = Stop the running daemon
cli-mcp-help = MCP server for agents
cli-mcp-stdio-help = Over stdin/stdout, proxying to the daemon (default)
cli-domain-help = { $domain } commands
cli-missing-action = Missing subcommand; use --help to see the available ones
cli-more = Showing { $shown } of { $total }; next page with --cursor { $cursor }
cli-daemon-running = Daemon running (PID { $pid }) on { $socket }
cli-daemon-not-running = No daemon on { $socket }; it starts by itself when you run any command
cli-daemon-stopped = Daemon stopped
cli-daemon-listening = Daemon listening on { $socket }
cli-mcp-http = MCP over HTTP at { $url } (Authorization: Bearer <agent token>)
serve-already-running = A daemon is already running for { $db }
serve-io = Daemon I/O error: { $detail }
client-unavailable = Could not connect to the daemon ({ $socket }): { $detail }
client-rejected = The daemon rejected the connection: { $detail }
client-io = Lost the connection to the daemon: { $detail }
client-protocol = Unexpected reply from the daemon: { $detail }
agent-token-required = This daemon requires a token to act as an agent (TASKCRAFT_AGENT_TOKEN)
ui-local-mode = No daemon, working in this process: { $reason }
ui-workspace-busy = Another process has { $db } open and is not responding ({ $reason }); stop it with `taskcraft-cli serve --stop` and open TaskCraft again

# M4: exportación e importación
cmd-workspace-export = Export the workspace (or some projects) to a .taskcraft file: path, projects
cmd-workspace-import_preview = Show what an import would create, what collides and which errors there are, without changing anything: path, mode
cmd-workspace-import = Import a .taskcraft file in a single transaction: mode workspace (empty), project (as new projects) or merge (by ULID)
param-path = File path
param-projects = Comma-separated project keys (empty = the whole workspace)
param-mode = workspace (into an empty workspace, keeping IDs), project (as new projects) or merge (updates by ULID whatever is newer and creates what is missing)
exchange-not-taskcraft = Not a valid .taskcraft file: { $detail }
exchange-newer-format = The file uses format { $found } and this version only understands up to { $supported }; update TaskCraft
exchange-invalid = The file has { $count } problem(s); the first one: { $first }
exchange-target-not-empty = Importing as a workspace needs an empty workspace; use mode=project to add it as new projects
exchange-io = Could not read or write the file: { $detail }
ui-export = Export…
ui-import = Import…
ui-export-only = Only project { $project }
ui-export-done = Exported { $tasks } tasks to { $path }
ui-import-workspace = Into this (empty) workspace
ui-import-projects = As new projects
ui-import-preview = Preview
ui-import-done = Import done
ui-import-hint = Press "Preview" to see what would be imported
ui-import-counts = { $projects } projects, { $tasks } tasks, { $comments } comments, { $agents } agents, { $activity } history entries

# M5: modelo ampliado
estimate-unit-hours = Hours
estimate-unit-points = Points
estimate-unit-size = Size (S/M/L/XL)
field-kind-text = Text
field-kind-number = Number
field-kind-select = Select
field-kind-multiselect = Multi-select
field-kind-bool = Yes/no
field-kind-date = Date
field-kind-url = Link
epic-kind-epic = Epic
epic-kind-milestone = Milestone
epic-status-open = Open
epic-status-closed = Closed
template-kind-task = Task
template-kind-project = Project
relation-relates = Related
relation-duplicates = Duplicate of
error-parse-estimate-unit = Invalid estimate unit `{ $value }`; values: { $expected }
error-parse-field-kind = Invalid field type `{ $value }`; values: { $expected }
error-parse-epic-kind = Invalid kind `{ $value }`; values: { $expected }
error-parse-epic-status = Invalid epic status `{ $value }`; values: { $expected }
error-parse-template-kind = Invalid template kind `{ $value }`; values: { $expected }
error-parse-relation = Invalid relation `{ $value }`; values: { $expected }
rule-unknown-status = Status `{ $key }` does not exist in this project; use one of: { $valid }
rule-stage-key = Invalid stage name `{ $name }`: use letters and digits (up to 40)
rule-stage-exists = The workflow already has a `{ $key }` column
rule-estimate = Invalid estimate `{ $value }` for unit { $unit }: a number (hours, points) or S, M, L, XL (sizes)
rule-field-key = Invalid field name `{ $name }`
rule-field-exists = This project already has a `{ $key }` field
rule-field-options = Options are required for select fields (and only for them), without commas or duplicates
rule-unknown-field = Field `{ $key }` does not exist; fields: { $valid }
rule-field-value = Invalid value `{ $value }` for field { $field } ({ $kind })
notfound-epic = Epic or milestone `{ $epic }` does not exist
epic-ambiguous = Several epics are called `{ $epic }`; use its ID or give the project
notfound-status = Column `{ $status }` does not exist; columns: { $valid }
status-base-permanent = `{ $status }` is a base status: it can be renamed or moved, not deleted
template-invalid = Invalid template definition: { $detail }
template-name-taken = A template called `{ $name }` already exists
notfound-template = Template `{ $template }` does not exist
attachment-bad-base64 = The content is not valid base64
attachment-source = Give the content as data (base64) or path (local file), not both
attachment-too-big = The attachment is { $size } KB and the maximum is { $max } MB (workspace.configure max_attachment_mb)
notfound-attachment = Attachment `{ $attachment }` does not exist
relation-self = A task cannot be related to itself
notfound-relation = Those tasks do not have that relation
locked-epic = { $task } is in { $epic }, locked by the supervisor: agents can read it but not change it
cmd-status-list = Workflow columns of a project: base statuses and stages, with their task counts
cmd-status-create = Create a column in a project workflow: name, base (status it belongs to), owner (owning agent: its queue or its workbench) and position
cmd-status-update = Rename, move or change the owner of a column: status, name, owner, position
cmd-status-delete = Delete a custom column (plan level); its tasks stay in the base status
cmd-field-list = Custom fields of a project
cmd-field-create = Create a custom field (supervisor only): name, kind, options
cmd-field-update = Change the name or options of a field (supervisor only)
cmd-field-delete = Delete a field and its values (supervisor only)
cmd-task-set_field = Set a custom field of a task (empty clears it): field, value
cmd-epic-create = Create an epic or milestone: project, name, kind, description, target
cmd-epic-update = Edit an epic or milestone: name, description, target
cmd-epic-close = Close an epic or milestone
cmd-epic-reopen = Reopen an epic or milestone
cmd-epic-delete = Delete an epic or milestone (supervisor only); its tasks are left without it
cmd-epic-list = Epics and milestones with their progress: project, kind, status
cmd-epic-get = An epic or milestone with its progress and tasks
cmd-task-set_epic = Put a task in an epic (none removes it)
cmd-task-set_milestone = Put a task in a milestone (none removes it)
cmd-template-create = Create a template (supervisor only): name, kind, definition (JSON with items and, for projects, project)
cmd-template-from_task = Save a task and its subtasks as a template (supervisor only): task, name
cmd-template-list = Available templates
cmd-template-get = A template with its definition
cmd-template-delete = Delete a template (supervisor only)
cmd-template-instantiate = Create tasks (or a new project) from a template: template, project, parent, key
cmd-attachment-add = Attach a file to a task: data (base64) or path, name, mime
cmd-attachment-list = Attachments of a task
cmd-attachment-get = Download an attachment: as base64 or saved to path
cmd-attachment-remove = Remove an attachment
cmd-task-relate = Relate two tasks: kind relates or duplicates (the task duplicates other and is cancelled)
cmd-task-unrelate = Remove a relation between two tasks
param-base = Base status: backlog, todo, in_progress, in_review, done or cancelled
param-position = Position in the workflow (1 = first column)
param-options = Comma-separated options (select fields)
param-field = Custom field: key or name
param-value = Value (empty clears it)
param-epic = Epic: name or ID (none removes it)
param-milestone = Milestone: name or ID (none removes it)
param-estimate = Estimate in the project unit: a number (hours, points) or S, M, L, XL
param-estimate_unit = Estimate unit: hours, points or size
param-target = Target date
param-template = Template: name or ID
param-definition = Template JSON: "items" (the elements of a plan.create) and, for projects, "project" (name, description, workflow, fields and unit)
param-other = The other task: key (API-42) or ID
param-data = Content in base64
param-mime = MIME type (otherwise from the extension)
param-attachment = Attachment ID
param-max_attachment_mb = Maximum attachment size in MB (1–1024)
ui-any-epic = Any epic
ui-attach = Attach
ui-attach-drop = You can also drop files onto the window. "Save" puts them in { $home }.
ui-attach-hint = Path of a file
ui-attachments = Attachments
ui-base-status = base status
ui-col-project = Project
ui-duplicated-by = Duplicated by
ui-epic = Epic
ui-epic-close = Close
ui-epic-name-hint = Name of the epic or milestone
ui-epic-progress = { $done } of { $total } done
ui-epic-reopen = Reopen
ui-epic-tasks = Show its tasks
ui-estimate = Estimate
ui-estimate-unit = Estimate unit
ui-fields = Custom fields
ui-lanes-epic = By epic
ui-links = Links
ui-mentioned-by = Mentioned in
ui-mention-hint = Write [[API-42]] in a description or comment to link another task.
ui-milestone = Milestone
ui-new-field-hint = New field
ui-new-stage-hint = New stage
ui-no-epic = No epic
ui-no-epics = No epics or milestones yet
ui-none = None
ui-no-templates = No templates yet: save a task with its subtasks below.
ui-planning = Planning
ui-related = Related
ui-relate-with = Relate to…
ui-save-as = Save
ui-template-apply = Apply template
ui-templates = Templates…
ui-template-save = Save { $task } as a template:
ui-view-epics = Epics and milestones
ui-workflow = Workflow
ui-workflow-fields = Workflow and fields…
ui-workflow-hint = Rename or reorder the columns and add your own stages; each stage hangs from a base status, which is what agents understand.
ui-delete = Delete
field-stage = stage
field-fields = fields
field-estimate = estimate
field-epic_id = epic
field-milestone_id = milestone
field-workflow = workflow
field-estimate_unit = estimate unit
field-target = target date
field-max_attachment_mb = maximum attachment size

# M6: permisos y supervisión
agent-level-read = Read only
agent-level-execute = Execute
agent-level-plan = Plan
agent-level-admin = Administer
error-parse-agent-level = Invalid level `{ $value }`; values: { $expected }
agent-paused = The supervisor paused { $agent }: it can read, but not write until resumed
workspace-read-only = The workspace is read-only for agents; the supervisor will re-enable it
agent-level = { $agent } has level { $level } and { $command } needs { $needed }
agent-out-of-scope = { $agent } cannot write in project { $project } (outside its scope)
wip-full = Column { $column } already holds { $limit } tasks (its limit)
notfound-activity = History entry `{ $entry }` does not exist
revert-conflict = Cannot revert { $entry }: { $fields } changed afterwards
revert-unsupported = Cannot revert a { $action } action on { $entity }
revert-gone = What that entry changed ({ $entity }) no longer exists
idempotency-key-reused = Idempotency key `{ $key }` was already used with { $command }
cmd-agent-pause = Pause an agent: it can read but not write (supervisor only)
cmd-agent-resume = Resume a paused agent (supervisor only)
cmd-activity-revert = Revert a history entry, or its whole command with whole_command=true (supervisor only)
cmd-activity-summary = What happened since an instant: done, in review, escalated, created, findings and expired leases
cmd-system-housekeeping = Periodic system cleanup (old idempotency keys)
param-level = Permission level: read, execute, plan or admin
param-scope = Projects it may write in, comma-separated (empty = all)
param-capabilities = Agent capabilities (labels), comma-separated
param-read_only = Read-only for every agent (true/false)
param-wip = Task limit of the column (0 removes it)
param-wip_reject = When full, reject new tasks (true) or only warn (false)
param-entry = History entry ID
param-whole_command = Revert everything the same command did
param-since = Since when (date and time, e.g. 2026-10-26T08:00:00Z)
param-idempotency_key = Key so that a retry does not duplicate what was created
field-level = level
field-scope = scope
field-capabilities = capabilities
field-paused = paused
ui-agent-capabilities = Capabilities
ui-agent-level = Permission level
ui-agent-scope = Scope
ui-agent-scope-all = (all projects)
ui-agent-token-hint = Set it in the MCP client as TASKCRAFT_AGENT_TOKEN (stdio) or Authorization: Bearer (HTTP). All agents of the interface share it; you can copy it again with "Copy token".
ui-copy = Copy
ui-new-token = New token
ui-notice-question = { $task }: { $agent } asks: { $question }
ui-notice-review = { $task } is ready for review ({ $agent })
ui-notice-review-plain = { $task } is ready for review
ui-notice-question-plain = { $task } asks: { $question }
ui-os-notify = System notifications
ui-pause = Pause
ui-paused = Paused
ui-read-only = Read-only for agents
ui-read-only-on = Agents are read-only
ui-resume = Resume
ui-revert-command = Revert everything that command did
ui-revert-entry = Revert this change
ui-summary = Since your last visit
ui-summary-counts = { $created } tasks created, { $comments } comments, { $findings } findings, { $expired } expired leases
ui-summary-done = Done
ui-summary-escalated = Pending questions
ui-summary-review = In review
ui-summary-since = What happened since your last visit ({ $ago }):
ui-wip-reject = reject

# M7: tiempo y métricas
time-source-timer = Timer
time-source-manual = Manual
error-parse-time-source = Invalid time source `{ $value }`; values: { $expected }
timer-not-running = You have no running timer
cmd-timer-start = Start the timer on a task (stopping any previous one)
cmd-timer-stop = Stop your timer and save the time
cmd-time-log = Log time by hand: task, minutes, date, note
cmd-time-report = Time per actor and per task (automatic from history and logged): project, task, actor, days, from, to
cmd-time-sheet = Timesheet per day, week or month: period, project, actor, days, from, to
cmd-metrics-burndown = Burndown of a project, an epic or all projects: remaining and done at the end of each day
cmd-metrics-cfd = Cumulative flow of a project or all of them: tasks per status at the end of each day
cmd-metrics-agents = Performance per agent: submissions, approved, rejected, average cycle and minutes
cmd-metrics-flow = Flow metrics: lead and cycle time, throughput, handoffs, returns, WIP, per agent and per column: project, epic, cycle, days, from, to
cmd-task-timeline = A task's journey through the columns: each step with who moved it, its owner and its duration
param-minutes = Minutes (1–1440)
param-date = Date
param-note = Note
param-owner = Agent that owns the column (its queue or its workbench); empty or none removes it
param-from = From (date)
param-to = To (date)
param-period = Group by day, week or month
param-days = Minimum age in days, or days back for metrics
field-minutes = minutes
field-running = running
field-note = note
field-source = source
field-started_at = start
ui-burndown = Burndown · { $project }
ui-cfd = Cumulative flow
ui-col-agent = Agent
ui-col-task = Task
ui-loading = Calculating…
ui-metrics-charts = Charts
ui-metrics-days = Last { $n } days
ui-metrics-hours = Timesheets
ui-metrics-no-data = No data in this period
ui-throughput-day = Throughput per day
ui-throughput-week = Throughput per week
ui-flow-done = Done
ui-flow-done-hint = Tasks that moved to done in the period
ui-flow-handoffs = { $n ->
    [one] 1 handoff
   *[other] { $n } handoffs
}
ui-flow-throughput-week = Done / week
ui-flow-throughput-week-hint = Average tasks done per week in the period
ui-flow-lead = Lead time (p50)
ui-flow-lead-hint = From when a task is created until it is done; median (p50) and 85th percentile
ui-flow-cycle = Cycle time (p50)
ui-flow-cycle-hint = From when a task first becomes active until it is done; median (p50) and 85th percentile
ui-flow-p85 = p85 { $time }
ui-flow-returned = Sent back
ui-flow-returned-hint = Share of done tasks that went back at least once to a column they had already been in
ui-flow-returns = { $n ->
    [one] 1 return
   *[other] { $n } returns
}
ui-flow-wip = Average WIP
ui-flow-wip-hint = Daily average of tasks in progress (work in progress)
ui-flow-by-agent = By agent
ui-flow-by-column = By column
ui-flow-worked = Worked
ui-flow-worked-hint = Time with their tasks in progress
ui-flow-queued = Queued
ui-flow-queued-hint = Time tasks waited in their queue columns
ui-flow-returns-received = Returns received
ui-flow-returns-received-hint = Times a task was sent back to one of their columns
ui-flow-returns-sent = Returns sent
ui-flow-returns-sent-hint = Times they sent a task back to a column it had already been in
ui-flow-column = Column
ui-flow-owner = Owner
ui-flow-tasks-now = Tasks now
ui-flow-avg-time = Avg time
ui-flow-avg-time-hint = Average time of each stay of a task in the column
ui-journey = Journey
ui-journey-now = now
ui-journey-by = by { $who }
ui-journey-done = Completed on { $date }
ui-journey-cancelled = Cancelled on { $date }
ui-no-time = No time recorded in this period
ui-perf-time = Time
ui-period = Period
ui-period-day = By day
ui-period-month = By month
ui-period-week = By week
ui-refresh = Refresh
ui-remaining = Remaining
ui-time = Time
ui-time-log = Log
ui-time-logged = Logged
ui-timer-elsewhere = Your timer is running on { $task }
ui-timer-running = Timer running
ui-timer-start = Start timer
ui-timer-stop = Stop timer
ui-total = Total
ui-view-metrics = Metrics

## M8 · Views (RF-VIS-05/08/10/12/14, D12)
cmd-view-save = Save a named view: name, filter (D12 language), layout, columns, project
cmd-view-list = Saved views
cmd-view-get = A saved view and its tasks
cmd-view-delete = Delete a saved view
cmd-today-mark = Mark a task "for today"
cmd-today-unmark = Remove the "for today" mark
cmd-today-list = Today: overdue, due today and marked tasks
param-filter = Filter: status:todo priority>=high -label:docs due<=today+3d (AND by default, OR, NOT, parentheses)
param-view = Saved view: name or ID
param-layout = How to show it: list, board, table, today or gantt
param-columns = Table columns, comma separated
notfound-view = There is no view `{ $view }`
view-name-empty = The view needs a name
rule-filter = Malformed filter at position { $position }: expected { $expected }
ui-col-epic = Epic
ui-col-estimate = Estimate
ui-col-milestone = Milestone
ui-col-updated = Updated
ui-columns = Columns
ui-filter-help = Free text or filter: status:todo priority>=high -label:docs due<=today+3d (OR, NOT and parentheses)
ui-gantt-empty = No task has a start or due date
ui-gantt-milestones = Milestones
ui-gantt-undated = { $count } tasks without dates are not on the timeline
ui-shortcut-activity = Show or hide the activity
ui-shortcut-clear = Remove the shortcut
ui-shortcut-conflict = Some shortcuts are repeated: only one of them will work
ui-shortcut-mark-today = Mark or unmark the selected task "for today"
ui-shortcut-press = Press a combination… (Esc cancels)
ui-shortcut-reset = Restore defaults
ui-shortcut-save-view = Save the view
ui-shortcut-search = Go to search
ui-today-due = Due today
ui-today-empty = Nothing for today: no overdue, due or marked tasks
ui-today-mark = For today
ui-today-marked = Picked for today
ui-today-overdue = Overdue
ui-today-unmark = Remove from today
ui-view-delete = Delete this saved view
ui-view-gantt = Gantt
ui-view-name = View name
ui-view-save = Save view
ui-view-table = Table
ui-view-today = Today
ui-views-empty = No saved views yet
ui-views-none = Saved views
ui-zoom = Zoom

## M9 · Integrations and data (RF-INT-02/03, RF-ALM-06/08, RF-EXP-03b/06/07)
cmd-workspace-backup = .taskcraft backup in the workspace folder (or dir), keeping the last backup_keep
cmd-workspace-export_markdown = Export to Markdown: one .md per task with YAML frontmatter, in folders per project: dir, projects
cmd-task-export_csv = Export the tasks of a view or a filter to CSV: path, view, filter, columns
cmd-automation-create = Create an automation (supervisor only): name, trigger, condition (filter), command, args (JSON with {"{"}task{"}"}, {"{"}parent{"}"}, {"{"}project{"}"}), or preset=parent_review
cmd-automation-list = Automations
cmd-automation-update = Enable, disable or change the condition of an automation
cmd-automation-delete = Delete an automation
cmd-task-review_parent = If all subtasks are closed, move the parent to review
cmd-webhook-create = Create a webhook (supervisor only): url and triggers; returns the signing secret only once
cmd-webhook-list = Webhooks (without secrets)
cmd-webhook-delete = Delete a webhook and its queue
cmd-webhook-deliveries = Latest webhook deliveries and their status
cmd-webhook-record = Record the result of a delivery (used by the daemon)
param-dir = Destination folder
param-auto = Only when due according to backup_hours (used by the daemon)
param-backup_hours = Hours between automatic backups (0 = off)
param-backup_keep = Backups to keep
param-trigger = Event: created, status_changed, completed, assigned, commented or escalated
param-triggers = Events to send, comma separated (empty = all)
param-condition = Filter (D12) the task must match
param-command = Command to run (registry ID)
param-args = Arguments as JSON; {"{"}task{"}"}, {"{"}parent{"}"} and {"{"}project{"}"} are replaced
param-preset = Ready-made rule: parent_review (when all children close, the parent goes to review)
param-automation = Automation: name or ID
param-enabled = Enabled (true/false)
param-url = Destination URL (http or https)
param-webhook = Webhook: ID or URL
param-delivery = Delivery ID
param-ok = Whether the delivery succeeded
param-error = Why it failed
arg-json = Invalid JSON: { $detail }
automation-command = There is no command `{ $command }`
webhook-url = The URL must start with http:// or https://
notfound-automation = There is no automation `{ $automation }`
notfound-webhook = There is no webhook `{ $webhook }`
notfound-delivery = There is no delivery `{ $delivery }`
error-parse-trigger = Unknown event `{ $value }`; valid: { $expected }
error-parse-delivery-status = Unknown delivery status `{ $value }`; valid: { $expected }
cli-workspace-help = Workspace by name (in the data directory); --db takes precedence
cli-workspaces-help = List the workspaces (* = current)
ui-backup-now = Back up now
ui-import-merge = Merge (by ULID)
ui-import-updates = { $projects } projects and { $tasks } tasks are updated
ui-workspaces = Workspaces…
ui-workspace-new = Open or create another workspace
ui-workspace-open = Open
ui-workspace-hint = It opens in a new window; each workspace has its own daemon and file.
cli-migrate-help = Copy a workspace to another backend (SQLite path or postgres://…); the destination must be empty
cli-migrate-from-help = Source: workspace.db path or postgres://… string
cli-migrate-to-help = Empty destination: workspace.db path or postgres://… string
cli-migrate-busy = The destination { $db } has a running daemon: stop it before migrating
cli-migrate-done = Migrated { $tasks } tasks to { $to }

## M10 · Web app (D14)
cmd-workspace-web_token = Issue the supervisor's web token (shown once; revoke=true cancels it)
param-revoke = Cancel the token instead of issuing a new one
web-token-home = This workspace has no folder to keep the web token in
cli-serve-web-help = Serve the web app from this folder (cargo xtask web leaves it in dist/web)
cli-serve-tls-cert-help = TLS certificate in PEM (required outside 127.0.0.1)
cli-serve-tls-key-help = TLS private key in PEM
web-tasks = Tasks
web-activity = Activity
web-logout = Sign out
web-login = Sign in
web-login-hint = Paste the supervisor's web token (taskcraft-cli workspace web-token) or an agent token
web-remember = Remember in this browser
web-review-empty = Nothing is waiting for review and there are no pending questions
web-question = Pending question for the supervisor
web-answer = Answer
web-approve = Approve
web-reject = Reject
web-reject-reason = Reason for rejecting
web-apply = Apply
web-pause = Pause
web-resume = Resume
web-activity-line = { $at } · { $actor } · { $action } · { $target }

## M11 · Interfaces and relay (D15)
cmd-interface-register = Register an interface (LLM client: Claude Code, Cursor…) with its token (supervisor only); then add agents to it: name, kind, description, color, level, scope, max_agents
cmd-interface-list = Interfaces and their agents
cmd-interface-get = An interface and its agents
cmd-interface-update = Change an interface's name, color, kind, description, maximum level, scope or maximum agents
cmd-interface-pause = Pause an interface: its agents can read but not write
cmd-interface-resume = Resume a paused interface
cmd-interface-disable = Disable an interface (its token stops working)
cmd-interface-enable = Re-enable an interface
cmd-interface-revoke = Revoke an interface for good
cmd-interface-rotate_token = Issue a new token for an interface and invalidate the previous one
cmd-agent-create = Create an agent in your interface (no token): name, model, description (its function: what it does and when to hand it work), capabilities, level
cmd-agent-mine = Who you are and which agents of your interface you can use
cmd-task-add_assignee = Add an agent to a task; it can have as many as needed: task, agent
cmd-task-remove_assignee = Remove an agent from a task: task, agent
param-interface = Interface: name or ID
param-max_agents = Maximum number of agents of the interface
param-comment = Comment (required when rejecting)
param-as_agent = Act as this agent of your interface for this call only
notfound-interface = There is no interface `{ $interface }`
interface-name-taken = An interface named { $name } already exists
interface-needs-agent = Interface { $interface } can only read and create agents: pick one with agent_use (or as_agent) to write
interface-only = Only an interface (or one of its agents) can do this
interface-max-agents = Interface { $interface } already has { $max } agents
agent-other-interface = { $agent } is not an agent of your interface
agent-no-interface = { $agent } does not belong to any interface
task-other-assignees = { $task } belongs to { $agents }: look for another one with task_next
ui-interface-register = New interface
ui-interface-kind = Client type
ui-interface-level = Maximum level of its agents
ui-interface-hint = The interface is the LLM client (Claude Code, Cursor…): it holds the only token. Add its agents here, or it creates them over MCP with agent_create.
ui-agent-add = Add agent
ui-no-interface = No interface
ui-assignees = Assignees
ui-pick-agent = Pick an agent
ui-agent-function = Function
ui-agent-function-hint = What this agent does and when to hand it work (e.g. "Reviews code and tests")
ui-assign-agents = Assign agents…
ui-column-no-owner = No owner
ui-column-owner-hint = Agent that owns the column: whatever enters it gets assigned to that agent
ui-add-column = Column
ui-add-column-hint = Add a column to the project workflow, including an agent's queue or workbench

# Extras M12: cost per agent (RF-HIS-06)
cmd-cost-report = Report the tokens and cost of your work on a task: task, tokens_in, tokens_out, amount, currency, model, note
cmd-cost-list = Reported costs (a task includes its subtasks): task, epic, project, actor, from, to
cmd-cost-summary = Total cost, grouped by agent, model, task or day: task, epic, project, actor, from, to, group
cost-empty = The report has no tokens and no amount: give tokens_in, tokens_out or amount
param-tokens_in = Input tokens consumed
param-tokens_out = Output tokens generated
param-amount = Amount, with up to 6 decimals (0.42)
param-currency = Currency: a three-letter code (USD by default)
param-group = Group by agent, model, task or day

# Extras M12: git integration (RF-INT-04, D16)
cmd-git-link = Link a task to a branch, a commit or a PR: task, kind (branch, commit or pr), ref, title, url
cmd-git-unlink = Remove a git link: task, kind, ref (a commit accepts its short SHA)
cmd-git-links = Git links of a task, a project or everything: task, project, kind
cmd-git-ingest = Record a commit: links the tasks it mentions and moves to review the ones it closes (fixes API-42): sha, message, branch, remote
error-parse-git-kind = Invalid link kind `{ $value }`; values: { $expected }
git-kind-branch = Branch
git-kind-commit = Commit
git-kind-pr = PR
git-bad-sha = Invalid SHA `{ $value }`: the commit's 40 (or 64) hexadecimal characters are needed
param-ref = Reference: branch name, commit SHA or PR URL
param-sha = Full commit SHA
param-message = Full commit message
param-branch = Branch the commit was made on
param-remote = Remote URL (origin), to link the commit on the web
cli-git-repo-help = Git repository folder (the current one by default)
cli-git-hook-help = Git hook that links every commit to its tasks
cli-git-hook-install-help = Install the post-commit hook in the repository
cli-git-hook-uninstall-help = Remove TaskCraft's post-commit hook
cli-git-scan-help = Read the repository's commits and link them to their tasks (fixes API-42 moves it to review)
cli-git-since-help = Commits from this revision up to HEAD
cli-git-last-help = The last N commits (20 by default)
cli-git-quiet-help = No summary (for the hook)
cli-git-not-repo = { $repo } is not a git repository: { $detail }
cli-git-hook-installed = Hook installed at { $path }
cli-git-hook-removed = TaskCraft hook removed
cli-git-hook-missing = There is no TaskCraft hook to remove
cli-git-hook-exists = There is already another hook at { $path }; add the line `taskcraft-cli git scan --last 1 --quiet` by hand
cli-git-hook-db-env = The workspace is a connection string: the hook will read it from TASKCRAFT_DB, which is not stored in the repository
cli-git-scanned = { $commits } commits read. Linked: { $linked }. To review: { $moved }
ui-git = Git
ui-git-hint = Commits that mention the task are linked automatically with `taskcraft-cli git hook install`.

# Extras M13: recurring tasks (RF-MOD-18, D17)
cmd-task-set_recurrence = Make a task repeat when it is done: task, every (day, week, month, year or 2w), interval, on (mon,thu), day (of the month), or after (3d after finishing it)
cmd-task-clear_recurrence = Stop a task from repeating: task
rule-recurrence = Invalid repeat rule `{ $value }`: use every=week on=mon,thu, every=month day=1, every=2d or after=3d
error-parse-repeat-unit = Invalid unit `{ $value }`; values: { $expected }
repeat-unit-day = day
repeat-unit-week = week
repeat-unit-month = month
repeat-unit-year = year
recurrence-every-day = { $n ->
    [one] Every day
   *[other] Every { $n } days
}
recurrence-every-week = { $n ->
    [one] Every week
   *[other] Every { $n } weeks
}
recurrence-every-month = { $n ->
    [one] Every month
   *[other] Every { $n } months
}
recurrence-every-year = { $n ->
    [one] Every year
   *[other] Every { $n } years
}
recurrence-on-days = { $base } ({ $days })
recurrence-on-day = { $base }, on day { $day }
recurrence-after = { $n ->
    [one] 1 day after finishing it
   *[other] { $n } days after finishing it
}
weekday-1 = Mon
weekday-2 = Tue
weekday-3 = Wed
weekday-4 = Thu
weekday-5 = Fri
weekday-6 = Sat
weekday-7 = Sun
param-every = How often: day, week, month, year, or with a number (2w = every 2 weeks)
param-interval = Every how many units (1 by default)
param-on = Weekdays: mon,thu
param-day = Day of the month (1–31; in shorter months, the last one)
param-after = Days after finishing it: 3d, 2w
ui-recurrence = Repeats
ui-recurrence-none = Does not repeat
ui-recurrence-every = Every…
ui-recurrence-after = After finishing
ui-recurrence-month-day = Day of month
ui-recurrence-month-day-hint = 0 = the same day as the due date; 31 = the last day of the month.
ui-recurrence-days-after = days after finishing it
ui-recurrence-hint = When it is done, the next one is created with its new date.

# Extras M13: stale tasks (RF-VIS-16)
cmd-task-stale = Pending tasks with no changes for weeks: project, weeks (the workspace's by default)
cmd-task-someday = Move a task to the Backlog's "Someday" stage (created if missing): task
param-weeks = Weeks
param-stale_weeks = Weeks without changes after which a pending task is stale (1–520; 4 by default)
param-urgent_days = Days before the due date in which a task is urgent in the matrix (1–365; 3 by default)
stage-someday = Someday
ui-stale-hint = No changes for { $weeks } weeks: move it to "Someday"? (context menu)
ui-someday-move = Move to "Someday"

# Extras M13: graph, critical path and matrix (RF-REL-05, RF-REL-06, RF-VIS-15)
cmd-project-graph = A project's dependency graph: nodes with their layer and edges, with the critical path marked; format=dot for Graphviz: project, closed, format
cmd-project-critical_path = Critical path: the heaviest chain of open dependencies (by estimate, days or 1 per task), which sets when the project can finish: project
cmd-task-matrix = Eisenhower matrix: open tasks by quadrant (important = high or urgent priority; urgent = due soon): project
param-closed = Also include closed tasks
param-format = Format: json or dot
quadrant-do = Do now
quadrant-schedule = Schedule
quadrant-delegate = Delegate
quadrant-drop = Drop
ui-view-matrix = Matrix
ui-view-graph = Graph
ui-matrix-hint = Important = high or urgent priority. Urgent = due in { $days } days or less. Drag between rows to change the priority.
ui-graph-closed = Show closed
ui-graph-loose = No dependencies
ui-graph-empty = There are no dependencies between these tasks. Create them with "Blocked by" in the detail or with task.link.
ui-graph-critical = Critical path (in red): { $n } tasks, weight { $total } (estimate, days or 1 per task).

# Extras M13: cycles or sprints (RF-INT-05)
cmd-cycle-create = Create a cycle (sprint) in a project; by default it starts after the last one, lasts the same (14 days for the first) and is numbered automatically: project, name, start, end, days
cmd-cycle-list = A project's cycles with their state and progress: project, closed
cmd-cycle-get = A cycle with its tasks; accepts its name, current or next: cycle, project
cmd-cycle-update = Change a cycle's name or dates: cycle, project, name, start, end
cmd-cycle-close = Close a cycle: what is still open and not planned in another open cycle also moves to the next one (created if missing): cycle, project
cmd-cycle-delete = Delete a cycle; its tasks keep their other cycles: cycle, project
cmd-cycle-rollover = Close overdue cycles and carry over what is open (the daemon does it daily)
cmd-task-set_cycle = Plan a task in a single cycle, replacing the ones it had (name, current, next or none): task, cycle
cmd-task-add_cycle = Also plan a task in another open cycle, keeping the ones it has: task, cycle
cmd-task-remove_cycle = Take a task out of a cycle (open or closed): task, cycle
notfound-cycle = There is no cycle `{ $cycle }`
cycle-bad-dates = Invalid cycle dates: { $start } to { $end } (the end cannot be before the start or last more than a year)
cycle-overlap = The cycle overlaps { $other }, which is still open
cycle-name-taken = There is already a cycle called { $name } in the project
cycle-closed = Cycle { $cycle } is already closed
cycle-other-project = The cycle belongs to another project
param-cycle = Cycle: name, current, next or none
param-end = Last day (included)
ui-cycle = Cycle
ui-cycle-new = New cycle
ui-cycles-empty = No open cycles. "New cycle" suggests the next one, as long as the previous one (14 days for the first).
ui-cycle-tasks = Show tasks
ui-cycle-close = Close cycle
ui-cycle-close-hint = What is still open moves to the next cycle (created if missing). The daemon closes overdue ones by itself.
ui-cycle-planned = Planned
ui-cycle-active = Active
ui-cycle-overdue = Overdue
ui-cycle-closed = Closed
ui-view-cycles = Cycles / sprints
ui-cycles-hint = A cycle (or sprint) is a period with a start and an end. When you close it, whatever is still open moves to the next one.
ui-cycle-edit = Edit cycle
ui-cycle-delete = Delete cycle
ui-cycle-delete-confirm = Delete "{ $cycle }"? Its tasks are kept, without a cycle.
ui-cycle-name = Name
ui-cycle-start = Start
ui-cycle-end = End (included)
ui-cycle-days = { $days ->
    [one] 1 day
   *[other] { $days } days
}
ui-cycles-closed = Closed ({ $count })
ui-any-cycle = Any cycle
ui-project = Project
ui-sidebar-tasks = Tasks
ui-sidebar-views = Views
ui-sidebar-epics = Epics
ui-sidebar-milestones = Milestones
ui-sidebar-no-cycles = No open cycles
ui-cycles = Sprints
ui-cycles-plan = Plan in sprints…
ui-saved-views = Saved views
ui-archived-count = Archived ({ $count })
ui-epic-new = New epic or milestone…

# Extras M15: Mermaid diagrams (RF-MOD-19, D21)
cmd-diagram-create = Create a Mermaid diagram in a project; it gets the key API-D3 and is cited in descriptions and comments with [[API-D3]] (link) or ![[API-D3]] alone on a line (embedded). A source that cannot be drawn is saved anyway and returns render_error: project, title, source, description
cmd-diagram-list = Diagrams of a project (or of all), without the source: project
cmd-diagram-get = A diagram with its source, render_error and the tasks that cite it; svg=true adds the rendered SVG (theme light or dark): diagram, project, svg, theme
cmd-diagram-update = Change the title, source or description of a diagram: diagram, project, title, source, description
cmd-diagram-delete = Delete a diagram; its number is not reused and remaining citations point to nothing: diagram, project
param-diagram = Diagram: key (API-D3) or ID; with project, also its number (D3)
param-source = Mermaid source (flowchart, sequenceDiagram, classDiagram, stateDiagram, erDiagram, gantt, mindmap…)
param-svg = true to include the rendered SVG
param-theme = SVG theme: light or dark
notfound-diagram = There is no diagram `{ $diagram }`
arg-bad-diagram-ref = `{ $value }` is neither a diagram key (API-D3) nor an ID
arg-too-large = Too large: at most { $max } bytes
diagram-error-empty = The diagram is empty
diagram-error-too-large = The source is larger than { $max } bytes
diagram-error-syntax = Cannot be drawn: { $detail }
diagram-error-render = Drawing failed: { $detail }
ui-view-diagrams = Diagrams
ui-diagrams-hint = Mermaid diagrams of the project. A task cites them with [[API-D3]] (link) or embeds them with ![[API-D3]] alone on a line of its description or of a comment.
ui-diagrams-empty = No diagrams
ui-diagrams-none = Pick a diagram or create a new one.
ui-diagram-new = New diagram
ui-diagram-new-in = New diagram in { $project }
ui-diagram-edit = Edit diagram
ui-diagram-delete = Delete diagram
ui-diagram-delete-confirm = Delete { $diagram }? Its number is not reused and remaining citations will show nothing.
ui-diagram-copy = Copy citation
ui-diagram-copy-hint = Copies ![[key]] to paste it into a task description
ui-diagram-title = Title
ui-diagram-description = Description (Markdown)
ui-diagram-source = Mermaid source
ui-diagram-preview = Preview
ui-diagram-cited-by = Cited in ({ $count })
ui-diagram-not-cited = No task cites it yet. Write [[{ $key }]] or ![[{ $key }]] in a description.
ui-diagram-embed = Embed diagram…
ui-diagram-cite-hint = [[API-D3]] links a diagram; ![[API-D3]] alone on a line shows it drawn.

# D22: interfaces first and copyable token (RF-ACT-07b)
rule-interface-name = Invalid interface name `{ $name }`: 1–60 characters; letters, digits, spaces, dot, dash or underscore, starting with a letter or digit
cmd-interface-token = The stored token of an interface, to copy it (supervisor only)
interface-token-unknown = Interface `{ $interface }` predates stored tokens: issue a new one to be able to copy it
ui-interfaces-first = First register an interface: the LLM client (Claude Code, Cursor…) that connects with a token. Then add its agents inside it, each with its function.
ui-interface-edit = Edit interface
ui-interface-name = Name
ui-interface-name-rule = 1–60 characters: letters, digits, spaces, dot, dash or underscore
ui-interface-agents = { $n }/{ $max } agents
ui-interface-no-agents = No agents yet: "Add agent".
ui-interface-full = The interface already has its maximum of agents
ui-interface-token = Interface token
ui-interface-token-of = Token of { $interface }:
ui-copy-token = Copy token
ui-copy-token-hint = Copies the interface token to the clipboard
ui-token-copied = Token of { $interface } copied
ui-confirm-new-token = Issue a new token for { $interface }? The current one stops working for all its agents.
ui-confirm-interface-revoke = Revoke { $interface }? Its token stops working and its agents stop working. This cannot be undone.
ui-no-interface-hint = Agents from before interfaces: use Edit to move them into one.
ui-agent-name-rule = 1–40 characters a–z, 0–9, dot, dash or underscore
ui-agent-no-token-hint = The agent has no token: it connects with its interface's.

# Menu bar by topic (RF-VIS-17)
ui-menu-agents = Agents
ui-menu-preferences = Preferences
ui-menu-pick-project = Pick a project in the sidebar
ui-menu-search = Search
ui-menu-interfaces-agents = Interfaces and agents
ui-about = About TaskCraft
ui-about-text = Task planner for several AI agents under human supervision. Everything is local: the app, the CLI and the MCP server work on the same workspace.
ui-agent-interface = Interface
ui-agent-model = Model

# D23: data management (RF-ALM-09)
cmd-project-delete = Permanently delete a project with all its tasks, comments, cycles, epics, diagrams, attachments and history (supervisor only). A .taskcraft copy is saved first: project, confirm (the project key), dir
cmd-workspace-wipe = Empty the workspace: projects, tasks, labels, agents, interfaces, templates and history; only its name and settings remain (supervisor only). A .taskcraft copy is saved first: confirm (the workspace name), dir
cmd-activity-prune = Delete the history before a date (supervisor only). A .taskcraft copy is saved first: before, dir
param-confirm = Confirmation: repeat the project key or the workspace name
param-backup = true to save a .taskcraft copy first
data-confirm-mismatch = The confirmation does not match: type `{ $expected }`
data-prune-future = `{ $before }` is in the future: only past history can be deleted
ui-menu-data = Data
ui-delete-data = Delete data
ui-delete-trash = Empty the trash
ui-delete-history = Old history
ui-delete-project = A whole project
ui-delete-workspace = The whole workspace
ui-delete-trash-hint = Permanently deletes the tasks in the trash, with their comments and attachments.
ui-delete-history-hint = Deletes the history entries before the chosen date. Nothing else changes.
ui-delete-project-hint = Permanently deletes the project with all its tasks, comments, cycles, epics, diagrams, attachments and history.
ui-delete-workspace-hint = Empties the workspace: projects, tasks, labels, agents, interfaces, templates and history. Only its name and settings remain.
ui-delete-trash-count = { $count } tasks in the trash
ui-delete-history-before = Delete what is older than
ui-delete-pick-project = Pick the project
ui-delete-type-to-confirm = To confirm, type "{ $text }":
ui-delete-backup-first = A .taskcraft copy is always saved in the backups folder before deleting; restore it with Import.
ui-delete-done = Deleted. Copy saved in { $path }

# TC-1: diagram viewer with zoom
ui-zoom-in = Zoom in
ui-zoom-out = Zoom out
ui-zoom-fit = Fit
ui-zoom-hint = Wheel or two fingers: zoom · drag or Shift+wheel: move · double click: fit
ui-zoom-hint-ctrl = Ctrl+wheel: zoom · drag: move · double click: fit
ui-diagram-open-hint = click to open it with zoom
