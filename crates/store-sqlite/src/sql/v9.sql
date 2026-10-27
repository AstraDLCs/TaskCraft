-- v9: copias automáticas (RF-ALM-06), automatizaciones (RF-INT-03) y
-- webhooks con su cola de envíos (RF-INT-02).

ALTER TABLE workspace ADD COLUMN backup_hours INTEGER NOT NULL DEFAULT 24;
ALTER TABLE workspace ADD COLUMN backup_keep INTEGER NOT NULL DEFAULT 7;

CREATE TABLE automation (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    trigger     TEXT NOT NULL,
    condition   TEXT NOT NULL,
    command     TEXT NOT NULL,
    args        TEXT NOT NULL,
    enabled     INTEGER NOT NULL,
    project_id  TEXT REFERENCES project(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE webhook (
    id          TEXT PRIMARY KEY,
    url         TEXT NOT NULL,
    secret      TEXT NOT NULL,
    triggers    TEXT NOT NULL,
    enabled     INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE webhook_delivery (
    id          TEXT PRIMARY KEY,
    webhook_id  TEXT NOT NULL REFERENCES webhook(id) ON DELETE CASCADE,
    trigger     TEXT NOT NULL,
    payload     TEXT NOT NULL,
    attempts    INTEGER NOT NULL,
    next_at     TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('pending', 'delivered', 'failed')),
    last_error  TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX webhook_delivery_due ON webhook_delivery(status, next_at);
