-- v8: asignados sin rol, sin votos de revisión ni política de cierre (D20).
-- Igual que la v16 de SQLite: cada `{actor, role}` pasa a ser solo el actor,
-- sin repetir y en su orden.

UPDATE task SET assignees = COALESCE(
    (SELECT jsonb_agg(actor ORDER BY first)::text FROM (
        SELECT e.value->>'actor' AS actor, MIN(e.ordinality) AS first
        FROM jsonb_array_elements(task.assignees::jsonb) WITH ORDINALITY e
        GROUP BY e.value->>'actor') a),
    '[]');

ALTER TABLE task DROP COLUMN reviews;
ALTER TABLE project DROP COLUMN close_policy;
