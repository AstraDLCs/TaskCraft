-- v20 (D23): el historial sigue sin poder modificarse, pero se puede podar o
-- borrar al eliminar información. El borrado solo pasa con la «puerta»
-- abierta, y se abre y se cierra dentro de la misma transacción.

CREATE TABLE activity_gate (open INTEGER NOT NULL);

DROP TRIGGER activity_append_only_delete;
CREATE TRIGGER activity_append_only_delete BEFORE DELETE ON activity
WHEN NOT EXISTS (SELECT 1 FROM activity_gate)
BEGIN
    SELECT RAISE(ABORT, 'activity is append-only');
END;
