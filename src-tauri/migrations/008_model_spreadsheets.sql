-- Model-associated Excel spreadsheets (calc vs export)

ALTER TABLE document_files ADD COLUMN sheet_kind TEXT;
ALTER TABLE document_files ADD COLUMN version_policy TEXT;

CREATE INDEX IF NOT EXISTS idx_document_files_sheet_kind
    ON document_files(document_version_id, sheet_kind);
