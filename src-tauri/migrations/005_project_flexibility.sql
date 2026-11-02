-- Phase flexibility: multi authorized roots, optional project code,
-- physical path independent of business code.

CREATE TABLE IF NOT EXISTS authorized_roots (
    id TEXT PRIMARY KEY NOT NULL,
    absolute_path TEXT NOT NULL UNIQUE,
    label TEXT,
    is_primary INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_authorized_roots_primary ON authorized_roots(is_primary);

-- Rebuild projects with nullable code + authorized_root_id + import/layout metadata.
CREATE TABLE IF NOT EXISTS projects_new (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    client_id TEXT REFERENCES clients(id) ON DELETE SET NULL,
    client_name TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived', 'cancelled', 'completed')),
    authorized_root_id TEXT NOT NULL REFERENCES authorized_roots(id),
    root_relative_path TEXT NOT NULL,
    import_mode TEXT NOT NULL DEFAULT 'created'
        CHECK (import_mode IN ('created', 'linked', 'copied')),
    layout_mode TEXT NOT NULL DEFAULT 'standard'
        CHECK (layout_mode IN ('standard', 'mapped', 'pending_reorganize')),
    start_date TEXT,
    end_date TEXT,
    is_favorite INTEGER NOT NULL DEFAULT 0,
    last_opened_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Placeholder primary root so FK works before runtime backfill when no root_path yet.
INSERT OR IGNORE INTO authorized_roots (id, absolute_path, label, is_primary, created_at)
SELECT
    'legacy-primary-root',
    COALESCE(
        (SELECT value FROM app_settings WHERE key = 'root_path'),
        '__UNSET_ROOT__'
    ),
    'Raíz principal',
    1,
    datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM authorized_roots WHERE is_primary = 1);

INSERT INTO projects_new (
    id, code, name, description, client_id, client_name, status,
    authorized_root_id, root_relative_path, import_mode, layout_mode,
    start_date, end_date, is_favorite, last_opened_at, created_at, updated_at
)
SELECT
    p.id,
    p.code,
    p.name,
    p.description,
    p.client_id,
    p.client_name,
    p.status,
    (SELECT id FROM authorized_roots WHERE is_primary = 1 LIMIT 1),
    p.root_relative_path,
    'created',
    'standard',
    p.start_date,
    p.end_date,
    p.is_favorite,
    p.last_opened_at,
    p.created_at,
    p.updated_at
FROM projects p;

DROP TABLE projects;
ALTER TABLE projects_new RENAME TO projects;

CREATE INDEX IF NOT EXISTS idx_projects_status ON projects(status);
CREATE INDEX IF NOT EXISTS idx_projects_code ON projects(code);
CREATE INDEX IF NOT EXISTS idx_projects_last_opened ON projects(last_opened_at);
CREATE INDEX IF NOT EXISTS idx_projects_root ON projects(authorized_root_id);

-- indexed_files: scope relative paths per authorized root
CREATE TABLE IF NOT EXISTS indexed_files_new (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    authorized_root_id TEXT REFERENCES authorized_roots(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    absolute_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    extension TEXT,
    size_bytes INTEGER,
    modified_at TEXT,
    is_directory INTEGER NOT NULL DEFAULT 0,
    is_available INTEGER NOT NULL DEFAULT 1,
    category_key TEXT,
    indexed_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (authorized_root_id, relative_path)
);

INSERT INTO indexed_files_new (
    id, project_id, authorized_root_id, relative_path, absolute_path, file_name,
    extension, size_bytes, modified_at, is_directory, is_available, category_key, indexed_at
)
SELECT
    f.id,
    f.project_id,
    (SELECT id FROM authorized_roots WHERE is_primary = 1 LIMIT 1),
    f.relative_path,
    f.absolute_path,
    f.file_name,
    f.extension,
    f.size_bytes,
    f.modified_at,
    f.is_directory,
    f.is_available,
    f.category_key,
    f.indexed_at
FROM indexed_files f;

DROP TABLE indexed_files;
ALTER TABLE indexed_files_new RENAME TO indexed_files;

CREATE INDEX IF NOT EXISTS idx_indexed_files_project ON indexed_files(project_id);
CREATE INDEX IF NOT EXISTS idx_indexed_files_category ON indexed_files(category_key);
CREATE INDEX IF NOT EXISTS idx_indexed_files_abs ON indexed_files(absolute_path);
