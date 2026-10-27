-- v3: coordinación multiagente (P4): claims con lease, escalado, bloqueo
-- humano, configuración de coordinación y bandeja de hallazgos.

ALTER TABLE task ADD COLUMN claimed_by TEXT;
ALTER TABLE task ADD COLUMN claimed_at TEXT;
ALTER TABLE task ADD COLUMN lease_until TEXT;
-- Revisión que el agente vio en su última escritura: si otro la cambió, conflicto (RF-SUP-04).
ALTER TABLE task ADD COLUMN claim_revision INTEGER;
ALTER TABLE task ADD COLUMN needs_human TEXT;
ALTER TABLE task ADD COLUMN locked INTEGER NOT NULL DEFAULT 0;
CREATE INDEX task_claim ON task(claimed_by, status);

ALTER TABLE project ADD COLUMN locked INTEGER NOT NULL DEFAULT 0;

ALTER TABLE workspace ADD COLUMN lease_minutes INTEGER NOT NULL DEFAULT 15;
ALTER TABLE workspace ADD COLUMN agent_wip_limit INTEGER NOT NULL DEFAULT 1;
ALTER TABLE workspace ADD COLUMN auto_archive_days INTEGER NOT NULL DEFAULT 30;

CREATE TABLE inbox_item (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    author      TEXT NOT NULL,
    status      TEXT NOT NULL,
    task_id     TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX inbox_item_status ON inbox_item(status, id);
