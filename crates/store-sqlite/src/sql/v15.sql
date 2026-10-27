-- v15: ciclos o sprints por proyecto (RF-INT-05) y el ciclo de cada tarea.

CREATE TABLE cycle (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    start_day   TEXT NOT NULL,
    end_day     TEXT NOT NULL,
    closed_at   TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX cycle_project ON cycle(project_id, start_day);

ALTER TABLE task ADD COLUMN cycle_id TEXT REFERENCES cycle(id);
CREATE INDEX task_cycle ON task(cycle_id);
