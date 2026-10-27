-- v4: daemon y MCP (M2): tokens de agente guardados como hash (D09) y
-- búsqueda de texto completo en título, descripción y comentarios (RF-VIS-11).

ALTER TABLE agent ADD COLUMN token_hash TEXT;
CREATE UNIQUE INDEX agent_token ON agent(token_hash) WHERE token_hash IS NOT NULL;

-- El rowid de cada fila es el de su tarea: las actualizaciones no recorren la tabla.
CREATE VIRTUAL TABLE task_fts USING fts5(
    title,
    description,
    comments,
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO task_fts (rowid, title, description, comments)
SELECT t.rowid, t.title, t.description,
       COALESCE((SELECT group_concat(c.body, ' ') FROM comment c WHERE c.task_id = t.id), '')
FROM task t;

CREATE TRIGGER task_fts_insert AFTER INSERT ON task
BEGIN
    INSERT INTO task_fts (rowid, title, description, comments)
    VALUES (new.rowid, new.title, new.description, '');
END;

CREATE TRIGGER task_fts_update AFTER UPDATE OF title, description ON task
WHEN old.title IS NOT new.title OR old.description IS NOT new.description
BEGIN
    UPDATE task_fts SET title = new.title, description = new.description
    WHERE rowid = new.rowid;
END;

CREATE TRIGGER task_fts_delete AFTER DELETE ON task
BEGIN
    DELETE FROM task_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER task_fts_comment AFTER INSERT ON comment
BEGIN
    UPDATE task_fts SET comments = comments || ' ' || new.body
    WHERE rowid = (SELECT rowid FROM task WHERE id = new.task_id);
END;
