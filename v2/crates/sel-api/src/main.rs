use sel_api::{run, AppState};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let port = std::env::var("SEL_API_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let bind = std::env::var("SEL_API_BIND").unwrap_or_else(|_| "127.0.0.1".into());
    let address = format!("{bind}:{port}")
        .parse()
        .map_err(|error| std::io::Error::other(format!("invalid SEL_API_BIND: {error}")))?;
    tracing_subscriber::fmt::init();
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        let pool = sel_storage::open_postgres(&database_url)
            .await
            .map_err(|error| {
                std::io::Error::other(format!("database connection failed: {error}"))
            })?;
        sel_storage::migrate_postgres(&pool)
            .await
            .map_err(|error| {
                std::io::Error::other(format!("database migration failed: {error}"))
            })?;
    }
    run(address, AppState::new(Uuid::new_v4())).await
}
