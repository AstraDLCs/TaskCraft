-- v1: workspace, proyectos, tareas, checklist/criterios, etiquetas, dependencias.
-- Tipos portables a PostgreSQL (D02): IDs ULID y fechas como TEXT.

CREATE TABLE workspace (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    max_depth   INTEGER NOT NULL,
    trash_days  INTEGER NOT NULL
);

CREATE TABLE project (
    id            TEXT PRIMARY KEY,
    key           TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    description   TEXT NOT NULL,
    status        TEXT NOT NULL,
    close_policy  TEXT NOT NULL,
    next_number   INTEGER NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE task (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES project(id),
    number       INTEGER NOT NULL,
    parent_id    TEXT REFERENCES task(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    description  TEXT NOT NULL,
    status       TEXT NOT NULL,
    priority     TEXT NOT NULL,
    start_at     TEXT,
    due_at       TEXT,
    assignee     TEXT,
    creator      TEXT NOT NULL,
    revision     INTEGER NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    archived_at  TEXT,
    deleted_at   TEXT,
    UNIQUE (project_id, number)
);
CREATE INDEX task_parent ON task(parent_id);
CREATE INDEX task_project_status ON task(project_id, status);

CREATE TABLE task_item (
    id        TEXT PRIMARY KEY,
    task_id   TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    kind      TEXT NOT NULL CHECK (kind IN ('checklist', 'criterion')),
    position  INTEGER NOT NULL,
    text      TEXT NOT NULL,
    done      INTEGER NOT NULL
);
CREATE INDEX task_item_task ON task_item(task_id, kind, position);

CREATE TABLE label (
    id     TEXT PRIMARY KEY,
    name   TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color  TEXT NOT NULL
);

CREATE TABLE task_label (
    task_id   TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    label_id  TEXT NOT NULL REFERENCES label(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, label_id)
);

CREATE TABLE task_dependency (
    blocker_id  TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    blocked_id  TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);
CREATE INDEX task_dependency_blocked ON task_dependency(blocked_id);
