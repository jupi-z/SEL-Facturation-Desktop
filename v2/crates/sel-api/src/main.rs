use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::VerifyingKey;
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
        required_session_auth_mode()?;
        required_offline_grant_security()?;
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

fn required_session_auth_mode() -> Result<(), std::io::Error> {
    if std::env::var("SEL_AUTH_MODE").ok().as_deref() != Some("session") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "SEL_AUTH_MODE=session is required; the shared bearer token is test-only",
        ));
    }
    Ok(())
}

fn required_offline_grant_security() -> Result<(), std::io::Error> {
    let valid_key = std::env::var("SEL_OFFLINE_GRANT_PUBLIC_KEY")
        .ok()
        .and_then(|value| BASE64.decode(value.trim()).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok())
        .is_some();
    if std::env::var("SEL_REQUIRE_SIGNED_OFFLINE_GRANTS")
        .ok()
        .as_deref()
        != Some("1")
        || !valid_key
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "signed offline grants and SEL_OFFLINE_GRANT_PUBLIC_KEY are required in commercial mode",
        ));
    }
    Ok(())
}

#[cfg(test)]
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
    fn production_requires_individual_session_authentication() {
        std::env::remove_var("SEL_AUTH_MODE");
        assert!(required_session_auth_mode().is_err());
        std::env::set_var("SEL_AUTH_MODE", "legacy");
        assert!(required_session_auth_mode().is_err());
        std::env::set_var("SEL_AUTH_MODE", "session");
        assert!(required_session_auth_mode().is_ok());
        std::env::remove_var("SEL_AUTH_MODE");
    }

    #[test]
    fn production_requires_signed_offline_grant_configuration() {
        std::env::remove_var("SEL_REQUIRE_SIGNED_OFFLINE_GRANTS");
        std::env::remove_var("SEL_OFFLINE_GRANT_PUBLIC_KEY");
        assert!(required_offline_grant_security().is_err());
        std::env::set_var("SEL_REQUIRE_SIGNED_OFFLINE_GRANTS", "1");
        std::env::set_var("SEL_OFFLINE_GRANT_PUBLIC_KEY", "fixture-key");
        assert!(required_offline_grant_security().is_err());
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        std::env::set_var(
            "SEL_OFFLINE_GRANT_PUBLIC_KEY",
            BASE64.encode(signing_key.verifying_key().to_bytes()),
        );
        assert!(required_offline_grant_security().is_ok());
        std::env::remove_var("SEL_REQUIRE_SIGNED_OFFLINE_GRANTS");
        std::env::remove_var("SEL_OFFLINE_GRANT_PUBLIC_KEY");
    }
}
