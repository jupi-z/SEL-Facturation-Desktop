#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let port = std::env::var("SEL_LOCAL_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8090);
    let database_url = std::env::var("SEL_LOCAL_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://sel-local-v2.sqlite".into());
    let organization_id = std::env::var("SEL_ORGANIZATION_ID")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(uuid::Uuid::new_v4);
    let device_id = std::env::var("SEL_DEVICE_ID")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(uuid::Uuid::new_v4);
    let store = sel_storage::LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .map_err(|error| std::io::Error::other(format!("local database failed: {error}")))?;
    sel_local::run_with_store(([127, 0, 0, 1], port).into(), store).await
}
