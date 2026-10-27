-- v13: regla de repetición de la tarea (RF-MOD-18, D17), en JSON.

ALTER TABLE task ADD COLUMN recurrence TEXT;
