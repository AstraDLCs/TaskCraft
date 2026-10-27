-- v8: vistas guardadas (RF-VIS-12, D12) y tareas marcadas «para hoy» (RF-VIS-08).

CREATE TABLE saved_view (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    filter      TEXT NOT NULL,
    layout      TEXT NOT NULL,
    columns     TEXT NOT NULL,
    project_id  TEXT REFERENCES project(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE today_mark (
    task_id  TEXT PRIMARY KEY REFERENCES task(id) ON DELETE CASCADE,
    day      TEXT NOT NULL
);
