-- v6: permisos y supervisión (M6). Nivel, alcance, capacidades y pausa de
-- cada agente (D11, RF-ACT-04/05, RF-SUP-08), solo lectura global y claves
-- de idempotencia de las creaciones (RF-MCP-09).

ALTER TABLE agent ADD COLUMN level TEXT NOT NULL DEFAULT 'plan';
ALTER TABLE agent ADD COLUMN scope TEXT NOT NULL DEFAULT '[]';
ALTER TABLE agent ADD COLUMN capabilities TEXT NOT NULL DEFAULT '[]';
ALTER TABLE agent ADD COLUMN paused INTEGER NOT NULL DEFAULT 0;

ALTER TABLE workspace ADD COLUMN read_only INTEGER NOT NULL DEFAULT 0;

-- Respuesta guardada de una creación con clave: un reintento la devuelve
-- sin volver a crear nada.
CREATE TABLE idempotency (
    actor       TEXT NOT NULL,
    key         TEXT NOT NULL,
    command     TEXT NOT NULL,
    response    TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (actor, key)
);
CREATE INDEX idempotency_age ON idempotency(created_at);

-- Revertir un comando entero (RF-SUP-07) busca sus entradas por correlación.
CREATE INDEX activity_correlation ON activity(correlation);
