-- v17: una tarea puede estar en varios ciclos o sprints (RF-INT-05). El ciclo
-- único pasa a ser una lista JSON de ids, como los asignados.

ALTER TABLE task ADD COLUMN cycles TEXT NOT NULL DEFAULT '[]';
UPDATE task SET cycles = json_array(cycle_id) WHERE cycle_id IS NOT NULL;

DROP INDEX task_cycle;
ALTER TABLE task DROP COLUMN cycle_id;
