use sel_storage::migrate_sqlite;
use sqlx::Row;

#[tokio::test]
async fn sqlite_migrations_are_repeatable_and_create_local_sync_tables() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate_sqlite(&pool).await.unwrap();
    migrate_sqlite(&pool).await.unwrap();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(&pool)
            .await
            .unwrap();
    for required in [
        "customers",
        "invoices",
        "command_journal",
        "outbox",
        "sync_inbox",
        "sync_conflicts",
        "sale_lines",
        "invoice_meter_readings",
    ] {
        assert!(
            tables.iter().any(|name| name == required),
            "missing {required}"
        );
    }
    for (table, column) in [
        ("payments", "document_reference"),
        ("payments", "issued_at"),
        ("sales", "document_reference"),
    ] {
        let columns: Vec<String> = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.try_get::<String, _>("name").unwrap())
            .collect();
        assert!(
            columns.iter().any(|name| name == column),
            "missing {table}.{column}"
        );
    }
}

#[test]
fn server_schemas_contain_no_invoice_file_storage_contract() {
    for path in [
        "../../migrations/postgres/0001_init.sql",
        "../../migrations/sqlite/0001_init.sql",
    ] {
        let schema =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
                .unwrap()
                .to_lowercase();
        for forbidden in [
            "invoice_pdf",
            "invoice_file",
            "pdf_blob",
            "document_binary",
            "invoice_attachment",
            "invoice_file_url",
        ] {
            assert!(
                !schema.contains(forbidden),
                "forbidden invoice file token {forbidden} in {path}"
            );
        }
    }
}
