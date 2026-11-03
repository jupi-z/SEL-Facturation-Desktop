use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn individual_session_login_rotation_and_revocation_are_enforced() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    std::env::remove_var("SEL_ALLOW_LEGACY_TOKEN");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL auth fixture")
        .await
        .expect("create fixture organization");
    let user_id = Uuid::new_v4();
    let password_hash = CentralStore::hash_user_password("correct horse battery staple")
        .expect("hash fixture password");
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
         VALUES ($1, $2, 'ADMIN', 'ACTIVE', 'auth-fixture', $3)",
    )
    .bind(user_id)
    .bind(organization_id)
    .bind(password_hash)
    .execute(&store.pool)
    .await
    .expect("insert fixture user");
    let target_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref)
         VALUES ($1, $2, 'CASHIER', 'ACTIVE', 'cashier-fixture')",
    )
    .bind(target_user_id)
    .bind(organization_id)
    .execute(&store.pool)
    .await
    .expect("insert target user");

    let audit_pool = store.pool.clone();
    let app = router(AppState::with_production_central_store(store));
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "identity_ref": "auth-fixture",
                        "password": "correct horse battery staple"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let login = json_body(login).await;
    assert_eq!(login["user_id"], user_id.to_string());
    let first_access = login["access_token"].as_str().unwrap().to_owned();
    let first_refresh = login["refresh_token"].as_str().unwrap().to_owned();

    let authorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/customers")
                .header("authorization", format!("Bearer {first_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);

    let provisioned = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/users/{target_user_id}/password"))
                .header("authorization", format!("Bearer {first_access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "password": "cashier fixture password"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(provisioned.status(), StatusCode::NO_CONTENT);
    let target_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "identity_ref": "cashier-fixture",
                        "password": "cashier fixture password"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(target_login.status(), StatusCode::OK);
    let target_login = json_body(target_login).await;
    let target_access = target_login["access_token"].as_str().unwrap();

    let rotated_password = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/users/{target_user_id}/password"))
                .header("authorization", format!("Bearer {first_access}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "password": "new cashier fixture password"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotated_password.status(), StatusCode::NO_CONTENT);

    let revoked_target_session = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/customers")
                .header("authorization", format!("Bearer {target_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_target_session.status(), StatusCode::UNAUTHORIZED);
    let new_password_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "identity_ref": "cashier-fixture",
                        "password": "new cashier fixture password"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_password_login.status(), StatusCode::OK);
    let audit = sqlx::query(
        "SELECT count(*) AS count, bool_and(actor = $1) AS actor_matches,
                bool_and(NOT (redacted_metadata ? 'password')) AS no_password
         FROM audit_events
         WHERE event_type = 'USER_PASSWORD_PROVISIONED' AND subject_id = $2",
    )
    .bind(user_id)
    .bind(target_user_id)
    .fetch_one(&audit_pool)
    .await
    .expect("read password provision audit");
    assert_eq!(audit.try_get::<i64, _>("count").unwrap(), 2);
    assert!(audit.try_get::<bool, _>("actor_matches").unwrap());
    assert!(audit.try_get::<bool, _>("no_password").unwrap());

    let rotated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"refresh_token": first_refresh})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::OK);
    let rotated = json_body(rotated).await;
    let second_access = rotated["access_token"].as_str().unwrap().to_owned();
    let old_access = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/customers")
                .header("authorization", format!("Bearer {first_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_access.status(), StatusCode::UNAUTHORIZED);

    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/logout")
                .header("authorization", format!("Bearer {second_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let revoked = app
        .oneshot(
            Request::builder()
                .uri("/v1/customers")
                .header("authorization", format!("Bearer {second_access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn role_and_organization_boundaries_reject_write_escalation_and_idor() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let foreign_organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect organization store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL RBAC organization")
        .await
        .expect("create organization");
    let foreign_store = CentralStore::connect(&database_url, foreign_organization_id)
        .await
        .expect("connect foreign store");
    foreign_store
        .ensure_organization("SEL foreign organization")
        .await
        .expect("create foreign organization");
    let cashier_id = Uuid::new_v4();
    let password_hash = CentralStore::hash_user_password("cashier boundary password")
        .expect("hash cashier password");
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
         VALUES ($1, $2, 'CASHIER', 'ACTIVE', 'rbac-cashier', $3)",
    )
    .bind(cashier_id)
    .bind(organization_id)
    .bind(password_hash)
    .execute(&store.pool)
    .await
    .expect("insert cashier");
    let foreign_customer = foreign_store
        .create_customer(&sel_contracts::CreateCustomer {
            customer_code: format!("FOREIGN-{}", &foreign_organization_id.to_string()[..8]),
            legal_name: "Foreign organization customer".into(),
            address: None,
            phone: None,
        })
        .await
        .expect("create foreign customer");

    let app = router(AppState::with_production_central_store(store));
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "identity_ref": "rbac-cashier",
                        "password": "cashier boundary password"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let login = json_body(login).await;
    let access_token = login["access_token"].as_str().unwrap();

    let forbidden_write = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/customers")
                .header("authorization", format!("Bearer {access_token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_code": "RBAC-WRITE",
                        "legal_name": "Must be rejected"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden_write.status(), StatusCode::FORBIDDEN);

    let foreign_read = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/customers/{}", foreign_customer.id))
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(foreign_read.status(), StatusCode::NOT_FOUND);
}
