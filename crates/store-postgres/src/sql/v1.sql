-- v1: el modelo completo de TaskCraft en PostgreSQL (D13), equivalente al
-- esquema v9 de SQLite. IDs, fechas y JSON como TEXT; enteros y banderas
-- (0/1) como BIGINT, igual que en SQLite. El texto se compara y ordena byte
-- a byte (COLLATE "C"), como en SQLite.

CREATE TABLE workspace (
    id                 TEXT COLLATE "C" PRIMARY KEY,
    name               TEXT COLLATE "C" NOT NULL,
    created_at         TEXT COLLATE "C" NOT NULL,
    max_depth          BIGINT NOT NULL,
    trash_days         BIGINT NOT NULL,
    lease_minutes      BIGINT NOT NULL DEFAULT 15,
    agent_wip_limit    BIGINT NOT NULL DEFAULT 1,
    auto_archive_days  BIGINT NOT NULL DEFAULT 30,
    max_attachment_mb  BIGINT NOT NULL DEFAULT 10,
    read_only          BIGINT NOT NULL DEFAULT 0,
    backup_hours       BIGINT NOT NULL DEFAULT 24,
    backup_keep        BIGINT NOT NULL DEFAULT 7
);

-- Texto sin acentos ni mayúsculas, para buscar como FTS5 con
-- `remove_diacritics` (RF-VIS-11).
CREATE FUNCTION tc_fold(t TEXT) RETURNS TEXT AS $$
    SELECT lower(translate(t,
        'ÁÀÄÂÃÉÈËÊÍÌÏÎÓÒÖÔÕÚÙÜÛÑÇáàäâãéèëêíìïîóòöôõúùüûñç',
        'AAAAAEEEEIIIIOOOOOUUUUNCaaaaaeeeeiiiiooooouuuunc'))
$$ LANGUAGE sql IMMUTABLE;

-- Contador de escrituras confirmadas: el `change_token` del backend.
CREATE TABLE change_counter (
    id     BIGINT PRIMARY KEY CHECK (id = 1),
    value  BIGINT NOT NULL
);
INSERT INTO change_counter (id, value) VALUES (1, 0);

CREATE TABLE project (
    id             TEXT COLLATE "C" PRIMARY KEY,
    key            TEXT COLLATE "C" NOT NULL UNIQUE,
    name           TEXT COLLATE "C" NOT NULL,
    description    TEXT COLLATE "C" NOT NULL,
    status         TEXT COLLATE "C" NOT NULL,
    close_policy   TEXT COLLATE "C" NOT NULL,
    next_number    BIGINT NOT NULL,
    created_at     TEXT COLLATE "C" NOT NULL,
    updated_at     TEXT COLLATE "C" NOT NULL,
    locked         BIGINT NOT NULL DEFAULT 0,
    workflow       TEXT COLLATE "C" NOT NULL DEFAULT '[]',
    estimate_unit  TEXT COLLATE "C" NOT NULL DEFAULT 'hours',
    fields         TEXT COLLATE "C" NOT NULL DEFAULT '[]'
);

CREATE TABLE epic (
    id           TEXT COLLATE "C" PRIMARY KEY,
    project_id   TEXT COLLATE "C" NOT NULL REFERENCES project(id),
    kind         TEXT COLLATE "C" NOT NULL CHECK (kind IN ('epic', 'milestone')),
    name         TEXT COLLATE "C" NOT NULL,
    description  TEXT COLLATE "C" NOT NULL,
    target       TEXT COLLATE "C",
    status       TEXT COLLATE "C" NOT NULL,
    locked       BIGINT NOT NULL DEFAULT 0,
    created_at   TEXT COLLATE "C" NOT NULL,
    updated_at   TEXT COLLATE "C" NOT NULL
);
CREATE INDEX epic_project ON epic(project_id, kind);

