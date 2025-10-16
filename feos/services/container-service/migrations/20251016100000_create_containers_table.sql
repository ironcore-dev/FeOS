-- SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
-- SPDX-License-Identifier: Apache-2.0

CREATE TABLE IF NOT EXISTS containers (
    container_id TEXT PRIMARY KEY NOT NULL,
    image_uuid TEXT NOT NULL,
    state TEXT NOT NULL,
    pid INTEGER,
    config_blob BLOB NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TRIGGER IF NOT EXISTS trigger_containers_updated_at
AFTER UPDATE ON containers
FOR EACH ROW
BEGIN
    UPDATE containers SET updated_at = CURRENT_TIMESTAMP WHERE container_id = OLD.container_id;
END;
