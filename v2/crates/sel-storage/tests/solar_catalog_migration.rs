use sqlx::{sqlite::SqlitePoolOptions, Row};
use uuid::Uuid;

#[tokio::test]
async fn product_version_migration_preserves_legacy_rows_and_allows_dated_versions() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory SQLite");
    sqlx::raw_sql(
        "CREATE TABLE products (
            id TEXT PRIMARY KEY,
            sku TEXT NOT NULL UNIQUE,
            product_type TEXT NOT NULL,
            label TEXT NOT NULL,
            price_reference TEXT NOT NULL,
            currency TEXT NOT NULL,
            active INTEGER NOT NULL
        );",
    )
    .execute(&pool)
    .await
    .expect("create pre-version catalog schema");
    let legacy_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO products
         (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, 'LEGACY-KIT', 'SOLAR_KIT', 'Legacy kit', '125.00', 'USD', 1)",
    )
    .bind(&legacy_id)
    .execute(&pool)
    .await
    .expect("insert legacy catalog row");

    let migration = include_str!("../../../migrations/sqlite/0007_solar_product_versions.sql");
    sqlx::raw_sql(migration)
        .execute(&pool)
        .await
        .expect("apply product version migration");
    let legacy = sqlx::query(
        "SELECT id, sku, label, price_reference, active, effective_from, status
         FROM products WHERE id = $1",
    )
    .bind(&legacy_id)
    .fetch_one(&pool)
    .await
    .expect("read preserved legacy product");
    assert_eq!(legacy.try_get::<String, _>("sku").unwrap(), "LEGACY-KIT");
    assert_eq!(legacy.try_get::<String, _>("label").unwrap(), "Legacy kit");
    assert_eq!(
        legacy.try_get::<String, _>("price_reference").unwrap(),
        "125.00"
    );
    assert_eq!(legacy.try_get::<i64, _>("active").unwrap(), 1);
    assert_eq!(
        legacy.try_get::<String, _>("effective_from").unwrap(),
        "1970-01-01"
    );
    assert_eq!(legacy.try_get::<String, _>("status").unwrap(), "APPROVED");

    sqlx::query(
        "INSERT INTO products
         (id, sku, product_type, label, price_reference, currency, active,
          effective_from, status)
         VALUES ($1, 'LEGACY-KIT', 'SOLAR_KIT', 'Next kit version', '140.00', 'USD', 0,
                 '2027-01-01', 'DRAFT')",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&pool)
    .await
    .expect("insert dated version with preserved SKU");
    let version_count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM products WHERE sku = 'LEGACY-KIT'")
            .fetch_one(&pool)
            .await
            .expect("count product versions");
    assert_eq!(version_count, 2);
}
