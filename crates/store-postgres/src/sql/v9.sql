-- v9: una tarea puede estar en varios ciclos o sprints (RF-INT-05). Igual que
-- la v17 de SQLite: el ciclo único pasa a ser una lista JSON de ids.

ALTER TABLE task ADD COLUMN cycles TEXT COLLATE "C" NOT NULL DEFAULT '[]';
UPDATE task SET cycles = json_build_array(cycle_id)::text WHERE cycle_id IS NOT NULL;

DROP INDEX task_cycle;
ALTER TABLE task DROP COLUMN cycle_id;
