-- Per-project opt-in: physical folders by revision (memorias/planos)
-- and optional version folders for modelos (CATEGORIA/v01).
-- Folder scheme: CATEGORIA/RA, CATEGORIA/R0, CATEGORIA/RB.

ALTER TABLE projects ADD COLUMN physical_revision_folders INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS revision_folder_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'previewed', 'applied', 'failed', 'cancelled')),
    backup_absolute_path TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_rev_folder_sessions_project
    ON revision_folder_sessions(project_id);

CREATE TABLE IF NOT EXISTS revision_folder_journal (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES revision_folder_sessions(id) ON DELETE CASCADE,
    op TEXT NOT NULL,
    source_path TEXT,
    dest_path TEXT,
    ok INTEGER NOT NULL DEFAULT 1,
    message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_rev_folder_journal_session
    ON revision_folder_journal(session_id);
