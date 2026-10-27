-- v7: tiempo registrado a mano y cronómetros (RF-HIS-04). El tiempo
-- automático por actor (RF-HIS-03) se calcula del historial, sin tablas.

CREATE TABLE time_entry (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    actor       TEXT NOT NULL,
    source      TEXT NOT NULL CHECK (source IN ('timer', 'manual')),
    started_at  TEXT NOT NULL,
    minutes     INTEGER NOT NULL,
    running     INTEGER NOT NULL DEFAULT 0,
    note        TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX time_entry_task ON time_entry(task_id, started_at);
CREATE INDEX time_entry_actor ON time_entry(actor, running);
