use sel_storage::{migrate_postgres, CentralStore, LocalStore};
use sqlx::postgres::PgPoolOptions;
use std::str::FromStr;
use uuid::Uuid;

async fn seed_sqlite_tariffs(store: &LocalStore, approval_ref: &str) {
    for (category, electricity, water) in [
        ("RESIDENTIAL", "0.095", "0.30"),
        ("COMMERCIAL", "0.12", "0.30"),
    ] {
        for (service, unit, price) in [("ELECTRICITY", "KWH", electricity), ("WATER", "M3", water)]
        {
            sqlx::query(
                "INSERT INTO tariff_versions
                 (id, service_code, customer_category, unit, unit_price, currency,
                  effective_from, approval_ref, status)
                 VALUES (?, ?, ?, ?, ?, 'USD', '2026-01-01', ?, 'APPROVED')",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(service)
            .bind(category)
            .bind(unit)
            .bind(price)
            .bind(approval_ref)
            .execute(&store.pool)
            .await
            .expect("seed category-specific approved tariff");
        }
    }
}

#[tokio::test]
async fn sqlite_tariffs_are_selected_for_the_customer_category() {
    let path = std::env::temp_dir().join(format!("sel-category-tariff-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("open local SQLite store");
    seed_sqlite_tariffs(&store, "SEL-APPROVED-2026").await;

    let residential = store
        .load_tariffs("SEL-APPROVED-2026", "USD", "2026-10", " residential ")
        .await
        .expect("load residential tariffs");
    let commercial = store
        .load_tariffs("SEL-APPROVED-2026", "USD", "2026-10", "COMMERCIAL")
        .await
        .expect("load commercial tariffs");

    assert_eq!(
        residential
            .electricity_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.095"
    );
    assert_eq!(
        commercial
            .electricity_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.12"
    );
    assert_eq!(
        residential
            .water_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.3"
    );
    assert_eq!(
        commercial.water_unit_price.unwrap().normalize().to_string(),
        "0.3"
    );
    assert!(store
        .load_tariffs("SEL-APPROVED-2026", "USD", "2026-10", "")
        .await
        .is_err());
    assert!(store
        .load_tariffs("SEL-APPROVED-2026", "USD", "2026-10", "INDUSTRIAL")
        .await
        .expect("an unconfigured category must not borrow another category tariff")
        .electricity_unit_price
        .is_none());
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency,
          effective_from, approval_ref, status)
         VALUES (?, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', '0.10', 'USD',
                 '2026-01-01', 'SEL-APPROVED-2026', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .expect("seed overlapping tariff to verify fail-closed behavior");
    assert!(store
        .load_tariffs("SEL-APPROVED-2026", "USD", "2026-10", "RESIDENTIAL")
        .await
        .is_err());

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn postgres_tariffs_are_selected_for_the_customer_category() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for PostgreSQL integration tests");
    let organization_id = Uuid::new_v4();
    let schema = format!("category_tariffs_{}", Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("connect test database to create isolated schema");
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .expect("create isolated test schema");
    let options = sqlx::postgres::PgConnectOptions::from_str(&database_url)
        .expect("parse test database URL")
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .expect("connect to isolated test schema");
    admin_pool.close().await;
    let store = CentralStore::from_pool(pool, organization_id);
    migrate_postgres(&store.pool).await.expect("run migrations");
    let approval_ref = format!("SEL-TEST-{}", Uuid::new_v4());

    for (category, electricity, water) in [
        ("RESIDENTIAL", "0.095", "0.30"),
        ("COMMERCIAL", "0.12", "0.30"),
    ] {
        for (service, unit, price) in [("ELECTRICITY", "KWH", electricity), ("WATER", "M3", water)]
        {
            sqlx::query(
                "INSERT INTO tariff_versions
                 (id, service_code, customer_category, unit, unit_price, currency,
                  effective_from, approval_ref, status)
                 VALUES ($1, $2, $3, $4, $5::numeric, 'USD', DATE '2026-01-01', $6, 'APPROVED')",
            )
            .bind(Uuid::new_v4())
            .bind(service)
            .bind(category)
            .bind(unit)
            .bind(price)
            .bind(&approval_ref)
            .execute(&store.pool)
            .await
            .expect("seed category-specific approved tariff");
        }
    }

    let residential = store
        .load_tariffs(&approval_ref, "USD", "2026-10", "RESIDENTIAL")
        .await
        .expect("load residential tariffs");
    let commercial = store
        .load_tariffs(&approval_ref, "USD", "2026-10", "COMMERCIAL")
        .await
        .expect("load commercial tariffs");
    let automatic = store
        .load_tariffs("", "USD", "2026-10", "COMMERCIAL")
        .await
        .expect("resolve the unique approved tariff revision automatically");

    assert_eq!(
        residential
            .electricity_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.095"
    );
    assert_eq!(
        commercial
            .electricity_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.12"
    );
    assert_eq!(automatic.revision.as_deref(), Some(approval_ref.as_str()));
    assert_eq!(
        automatic
            .electricity_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.12"
    );
    assert_eq!(
        residential
            .water_unit_price
            .unwrap()
            .normalize()
            .to_string(),
        "0.3"
    );
    assert_eq!(
        commercial.water_unit_price.unwrap().normalize().to_string(),
        "0.3"
    );
    assert!(store
        .load_tariffs(&approval_ref, "USD", "2026-10", "INDUSTRIAL")
        .await
        .expect("unconfigured category has no tariff")
        .electricity_unit_price
        .is_none());
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency,
          effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', 0.10, 'USD',
                 DATE '2026-01-01', $2, 'APPROVED')",
    )
    .bind(Uuid::new_v4())
    .bind(&approval_ref)
    .execute(&store.pool)
    .await
    .expect("seed overlapping tariff to verify fail-closed behavior");
    assert!(store
        .load_tariffs(&approval_ref, "USD", "2026-10", "RESIDENTIAL")
        .await
        .is_err());
    store.pool.close().await;
    let cleanup_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("reconnect to remove isolated test schema");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&cleanup_pool)
        .await
        .expect("remove isolated test schema");
}
