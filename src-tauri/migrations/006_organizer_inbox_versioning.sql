-- Organizer + global inbox + auto-versioning (incremental, no wipe)

CREATE TABLE IF NOT EXISTS project_folder_mappings (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    category_key TEXT NOT NULL,
    physical_relative_path TEXT NOT NULL,
    mapping_mode TEXT NOT NULL DEFAULT 'virtual'
        CHECK (mapping_mode IN ('virtual', 'physical')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (project_id, category_key)
);

CREATE INDEX IF NOT EXISTS idx_folder_mappings_project ON project_folder_mappings(project_id);

CREATE TABLE IF NOT EXISTS organizer_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    authorized_root_id TEXT REFERENCES authorized_roots(id) ON DELETE SET NULL,
    scope_absolute_path TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'virtual'
        CHECK (mode IN ('virtual', 'physical')),
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'planned', 'applied', 'failed', 'cancelled')),
    backup_absolute_path TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_at TEXT
);

CREATE TABLE IF NOT EXISTS organizer_plan_items (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES organizer_sessions(id) ON DELETE CASCADE,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    project_folder_name TEXT NOT NULL,
    category_key TEXT,
    source_relative_path TEXT NOT NULL,
    target_relative_path TEXT,
    action TEXT NOT NULL
        CHECK (action IN ('map_virtual', 'rename_folder', 'move_contents', 'create_standard', 'skip')),
    confidence REAL NOT NULL DEFAULT 1.0,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'applied', 'skipped', 'failed')),
    detail TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_organizer_plan_session ON organizer_plan_items(session_id);

CREATE TABLE IF NOT EXISTS organizer_journal (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES organizer_sessions(id) ON DELETE CASCADE,
    plan_item_id TEXT REFERENCES organizer_plan_items(id) ON DELETE SET NULL,
    op TEXT NOT NULL,
    source_path TEXT,
    dest_path TEXT,
    ok INTEGER NOT NULL DEFAULT 1,
    message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_organizer_journal_session ON organizer_journal(session_id);

CREATE TABLE IF NOT EXISTS inbox_items (
    id TEXT PRIMARY KEY NOT NULL,
    absolute_path TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    extension TEXT,
    size_bytes INTEGER,
    modified_at TEXT,
    staged_at TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL DEFAULT 'staged'
        CHECK (status IN ('staged', 'assigned', 'removed')),
    notes TEXT
);

CREATE INDEX IF NOT EXISTS idx_inbox_status ON inbox_items(status);

CREATE TABLE IF NOT EXISTS inbox_assignments (
    id TEXT PRIMARY KEY NOT NULL,
    inbox_item_id TEXT NOT NULL REFERENCES inbox_items(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    category_key TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'copy'
        CHECK (mode IN ('copy', 'move')),
    dest_absolute_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_inbox_assign_item ON inbox_assignments(inbox_item_id);

CREATE TABLE IF NOT EXISTS pending_version_events (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    absolute_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    base_stem TEXT NOT NULL,
    version_hint TEXT,
    reason TEXT NOT NULL DEFAULT 'ambiguous'
        CHECK (reason IN ('ambiguous', 'unmatched_pattern', 'manual')),
    candidate_document_ids_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'resolved', 'dismissed')),
    resolved_document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT,
    UNIQUE (project_id, absolute_path)
);

CREATE INDEX IF NOT EXISTS idx_pending_versions_project ON pending_version_events(project_id);
CREATE INDEX IF NOT EXISTS idx_pending_versions_status ON pending_version_events(status);

CREATE TABLE IF NOT EXISTS file_version_tracking (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    document_version_id TEXT REFERENCES document_versions(id) ON DELETE SET NULL,
    absolute_path TEXT NOT NULL,
    base_stem TEXT NOT NULL,
    last_fingerprint TEXT,
    last_size_bytes INTEGER,
    last_modified_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (project_id, absolute_path)
);

CREATE INDEX IF NOT EXISTS idx_file_version_tracking_stem ON file_version_tracking(project_id, base_stem);
