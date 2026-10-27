-- v21 (D24): recorrido de cada tarea, paso a paso. Aparte del historial para
-- que las métricas sobrevivan a `activity.prune`; se va con su tarea.

CREATE TABLE task_transition (
    id           TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    project_id   TEXT NOT NULL,
    from_status  TEXT,
    from_stage   TEXT,
    to_status    TEXT NOT NULL,
    to_stage     TEXT,
    actor        TEXT NOT NULL,
    at           TEXT NOT NULL
);
CREATE INDEX task_transition_task ON task_transition(task_id, at);
CREATE INDEX task_transition_project ON task_transition(project_id, at);
