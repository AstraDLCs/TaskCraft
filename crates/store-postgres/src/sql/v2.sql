-- v2: interfaces con token y agentes sin token (D15, RF-ACT-07); asignados
-- con rol y votos de revisión (RF-COO-13). Igual que la v10 de SQLite.

CREATE TABLE interface (
    id           TEXT COLLATE "C" PRIMARY KEY,
    name         TEXT COLLATE "C" NOT NULL UNIQUE,
    kind         TEXT COLLATE "C" NOT NULL,
    description  TEXT COLLATE "C" NOT NULL,
    color        TEXT COLLATE "C" NOT NULL,
    status       TEXT COLLATE "C" NOT NULL,
    level        TEXT COLLATE "C" NOT NULL,
    scope        TEXT COLLATE "C" NOT NULL,
    max_agents   BIGINT NOT NULL,
    paused       BIGINT NOT NULL,
    token_hash   TEXT COLLATE "C",
    created_at   TEXT COLLATE "C" NOT NULL,
    updated_at   TEXT COLLATE "C" NOT NULL
);
CREATE UNIQUE INDEX interface_token ON interface(token_hash) WHERE token_hash IS NOT NULL;

ALTER TABLE agent ADD COLUMN interface_id TEXT COLLATE "C" REFERENCES interface(id);

INSERT INTO interface (id, name, kind, description, color, status, level, scope, max_agents,
                       paused, token_hash, created_at, updated_at)
SELECT id, name, model, description, color, status, 'admin', '[]', 50, 0, token_hash,
       created_at, updated_at
FROM agent WHERE token_hash IS NOT NULL;
UPDATE agent SET interface_id = id, token_hash = NULL WHERE token_hash IS NOT NULL;

ALTER TABLE task ADD COLUMN assignees TEXT COLLATE "C" NOT NULL DEFAULT '[]';
ALTER TABLE task ADD COLUMN reviews TEXT COLLATE "C" NOT NULL DEFAULT '[]';
UPDATE task SET assignees = json_build_array(json_build_object('actor', assignee, 'role', 'executor'))::text
WHERE assignee IS NOT NULL;
