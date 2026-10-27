-- v14: umbrales de las tareas envejecidas (RF-VIS-16) y de la matriz de
-- priorización (RF-VIS-15).

ALTER TABLE workspace ADD COLUMN stale_weeks INTEGER NOT NULL DEFAULT 4;
ALTER TABLE workspace ADD COLUMN urgent_days INTEGER NOT NULL DEFAULT 3;
