use sel_storage::LocalStore;
use uuid::Uuid;

#[tokio::test]
async fn local_store_rejects_an_identity_change_after_initial_enrollment() {
    let path = std::env::temp_dir().join(format!("sel-v2-identity-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let mut initial = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("initial local identity enrollment");
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    initial
        .configure_offline_identity(actor_id, grant_id)
        .await
        .expect("offline identity enrollment");
    let reopened = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("reopen local identity");
    assert_eq!(reopened.actor_id, actor_id);
    assert_eq!(reopened.offline_grant_id, grant_id);

    let error = match LocalStore::connect(&database_url, Uuid::new_v4(), device_id).await {
        Ok(_) => panic!("identity changes must fail closed"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("organization identity"));
    let _ = std::fs::remove_file(path);
}
