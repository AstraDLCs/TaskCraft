-- v6: umbrales de las tareas envejecidas (RF-VIS-16) y de la matriz de
-- priorización (RF-VIS-15). Igual que la v14 de SQLite.

ALTER TABLE workspace ADD COLUMN stale_weeks BIGINT NOT NULL DEFAULT 4;
ALTER TABLE workspace ADD COLUMN urgent_days BIGINT NOT NULL DEFAULT 3;
