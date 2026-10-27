-- v5: modelo ampliado (M5). Flujo por proyecto con etapas (D10, RF-MOD-07),
-- estimación (RF-MOD-14), campos personalizados (RF-MOD-15), épicas e hitos
-- (RF-MOD-13), plantillas (RF-MOD-16), adjuntos (RF-MOD-17), relaciones
-- (RF-REL-03) y menciones (RF-REL-04).

ALTER TABLE project ADD COLUMN workflow TEXT NOT NULL DEFAULT '[]';
ALTER TABLE project ADD COLUMN estimate_unit TEXT NOT NULL DEFAULT 'hours';
ALTER TABLE project ADD COLUMN fields TEXT NOT NULL DEFAULT '[]';

CREATE TABLE epic (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES project(id),
    kind         TEXT NOT NULL CHECK (kind IN ('epic', 'milestone')),
    name         TEXT NOT NULL,
    description  TEXT NOT NULL,
    target       TEXT,
    status       TEXT NOT NULL,
    locked       INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX epic_project ON epic(project_id, kind);

ALTER TABLE task ADD COLUMN stage TEXT;
ALTER TABLE task ADD COLUMN estimate INTEGER;
ALTER TABLE task ADD COLUMN epic_id TEXT REFERENCES epic(id) ON DELETE SET NULL;
ALTER TABLE task ADD COLUMN milestone_id TEXT REFERENCES epic(id) ON DELETE SET NULL;
ALTER TABLE task ADD COLUMN fields TEXT NOT NULL DEFAULT '{}';

ALTER TABLE workspace ADD COLUMN max_attachment_mb INTEGER NOT NULL DEFAULT 10;

CREATE INDEX task_epic ON task(epic_id);
CREATE INDEX task_milestone ON task(milestone_id);

CREATE TABLE template (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    kind         TEXT NOT NULL CHECK (kind IN ('task', 'project')),
    description  TEXT NOT NULL,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

-- El contenido de cada archivo, una vez por hash aunque lo adjunten varias tareas.
CREATE TABLE attachment_blob (
    sha256  TEXT PRIMARY KEY,
    data    BLOB NOT NULL
);

CREATE TABLE attachment (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    mime        TEXT NOT NULL,
    size        INTEGER NOT NULL,
    sha256      TEXT NOT NULL REFERENCES attachment_blob(sha256),
    author      TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX attachment_task ON attachment(task_id, created_at);

CREATE TABLE task_relation (
    a_id  TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    b_id  TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    kind  TEXT NOT NULL CHECK (kind IN ('relates', 'duplicates')),
    PRIMARY KEY (a_id, b_id, kind),
    CHECK (a_id <> b_id)
);
CREATE INDEX task_relation_b ON task_relation(b_id);

-- Menciones [[API-42]]: se recalculan al guardar el texto que las contiene.
CREATE TABLE mention (
    source_id   TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    comment_id  TEXT REFERENCES comment(id) ON DELETE CASCADE,
    target_id   TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE
);
CREATE INDEX mention_target ON mention(target_id);
CREATE INDEX mention_source ON mention(source_id, comment_id);
