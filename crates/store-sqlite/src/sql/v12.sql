-- v12: vínculos de tareas con ramas, commits y PRs (RF-INT-04, D16).

CREATE TABLE git_link (
    id         TEXT PRIMARY KEY,
    task_id    TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('branch', 'commit', 'pr')),
    reference  TEXT NOT NULL,
    title      TEXT NOT NULL,
    url        TEXT,
    actor      TEXT NOT NULL,
    at         TEXT NOT NULL,
    UNIQUE (task_id, kind, reference)
);
CREATE INDEX git_link_reference ON git_link(kind, reference);
