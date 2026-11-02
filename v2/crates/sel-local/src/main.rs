#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let port = std::env::var("SEL_LOCAL_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8090);
    let database_url = std::env::var("SEL_LOCAL_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://sel-local-v2.sqlite".into());
    let organization_id = required_uuid("SEL_ORGANIZATION_ID")?;
    let device_id = required_uuid("SEL_DEVICE_ID")?;
    let store = sel_storage::LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .map_err(|error| std::io::Error::other(format!("local database failed: {error}")))?;
    sel_local::run_with_store(([127, 0, 0, 1], port).into(), store).await
}

fn required_uuid(name: &str) -> Result<uuid::Uuid, std::io::Error> {
    let value = std::env::var(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be configured; local identities are never generated implicitly"),
        )
    })?;
    value.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be a valid UUID"),
        )
    })
}
