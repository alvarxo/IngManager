-- Model software subtypes + independent code prefixes (SAP/RVT/IDE/ROB/ETB/MOD).
-- Does NOT rewrite existing business_code values (keep MOD codes).

ALTER TABLE documents ADD COLUMN model_type TEXT;

CREATE INDEX IF NOT EXISTS idx_documents_model_type
    ON documents(project_id, model_type);

-- Backfill model_type for Modelos from principal or typed file extension.
-- Prefer file_role=principal, else known model extensions.
UPDATE documents
SET model_type = (
    SELECT CASE LOWER(COALESCE(f.extension, ''))
        WHEN 'sdb' THEN 'sap2000'
        WHEN 'rvt' THEN 'revit'
        WHEN 'rfa' THEN 'revit'
        WHEN 'idea' THEN 'idea_statica'
        WHEN 'ideastatica' THEN 'idea_statica'
        WHEN 'rtd' THEN 'robot'
        WHEN 'edb' THEN 'etabs'
        WHEN 'e2k' THEN 'etabs'
        ELSE 'other'
    END
    FROM document_files f
    INNER JOIN document_versions v ON v.id = f.document_version_id
    WHERE v.document_id = documents.id
      AND f.is_available = 1
      AND (
            f.file_role = 'principal'
         OR LOWER(COALESCE(f.extension, '')) IN (
                'sdb', 'rvt', 'rfa', 'idea', 'ideastatica', 'rtd', 'edb', 'e2k'
            )
      )
    ORDER BY
        CASE WHEN f.file_role = 'principal' THEN 0 ELSE 1 END,
        v.version_number DESC
    LIMIT 1
)
WHERE model_type IS NULL
  AND category_id IN (SELECT id FROM categories WHERE key = 'modelos');

-- Modelos without any detectable principal stay as other (explicit).
UPDATE documents
SET model_type = 'other'
WHERE model_type IS NULL
  AND category_id IN (SELECT id FROM categories WHERE key = 'modelos');
