#[cfg(test)]
mod tests {
    use crate::codes::next_project_code;
    use crate::db;
    use tempfile::tempdir;

    #[tokio::test]
    async fn migrations_and_settings_roundtrip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = db::init_pool(&db_path).await.expect("init pool");

        db::set_setting(&pool, "root_path", "/tmp/projects")
            .await
            .unwrap();
        let value = db::get_setting(&pool, "root_path").await.unwrap();
        assert_eq!(value.as_deref(), Some("/tmp/projects"));

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);

        // Ensure core tables exist
        for table in [
            "documents",
            "document_versions",
            "categories",
            "indexed_files",
            "transmissions",
            "document_subcategories",
            "revisions",
        ] {
            let _: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|_| panic!("missing table {table}"));
        }

        // Phase 3 columns present (empty table → None row is fine)
        let _: Option<(Option<String>,)> =
            sqlx::query_as("SELECT subcategory FROM documents LIMIT 1")
                .fetch_optional(&pool)
                .await
                .expect("documents.subcategory");

        assert_eq!(next_project_code(26, &[]).unwrap(), "2601");
    }
}
