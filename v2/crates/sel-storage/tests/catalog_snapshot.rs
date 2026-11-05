use sel_contracts::{catalog_snapshot_sha256, CatalogSnapshot, CatalogSnapshotPayload};
use sel_storage::LocalStore;
use serde_json::json;
use uuid::Uuid;

fn snapshot(product_status: &str) -> CatalogSnapshot {
    let payload = CatalogSnapshotPayload {
        schema_version: 1,
        products: vec![json!({
            "id": "3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0", "sku": "KIT-1",
            "product_type": "SOLAR_KIT", "label": "Kit test", "price_reference": "100",
            "currency": "USD", "active": product_status == "APPROVED",
            "effective_from": "2026-01-01", "effective_to": null,
            "approval_ref": "SEL-APPROVAL-1", "approved_by": null, "status": product_status
        })],
        tariffs: vec![json!({
            "id": "11000000-0000-4000-8000-000000000001", "service_code": "WATER",
            "customer_category": "RESIDENTIAL", "unit": "M3", "unit_price": "0.30",
            "currency": "USD", "effective_from": "2026-01-01", "effective_to": null,
            "approval_ref": "SEL-TARIFF-1", "approved_by": null, "status": "APPROVED"
        })],
        tax_profiles: vec![json!({
            "id": "11000000-0000-4000-8000-000000000002", "reference_type": "TAX_PROFILE",
            "natural_key": "WATER|SEL|SERVICE", "payload": {"service_code":"WATER",
                "jurisdiction":"SEL", "base":"SERVICE", "rate":"0.16", "source":"SEL"},
            "effective_from": "2026-01-01", "effective_to": null, "approval_ref": "SEL-TAX-1",
            "approved_by": null, "status": "APPROVED"
        })],
        fx_rates: vec![json!({
            "id": "11000000-0000-4000-8000-000000000003", "reference_type": "FX_RATE",
            "natural_key": "USD|CDF", "payload": {"from_currency":"USD", "to_currency":"CDF",
                "value":"2360", "source":"SEL"}, "effective_from": "2026-01-01",
            "effective_to": null, "approval_ref": "SEL-FX-1", "approved_by": null,
            "status": "APPROVED"
        })],
    };
    let sha256 = catalog_snapshot_sha256(&payload);
    CatalogSnapshot { payload, sha256 }
}

#[tokio::test]
async fn catalog_snapshot_is_authenticated_atomic_and_idempotent() {
    let path = std::env::temp_dir().join(format!("sel-v2-catalog-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("open SQLite local store");
    let approved = snapshot("APPROVED");
    store
        .apply_catalog_snapshot(&approved)
        .await
        .expect("apply snapshot");
    let water_unit_price: String = sqlx::query_scalar(
        "SELECT unit_price FROM tariff_versions WHERE service_code = 'WATER' AND unit = 'M3'",
    )
    .fetch_one(&store.pool)
    .await
    .expect("read the approved water tariff");
    assert_eq!(water_unit_price, "0.30");
    store
        .apply_catalog_snapshot(&approved)
        .await
        .expect("idempotent replay");
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM products), (SELECT count(*) FROM tariff_versions),
                (SELECT count(*) FROM tax_profile_versions), (SELECT count(*) FROM fx_rate_versions)",
    )
    .fetch_one(&store.pool)
    .await
    .expect("count cached references");
    assert_eq!(counts, (1, 1, 1, 1));

    let mut corrupted = approved.clone();
    corrupted.payload.products[0]["label"] = json!("tampered");
    assert!(store.apply_catalog_snapshot(&corrupted).await.is_err());
    let stored_label: String = sqlx::query_scalar("SELECT label FROM products WHERE sku = 'KIT-1'")
        .fetch_one(&store.pool)
        .await
        .expect("read cached product");
    assert_eq!(stored_label, "Kit test");

    sqlx::query(
        "INSERT INTO tax_profile_versions
         (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'WATER', 'OTHER-SEL-JURISDICTION', 'SERVICE', '0.18', 'fixture', '2026-01-01', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .expect("insert ambiguous applicable tax profile");
    assert!(
        store
            .load_tariffs("SEL-TARIFF-1", "USD", "2026-01")
            .await
            .is_err(),
        "billing must fail closed instead of choosing an arbitrary jurisdiction tax"
    );

    let revoked = snapshot("REVOKED");
    store
        .apply_catalog_snapshot(&revoked)
        .await
        .expect("apply revocation tombstone");
    let product: (i64, String) =
        sqlx::query_as("SELECT active, status FROM products WHERE sku = 'KIT-1'")
            .fetch_one(&store.pool)
            .await
            .expect("read revoked product");
    assert_eq!(product, (0, "REVOKED".into()));
    let cached_hash: String =
        sqlx::query_scalar("SELECT value FROM catalog_sync_state WHERE key = 'sha256'")
            .fetch_one(&store.pool)
            .await
            .expect("read catalog hash");
    assert_eq!(cached_hash, revoked.sha256);
    let _ = std::fs::remove_file(path);
}
