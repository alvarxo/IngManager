-- Phase 3: document metadata, version snapshots, revision formats

ALTER TABLE documents ADD COLUMN subcategory TEXT;
ALTER TABLE documents ADD COLUMN origin TEXT;
ALTER TABLE documents ADD COLUMN received_at TEXT;
ALTER TABLE documents ADD COLUMN notes TEXT;
ALTER TABLE documents ADD COLUMN metadata_json TEXT;

ALTER TABLE document_versions ADD COLUMN snapshot_relative_path TEXT;
ALTER TABLE document_versions ADD COLUMN registered_fingerprint TEXT;
ALTER TABLE document_versions ADD COLUMN source_relative_path TEXT;
ALTER TABLE document_versions ADD COLUMN is_snapshot INTEGER NOT NULL DEFAULT 0;

ALTER TABLE document_files ADD COLUMN format_label TEXT;
ALTER TABLE document_files ADD COLUMN revision_id TEXT REFERENCES revisions(id) ON DELETE SET NULL;

ALTER TABLE revisions ADD COLUMN document_version_id TEXT REFERENCES document_versions(id) ON DELETE SET NULL;
ALTER TABLE revisions ADD COLUMN status TEXT DEFAULT 'issued';

CREATE TABLE IF NOT EXISTS document_subcategories (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    category_key TEXT NOT NULL,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (project_id, category_key, name)
);

CREATE INDEX IF NOT EXISTS idx_documents_business_code ON documents(business_code);
CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(doc_type);
CREATE INDEX IF NOT EXISTS idx_revisions_document ON revisions(document_id);
