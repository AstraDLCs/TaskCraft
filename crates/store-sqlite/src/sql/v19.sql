-- v19 (D22): el token de cada interfaz se guarda para poder copiarlo desde la
-- app. Las interfaces anteriores quedan sin él: hay que generar uno nuevo.

ALTER TABLE interface ADD COLUMN token TEXT;
