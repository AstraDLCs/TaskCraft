-- v7: ciclos o sprints por proyecto (RF-INT-05) y el ciclo de cada tarea. Igual
-- que la v15 de SQLite.

CREATE TABLE cycle (
    id          TEXT COLLATE "C" PRIMARY KEY,
    project_id  TEXT COLLATE "C" NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    name        TEXT COLLATE "C" NOT NULL,
    start_day   TEXT COLLATE "C" NOT NULL,
    end_day     TEXT COLLATE "C" NOT NULL,
    closed_at   TEXT COLLATE "C",
    created_at  TEXT COLLATE "C" NOT NULL,
    updated_at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX cycle_project ON cycle(project_id, start_day);

ALTER TABLE task ADD COLUMN cycle_id TEXT COLLATE "C" REFERENCES cycle(id);
CREATE INDEX task_cycle ON task(cycle_id);
