use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

async fn body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn login(app: &axum::Router, identity: &str, password: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"identity_ref":identity,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, body(response).await)
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn admin_directory_creates_lists_and_suspends_users_without_destructive_delete() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    std::env::remove_var("SEL_ALLOW_LEGACY_TOKEN");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("SEL admin-directory fixture")
        .await
        .unwrap();
    let admin_id = store
        .bootstrap_initial_admin("directory-admin", "admin passcode 2026")
        .await
        .unwrap();
    let app = router(AppState::with_production_central_store(store.clone()));
    let (status, login_response) = login(&app, "directory-admin", "admin passcode 2026").await;
    assert_eq!(status, StatusCode::OK);
    let admin_token = login_response["access_token"].as_str().unwrap();

    let create = app.clone().oneshot(
        Request::builder().method("POST").uri("/v1/admin/users")
            .header("authorization", format!("Bearer {admin_token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({"identity_ref":"billing-user","role":"BILLING_AGENT","password":"billing passcode 2026"}).to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let user = body(create).await;
    let user_id = user["id"].as_str().unwrap().parse::<Uuid>().unwrap();

    let duplicate = app.clone().oneshot(
        Request::builder().method("POST").uri("/v1/admin/users")
            .header("authorization", format!("Bearer {admin_token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({"identity_ref":" BILLING-USER ","role":"CASHIER","password":"another passcode 2026"}).to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let (status, user_login) = login(&app, " BILLING-USER ", "billing passcode 2026").await;
    assert_eq!(status, StatusCode::OK);
    let user_token = user_login["access_token"].as_str().unwrap();
    let denied_list = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/users")
                .header("authorization", format!("Bearer {user_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied_list.status(), StatusCode::FORBIDDEN);

    let listing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/users")
                .header("authorization", format!("Bearer {admin_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let users = body(listing).await;
    assert_eq!(users.as_array().unwrap().len(), 2);
    assert!(users
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry.get("password_hash").is_none()));

    let other_organization_id = Uuid::new_v4();
    let other_organization = CentralStore::connect(&database_url, other_organization_id)
        .await
        .unwrap();
    other_organization
        .ensure_organization("Other organization fixture")
        .await
        .unwrap();
    other_organization
        .bootstrap_initial_admin("other-admin", "other passcode 2026")
        .await
        .unwrap();
    assert_eq!(store.admin_list_users().await.unwrap().len(), 2);

    let changed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/admin/users/{user_id}"))
                .header("authorization", format!("Bearer {admin_token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"role":"CASHIER","status":"REVOKED"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(changed.status(), StatusCode::OK);
    assert_eq!(body(changed).await["status"], "REVOKED");
    assert_eq!(
        store
            .authenticate_access_token(user_token)
            .await
            .unwrap_err()
            .to_string(),
        "session is invalid or expired"
    );
    let (status, _) = login(&app, "billing-user", "billing passcode 2026").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let protected_admin = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/admin/users/{admin_id}"))
                .header("authorization", format!("Bearer {admin_token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"status":"REVOKED"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protected_admin.status(), StatusCode::CONFLICT);
}
