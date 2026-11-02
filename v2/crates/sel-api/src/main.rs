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
    let database_url = std::env::var("DATABASE_URL").ok();
    let organization_id = match database_url.is_some() {
        true => required_uuid("SEL_ORGANIZATION_ID")?,
        false => {
            if std::env::var("SEL_ALLOW_IN_MEMORY").ok().as_deref() != Some("1") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "SEL_ALLOW_IN_MEMORY=1 is required for the non-commercial in-memory mode",
                ));
            }
            required_uuid("SEL_ORGANIZATION_ID").unwrap_or_else(|_| Uuid::new_v4())
        }
    };
    let state = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        required_token("SEL_API_AUTH_TOKEN")?;
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
        let store = sel_storage::CentralStore::from_pool(pool, organization_id);
        let legal_name = std::env::var("SEL_ORGANIZATION_NAME")
            .unwrap_or_else(|_| "SEL V2 test organization".into());
        store
            .ensure_organization(&legal_name)
            .await
            .map_err(|error| {
                std::io::Error::other(format!("organization bootstrap failed: {error}"))
            })?;
        AppState::with_production_central_store(store)
    } else {
        AppState::new(organization_id)
    };
    run(address, state).await
}

fn required_uuid(name: &str) -> Result<Uuid, std::io::Error> {
    let value = std::env::var(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be configured; identities are never generated in production"),
        )
    })?;
    value.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be a valid UUID"),
        )
    })
}

fn required_token(name: &str) -> Result<String, std::io::Error> {
    let token = std::env::var(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be configured before commercial mode starts"),
        )
    })?;
    if token.trim().len() < 16 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must contain at least 16 non-space characters"),
        ));
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_uuid_parser_rejects_missing_or_invalid_values() {
        assert!(required_uuid_value("SEL_ORGANIZATION_ID", None).is_err());
        assert!(required_uuid_value("SEL_ORGANIZATION_ID", Some("not-a-uuid".into())).is_err());
        assert!(required_uuid_value("SEL_ORGANIZATION_ID", Some(Uuid::nil().to_string())).is_ok());
    }

    #[test]
    fn production_token_validator_rejects_missing_and_short_values() {
        assert!(validate_token_value("SEL_API_AUTH_TOKEN", None).is_err());
        assert!(validate_token_value("SEL_API_AUTH_TOKEN", Some("short".into())).is_err());
        assert!(
            validate_token_value("SEL_API_AUTH_TOKEN", Some("a-secure-test-token".into())).is_ok()
        );
    }
}

fn required_uuid_value(name: &str, value: Option<String>) -> Result<Uuid, std::io::Error> {
    let value = value.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} is missing"),
        )
    })?;
    value.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be a valid UUID"),
        )
    })
}

fn validate_token_value(name: &str, value: Option<String>) -> Result<String, std::io::Error> {
    let token = value.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} is missing"),
        )
    })?;
    if token.trim().len() < 16 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} is too short"),
        ));
    }
    Ok(token)
}
