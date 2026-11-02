-- Phase 4: transmission metadata, snapshots, links

ALTER TABLE transmissions ADD COLUMN medium TEXT;
ALTER TABLE transmissions ADD COLUMN status TEXT NOT NULL DEFAULT 'draft';
ALTER TABLE transmissions ADD COLUMN folder_relative_path TEXT;
ALTER TABLE transmissions ADD COLUMN zip_relative_path TEXT;
ALTER TABLE transmissions ADD COLUMN zip_checksum TEXT;
ALTER TABLE transmissions ADD COLUMN linked_transmission_id TEXT;
ALTER TABLE transmissions ADD COLUMN original_import_relative_path TEXT;
ALTER TABLE transmissions ADD COLUMN extracted_relative_path TEXT;
ALTER TABLE transmissions ADD COLUMN antecedentes_relative_path TEXT;

ALTER TABLE transmission_items ADD COLUMN document_version_id TEXT;
ALTER TABLE transmission_items ADD COLUMN checksum TEXT;
ALTER TABLE transmission_items ADD COLUMN snapshot_relative_path TEXT;
ALTER TABLE transmission_items ADD COLUMN is_available INTEGER NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS transmission_links (
    id TEXT PRIMARY KEY NOT NULL,
    sent_transmission_id TEXT NOT NULL REFERENCES transmissions(id) ON DELETE CASCADE,
    received_transmission_id TEXT NOT NULL REFERENCES transmissions(id) ON DELETE CASCADE,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (sent_transmission_id, received_transmission_id)
);

CREATE INDEX IF NOT EXISTS idx_transmissions_project ON transmissions(project_id);
CREATE INDEX IF NOT EXISTS idx_transmissions_direction ON transmissions(direction);
CREATE INDEX IF NOT EXISTS idx_transmission_items_tx ON transmission_items(transmission_id);
