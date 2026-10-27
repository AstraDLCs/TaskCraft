-- v4: vínculos de tareas con ramas, commits y PRs (RF-INT-04, D16). Igual que la
-- v12 de SQLite.

CREATE TABLE git_link (
    id         TEXT COLLATE "C" PRIMARY KEY,
    task_id    TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    kind       TEXT COLLATE "C" NOT NULL CHECK (kind IN ('branch', 'commit', 'pr')),
    reference  TEXT COLLATE "C" NOT NULL,
    title      TEXT COLLATE "C" NOT NULL,
    url        TEXT COLLATE "C",
    actor      TEXT COLLATE "C" NOT NULL,
    at         TEXT COLLATE "C" NOT NULL,
    UNIQUE (task_id, kind, reference)
);
CREATE INDEX git_link_reference ON git_link(kind, reference);