CREATE TABLE task (
    id              TEXT COLLATE "C" PRIMARY KEY,
    project_id      TEXT COLLATE "C" NOT NULL REFERENCES project(id),
    number          BIGINT NOT NULL,
    parent_id       TEXT COLLATE "C" REFERENCES task(id) ON DELETE CASCADE,
    title           TEXT COLLATE "C" NOT NULL,
    description     TEXT COLLATE "C" NOT NULL,
    status          TEXT COLLATE "C" NOT NULL,
    priority        TEXT COLLATE "C" NOT NULL,
    start_at        TEXT COLLATE "C",
    due_at          TEXT COLLATE "C",
    assignee        TEXT COLLATE "C",
    creator         TEXT COLLATE "C" NOT NULL,
    revision        BIGINT NOT NULL,
    created_at      TEXT COLLATE "C" NOT NULL,
    updated_at      TEXT COLLATE "C" NOT NULL,
    archived_at     TEXT COLLATE "C",
    deleted_at      TEXT COLLATE "C",
    claimed_by      TEXT COLLATE "C",
    claimed_at      TEXT COLLATE "C",
    lease_until     TEXT COLLATE "C",
    claim_revision  BIGINT,
    needs_human     TEXT COLLATE "C",
    locked          BIGINT NOT NULL DEFAULT 0,
    stage           TEXT COLLATE "C",
    estimate        BIGINT,
    epic_id         TEXT COLLATE "C" REFERENCES epic(id) ON DELETE SET NULL,
    milestone_id    TEXT COLLATE "C" REFERENCES epic(id) ON DELETE SET NULL,
    fields          TEXT COLLATE "C" NOT NULL DEFAULT '{}',
    UNIQUE (project_id, number)
);
CREATE INDEX task_parent ON task(parent_id);
CREATE INDEX task_project_status ON task(project_id, status);
CREATE INDEX task_claim ON task(claimed_by, status);
CREATE INDEX task_epic ON task(epic_id);
CREATE INDEX task_milestone ON task(milestone_id);

CREATE TABLE task_item (
    id        TEXT COLLATE "C" PRIMARY KEY,
    task_id   TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    kind      TEXT COLLATE "C" NOT NULL CHECK (kind IN ('checklist', 'criterion')),
    position  BIGINT NOT NULL,
    text      TEXT COLLATE "C" NOT NULL,
    done      BIGINT NOT NULL
);
CREATE INDEX task_item_task ON task_item(task_id, kind, position);

CREATE TABLE label (
    id     TEXT COLLATE "C" PRIMARY KEY,
    name   TEXT COLLATE "C" NOT NULL,
    color  TEXT COLLATE "C" NOT NULL
);
CREATE UNIQUE INDEX label_name ON label (lower(name));

CREATE TABLE task_label (
    task_id   TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    label_id  TEXT COLLATE "C" NOT NULL REFERENCES label(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, label_id)
);

CREATE TABLE task_dependency (
    blocker_id  TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    blocked_id  TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);
CREATE INDEX task_dependency_blocked ON task_dependency(blocked_id);

