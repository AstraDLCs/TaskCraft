-- v10: interfaces con token y agentes sin token (D15, RF-ACT-07); asignados
-- con rol y votos de revisión (RF-COO-13).

CREATE TABLE interface (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    kind         TEXT NOT NULL,
    description  TEXT NOT NULL,
    color        TEXT NOT NULL,
    status       TEXT NOT NULL,
    level        TEXT NOT NULL,
    scope        TEXT NOT NULL,
    max_agents   INTEGER NOT NULL,
    paused       INTEGER NOT NULL,
    token_hash   TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE UNIQUE INDEX interface_token ON interface(token_hash) WHERE token_hash IS NOT NULL;

ALTER TABLE agent ADD COLUMN interface_id TEXT REFERENCES interface(id);

-- Cada agente con token pasa a ser su propia interfaz (mismo ID y nombre):
-- su token sigue valiendo. Nivel, alcance y pausa se quedan en el agente.
INSERT INTO interface (id, name, kind, description, color, status, level, scope, max_agents,
                       paused, token_hash, created_at, updated_at)
SELECT id, name, model, description, color, status, 'admin', '[]', 50, 0, token_hash,
       created_at, updated_at
FROM agent WHERE token_hash IS NOT NULL;
UPDATE agent SET interface_id = id, token_hash = NULL WHERE token_hash IS NOT NULL;

ALTER TABLE task ADD COLUMN assignees TEXT NOT NULL DEFAULT '[]';
ALTER TABLE task ADD COLUMN reviews TEXT NOT NULL DEFAULT '[]';
UPDATE task SET assignees = json_array(json_object('actor', assignee, 'role', 'executor'))
WHERE assignee IS NOT NULL;
