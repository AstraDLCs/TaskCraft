-- v11 (D22): el token de cada interfaz se guarda para poder copiarlo. Igual
-- que la v19 de SQLite.

ALTER TABLE interface ADD COLUMN token TEXT COLLATE "C";
