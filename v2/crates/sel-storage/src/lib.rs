use sqlx::{PgPool, SqlitePool};

pub async fn migrate_postgres(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations/postgres").run(pool).await
}

pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations/sqlite").run(pool).await
}

pub async fn open_sqlite(url: &str) -> Result<SqlitePool, sqlx::Error> {
    SqlitePool::connect(url).await
}

pub async fn open_postgres(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(url).await
}
