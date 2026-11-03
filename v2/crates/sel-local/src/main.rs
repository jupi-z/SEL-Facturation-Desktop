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
    let mut store = sel_storage::LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .map_err(|error| std::io::Error::other(format!("local database failed: {error}")))?;
    match (
        std::env::var("SEL_ACTOR_ID"),
        std::env::var("SEL_OFFLINE_GRANT_ID"),
    ) {
        (Ok(actor), Ok(grant)) => {
            let actor_id = required_value("SEL_ACTOR_ID", &actor)?;
            let grant_id = required_value("SEL_OFFLINE_GRANT_ID", &grant)?;
            store
                .configure_offline_identity(actor_id, grant_id)
                .await
                .map_err(|error| {
                    std::io::Error::other(format!("offline identity configuration failed: {error}"))
                })?;
        }
        (Err(_), Err(_)) => {}
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "SEL_ACTOR_ID and SEL_OFFLINE_GRANT_ID must be configured together",
            ));
        }
    }
    let auth_token = required_token("SEL_LOCAL_AUTH_TOKEN")?;
    match (
        std::env::var("SEL_CENTRAL_API_URL"),
        std::env::var("SEL_CENTRAL_API_TOKEN"),
    ) {
        (Ok(endpoint), Ok(sync_token)) => {
            validate_token("SEL_CENTRAL_API_TOKEN", sync_token.as_str())?;
            sel_local::run_with_store_and_auth_and_sync(
                ([127, 0, 0, 1], port).into(),
                store,
                auth_token,
                endpoint,
                sync_token,
            )
            .await
        }
        (Err(_), Err(_)) => {
            sel_local::run_with_store_and_auth(([127, 0, 0, 1], port).into(), store, auth_token)
                .await
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "SEL_CENTRAL_API_URL and SEL_CENTRAL_API_TOKEN must be configured together",
        )),
    }
}

fn required_uuid(name: &str) -> Result<uuid::Uuid, std::io::Error> {
    let value = std::env::var(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be configured; local identities are never generated implicitly"),
        )
    })?;
    let uuid = value.parse::<uuid::Uuid>().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be a valid UUID"),
        )
    })?;
    if uuid.is_nil() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must not be the nil UUID"),
        ));
    }
    Ok(uuid)
}

fn required_value(name: &str, value: &str) -> Result<uuid::Uuid, std::io::Error> {
    let uuid = value.parse::<uuid::Uuid>().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be a valid UUID"),
        )
    })?;
    if uuid.is_nil() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must not be the nil UUID"),
        ));
    }
    Ok(uuid)
}

fn required_token(name: &str) -> Result<String, std::io::Error> {
    let token = std::env::var(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be configured before the local API starts"),
        )
    })?;
    validate_token(name, &token)
}

fn validate_token(name: &str, token: &str) -> Result<String, std::io::Error> {
    if token.trim().len() < 16 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must contain at least 16 non-space characters"),
        ));
    }
    Ok(token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{required_uuid, required_value};
    use uuid::Uuid;

    #[test]
    fn local_identity_configuration_rejects_nil_and_accepts_stable_ids() {
        assert!(required_value("SEL_ACTOR_ID", &Uuid::nil().to_string()).is_err());
        assert!(required_value("SEL_OFFLINE_GRANT_ID", &Uuid::nil().to_string()).is_err());
        assert!(required_value("SEL_ACTOR_ID", &Uuid::new_v4().to_string()).is_ok());
    }

    #[test]
    fn local_required_uuid_rejects_nil() {
        std::env::set_var("SEL_TEST_NIL_UUID", Uuid::nil().to_string());
        assert!(required_uuid("SEL_TEST_NIL_UUID").is_err());
        std::env::remove_var("SEL_TEST_NIL_UUID");
    }
}
