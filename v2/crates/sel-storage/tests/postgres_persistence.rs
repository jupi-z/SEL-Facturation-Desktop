use sel_contracts::CreateCustomer;
use sel_storage::CentralStore;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn customer_survives_a_fresh_postgres_store_after_restart() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let first = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    first
        .ensure_organization("SEL V2 integration fixture")
        .await
        .expect("create fixture organization");
    let created = first
        .create_customer(&CreateCustomer {
            customer_code: format!("PG-{}", &organization_id.to_string()[..8]),
            legal_name: "PostgreSQL restart fixture".into(),
            address: Some("synthetic".into()),
            phone: None,
        })
        .await
        .expect("create customer");
    drop(first);

    let second = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("reconnect central store");
    let loaded = second
        .get_customer(created.id)
        .await
        .expect("read customer after restart")
        .expect("customer exists after restart");
    assert_eq!(loaded.id, created.id);
    assert_eq!(loaded.legal_name, "PostgreSQL restart fixture");
}
