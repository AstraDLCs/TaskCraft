-- v11: coste reportado por los agentes (RF-HIS-06). Importes en
-- millonésimas de la moneda para sumar sin redondeos.

CREATE TABLE cost_entry (
    id             TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    actor          TEXT NOT NULL,
    model          TEXT NOT NULL,
    tokens_in      INTEGER NOT NULL,
    tokens_out     INTEGER NOT NULL,
    amount_micros  INTEGER NOT NULL,
    currency       TEXT NOT NULL,
    note           TEXT NOT NULL,
    at             TEXT NOT NULL
);
CREATE INDEX cost_entry_task ON cost_entry(task_id, at);
CREATE INDEX cost_entry_actor ON cost_entry(actor, at);
