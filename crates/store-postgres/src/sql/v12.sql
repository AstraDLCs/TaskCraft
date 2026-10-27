-- v12 (D23): el historial sigue sin poder modificarse, pero se puede podar o
-- borrar al eliminar información, solo con `taskcraft.prune = on` puesto con
-- SET LOCAL en la misma transacción. Igual que la v20 de SQLite.

CREATE OR REPLACE FUNCTION activity_append_only() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' AND current_setting('taskcraft.prune', true) = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'activity is append-only';
END;
$$ LANGUAGE plpgsql;
