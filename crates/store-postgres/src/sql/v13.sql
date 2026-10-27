-- v13 (D24): recorrido de cada tarea, paso a paso. Igual que la v21 de SQLite.

CREATE TABLE task_transition (
    id           TEXT COLLATE "C" PRIMARY KEY,
    task_id      TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    project_id   TEXT COLLATE "C" NOT NULL,
    from_status  TEXT COLLATE "C",
    from_stage   TEXT COLLATE "C",
    to_status    TEXT COLLATE "C" NOT NULL,
    to_stage     TEXT COLLATE "C",
    actor        TEXT COLLATE "C" NOT NULL,
    at           TEXT COLLATE "C" NOT NULL
);
CREATE INDEX task_transition_task ON task_transition(task_id, at);
CREATE INDEX task_transition_project ON task_transition(project_id, at);
