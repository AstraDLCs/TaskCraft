-- v5: regla de repetición de la tarea (RF-MOD-18, D17), en JSON. Igual que la
-- v13 de SQLite.

ALTER TABLE task ADD COLUMN recurrence TEXT COLLATE "C";
