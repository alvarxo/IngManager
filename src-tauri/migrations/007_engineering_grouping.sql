-- Engineering file roles + organizer plan editability (incremental, no wipe)

ALTER TABLE document_files ADD COLUMN file_role TEXT DEFAULT 'standalone';
ALTER TABLE document_files ADD COLUMN engineering_group_key TEXT;

CREATE INDEX IF NOT EXISTS idx_document_files_group
    ON document_files(engineering_group_key);

CREATE TABLE IF NOT EXISTS engineering_groups (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    category_key TEXT NOT NULL,
    group_key TEXT NOT NULL,
    title TEXT NOT NULL,
    principal_file_name TEXT,
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (project_id, category_key, group_key)
);

CREATE INDEX IF NOT EXISTS idx_engineering_groups_project
    ON engineering_groups(project_id, category_key);
