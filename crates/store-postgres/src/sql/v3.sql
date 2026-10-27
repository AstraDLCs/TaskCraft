-- v3: coste reportado por los agentes (RF-HIS-06). Igual que la v11 de SQLite.
-- Importes en millonésimas de la moneda.

CREATE TABLE cost_entry (
    id             TEXT COLLATE "C" PRIMARY KEY,
    task_id        TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    actor          TEXT COLLATE "C" NOT NULL,
    model          TEXT COLLATE "C" NOT NULL,
    tokens_in      BIGINT NOT NULL,
    tokens_out     BIGINT NOT NULL,
    amount_micros  BIGINT NOT NULL,
    currency       TEXT COLLATE "C" NOT NULL,
    note           TEXT COLLATE "C" NOT NULL,
    at             TEXT COLLATE "C" NOT NULL
);
CREATE INDEX cost_entry_task ON cost_entry(task_id, at);
CREATE INDEX cost_entry_actor ON cost_entry(actor, at);
