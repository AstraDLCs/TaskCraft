-- v2: agentes, sesiones, historial inmutable y comentarios (P3).

CREATE TABLE agent (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    model        TEXT NOT NULL,
    description  TEXT NOT NULL,
    color        TEXT NOT NULL,
    status       TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE agent_session (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL REFERENCES agent(id),
    transport   TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    last_seen   TEXT NOT NULL,
    ended_at    TEXT
);
CREATE INDEX agent_session_open ON agent_session(agent_id, ended_at);

-- Sin claves foráneas: el historial sobrevive a la purga de lo que describe.
CREATE TABLE activity (
    id           TEXT PRIMARY KEY,
    at           TEXT NOT NULL,
    actor        TEXT NOT NULL,
    correlation  TEXT NOT NULL,
    command      TEXT NOT NULL,
    action       TEXT NOT NULL,
    entity_kind  TEXT NOT NULL,
    entity_id    TEXT NOT NULL,
    project_id   TEXT,
    task_id      TEXT,
    changes      TEXT NOT NULL
);
CREATE INDEX activity_task ON activity(task_id, id);
CREATE INDEX activity_project ON activity(project_id, id);
CREATE INDEX activity_actor ON activity(actor, id);

-- Inmutable también a nivel de base (RF-HIS-01).
CREATE TRIGGER activity_append_only_update BEFORE UPDATE ON activity
BEGIN
    SELECT RAISE(ABORT, 'activity is append-only');
END;
CREATE TRIGGER activity_append_only_delete BEFORE DELETE ON activity
BEGIN
    SELECT RAISE(ABORT, 'activity is append-only');
END;

CREATE TABLE comment (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    author      TEXT NOT NULL,
    body        TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX comment_task ON comment(task_id, id);
