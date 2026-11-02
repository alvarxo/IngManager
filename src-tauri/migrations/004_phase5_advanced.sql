-- Phase 5: FTS5 search, audit project_id, document favorites helper index

ALTER TABLE audit_log ADD COLUMN project_id TEXT;

CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    entity_type UNINDEXED,
    entity_id UNINDEXED,
    project_id UNINDEXED,
    title,
    code,
    body,
    file_name,
    tokenize = 'unicode61'
);

CREATE TABLE IF NOT EXISTS search_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
