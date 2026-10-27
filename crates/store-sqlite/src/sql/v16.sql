-- v16: asignados sin rol, sin votos de revisión ni política de cierre (D20).
-- Cada `{actor, role}` pasa a ser solo el actor, sin repetir y en su orden.

UPDATE task SET assignees = COALESCE(
    (SELECT json_group_array(actor) FROM (
        SELECT json_extract(e.value, '$.actor') AS actor, MIN(e.key) AS first
        FROM json_each(task.assignees) e
        GROUP BY actor
        ORDER BY first)),
    '[]');

ALTER TABLE task DROP COLUMN reviews;
ALTER TABLE project DROP COLUMN close_policy;