CREATE TABLE agent (
    id            TEXT COLLATE "C" PRIMARY KEY,
    name          TEXT COLLATE "C" NOT NULL UNIQUE,
    model         TEXT COLLATE "C" NOT NULL,
    description   TEXT COLLATE "C" NOT NULL,
    color         TEXT COLLATE "C" NOT NULL,
    status        TEXT COLLATE "C" NOT NULL,
    created_at    TEXT COLLATE "C" NOT NULL,
    updated_at    TEXT COLLATE "C" NOT NULL,
    token_hash    TEXT COLLATE "C",
    level         TEXT COLLATE "C" NOT NULL DEFAULT 'plan',
    scope         TEXT COLLATE "C" NOT NULL DEFAULT '[]',
    capabilities  TEXT COLLATE "C" NOT NULL DEFAULT '[]',
    paused        BIGINT NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX agent_token ON agent(token_hash) WHERE token_hash IS NOT NULL;

CREATE TABLE agent_session (
    id          TEXT COLLATE "C" PRIMARY KEY,
    agent_id    TEXT COLLATE "C" NOT NULL REFERENCES agent(id),
    transport   TEXT COLLATE "C" NOT NULL,
    started_at  TEXT COLLATE "C" NOT NULL,
    last_seen   TEXT COLLATE "C" NOT NULL,
    ended_at    TEXT COLLATE "C"
);
CREATE INDEX agent_session_open ON agent_session(agent_id, ended_at);

-- Sin claves foráneas: el historial sobrevive a la purga de lo que describe.
CREATE TABLE activity (
    id           TEXT COLLATE "C" PRIMARY KEY,
    at           TEXT COLLATE "C" NOT NULL,
    actor        TEXT COLLATE "C" NOT NULL,
    correlation  TEXT COLLATE "C" NOT NULL,
    command      TEXT COLLATE "C" NOT NULL,
    action       TEXT COLLATE "C" NOT NULL,
    entity_kind  TEXT COLLATE "C" NOT NULL,
    entity_id    TEXT COLLATE "C" NOT NULL,
    project_id   TEXT COLLATE "C",
    task_id      TEXT COLLATE "C",
    changes      TEXT COLLATE "C" NOT NULL
);
CREATE INDEX activity_task ON activity(task_id, id);
CREATE INDEX activity_project ON activity(project_id, id);
CREATE INDEX activity_actor ON activity(actor, id);
CREATE INDEX activity_correlation ON activity(correlation);

-- Inmutable también a nivel de base (RF-HIS-01).
CREATE FUNCTION activity_append_only() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'activity is append-only';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER activity_append_only BEFORE UPDATE OR DELETE ON activity
    FOR EACH ROW EXECUTE FUNCTION activity_append_only();

CREATE TABLE comment (
    id          TEXT COLLATE "C" PRIMARY KEY,
    task_id     TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    author      TEXT COLLATE "C" NOT NULL,
    body        TEXT COLLATE "C" NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX comment_task ON comment(task_id, id);

CREATE TABLE inbox_item (
    id          TEXT COLLATE "C" PRIMARY KEY,
    kind        TEXT COLLATE "C" NOT NULL,
    title       TEXT COLLATE "C" NOT NULL,
    body        TEXT COLLATE "C" NOT NULL,
    author      TEXT COLLATE "C" NOT NULL,
    status      TEXT COLLATE "C" NOT NULL,
    task_id     TEXT COLLATE "C",
    created_at  TEXT COLLATE "C" NOT NULL,
    updated_at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX inbox_item_status ON inbox_item(status, id);

CREATE TABLE template (
    id           TEXT COLLATE "C" PRIMARY KEY,
    name         TEXT COLLATE "C" NOT NULL,
    kind         TEXT COLLATE "C" NOT NULL CHECK (kind IN ('task', 'project')),
    description  TEXT COLLATE "C" NOT NULL,
    body         TEXT COLLATE "C" NOT NULL,
    created_at   TEXT COLLATE "C" NOT NULL,
    updated_at   TEXT COLLATE "C" NOT NULL
);
CREATE UNIQUE INDEX template_name ON template (lower(name));

CREATE TABLE attachment_blob (
    sha256  TEXT COLLATE "C" PRIMARY KEY,
    data    BYTEA NOT NULL
);

CREATE TABLE attachment (
    id          TEXT COLLATE "C" PRIMARY KEY,
    task_id     TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    name        TEXT COLLATE "C" NOT NULL,
    mime        TEXT COLLATE "C" NOT NULL,
    size        BIGINT NOT NULL,
    sha256      TEXT COLLATE "C" NOT NULL REFERENCES attachment_blob(sha256),
    author      TEXT COLLATE "C" NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX attachment_task ON attachment(task_id, created_at);

CREATE TABLE task_relation (
    a_id  TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    b_id  TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    kind  TEXT COLLATE "C" NOT NULL CHECK (kind IN ('relates', 'duplicates')),
    PRIMARY KEY (a_id, b_id, kind),
    CHECK (a_id <> b_id)
);
CREATE INDEX task_relation_b ON task_relation(b_id);

CREATE TABLE mention (
    source_id   TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    comment_id  TEXT COLLATE "C" REFERENCES comment(id) ON DELETE CASCADE,
    target_id   TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE
);
CREATE INDEX mention_target ON mention(target_id);
CREATE INDEX mention_source ON mention(source_id, comment_id);

CREATE TABLE idempotency (
    actor       TEXT COLLATE "C" NOT NULL,
    key         TEXT COLLATE "C" NOT NULL,
    command     TEXT COLLATE "C" NOT NULL,
    response    TEXT COLLATE "C" NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL,
    PRIMARY KEY (actor, key)
);
CREATE INDEX idempotency_age ON idempotency(created_at);

CREATE TABLE time_entry (
    id          TEXT COLLATE "C" PRIMARY KEY,
    task_id     TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    actor       TEXT COLLATE "C" NOT NULL,
    source      TEXT COLLATE "C" NOT NULL CHECK (source IN ('timer', 'manual')),
    started_at  TEXT COLLATE "C" NOT NULL,
    minutes     BIGINT NOT NULL,
    running     BIGINT NOT NULL DEFAULT 0,
    note        TEXT COLLATE "C" NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX time_entry_task ON time_entry(task_id, started_at);
CREATE INDEX time_entry_actor ON time_entry(actor, running);

CREATE TABLE saved_view (
    id          TEXT COLLATE "C" PRIMARY KEY,
    name        TEXT COLLATE "C" NOT NULL,
    filter      TEXT COLLATE "C" NOT NULL,
    layout      TEXT COLLATE "C" NOT NULL,
    columns     TEXT COLLATE "C" NOT NULL,
    project_id  TEXT COLLATE "C" REFERENCES project(id) ON DELETE CASCADE,
    created_at  TEXT COLLATE "C" NOT NULL,
    updated_at  TEXT COLLATE "C" NOT NULL
);
CREATE UNIQUE INDEX saved_view_name ON saved_view (lower(name));

CREATE TABLE today_mark (
    task_id  TEXT COLLATE "C" PRIMARY KEY REFERENCES task(id) ON DELETE CASCADE,
    day      TEXT COLLATE "C" NOT NULL
);

CREATE TABLE automation (
    id          TEXT COLLATE "C" PRIMARY KEY,
    name        TEXT COLLATE "C" NOT NULL,
    trigger     TEXT COLLATE "C" NOT NULL,
    condition   TEXT COLLATE "C" NOT NULL,
    command     TEXT COLLATE "C" NOT NULL,
    args        TEXT COLLATE "C" NOT NULL,
    enabled     BIGINT NOT NULL,
    project_id  TEXT COLLATE "C" REFERENCES project(id) ON DELETE CASCADE,
    created_at  TEXT COLLATE "C" NOT NULL,
    updated_at  TEXT COLLATE "C" NOT NULL
);
CREATE UNIQUE INDEX automation_name ON automation (lower(name));

CREATE TABLE webhook (
    id          TEXT COLLATE "C" PRIMARY KEY,
    url         TEXT COLLATE "C" NOT NULL,
    secret      TEXT COLLATE "C" NOT NULL,
    triggers    TEXT COLLATE "C" NOT NULL,
    enabled     BIGINT NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL
);

CREATE TABLE webhook_delivery (
    id          TEXT COLLATE "C" PRIMARY KEY,
    webhook_id  TEXT COLLATE "C" NOT NULL REFERENCES webhook(id) ON DELETE CASCADE,
    trigger     TEXT COLLATE "C" NOT NULL,
    payload     TEXT COLLATE "C" NOT NULL,
    attempts    BIGINT NOT NULL,
    next_at     TEXT COLLATE "C" NOT NULL,
    status      TEXT COLLATE "C" NOT NULL CHECK (status IN ('pending', 'delivered', 'failed')),
    last_error  TEXT COLLATE "C" NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX webhook_delivery_due ON webhook_delivery(status, next_at);
