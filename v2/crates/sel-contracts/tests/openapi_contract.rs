#[test]
fn openapi_document_is_valid_yaml_and_declares_no_pdf_route() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../openapi/sel-v2.yaml");
    let document: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let paths = document
        .get("paths")
        .and_then(serde_yaml::Value::as_mapping)
        .unwrap();
    assert!(paths.contains_key(serde_yaml::Value::from("/v1/invoices/{id}/document-data")));
    let tariff_draft = paths
        .get(serde_yaml::Value::from("/v1/admin/catalog/tariffs"))
        .expect("tariff administration route is documented");
    let create_operation = tariff_draft
        .get("post")
        .expect("tariff draft POST operation");
    assert!(create_operation.get("security").is_some());
    assert!(create_operation
        .get("parameters")
        .and_then(serde_yaml::Value::as_sequence)
        .is_some_and(|parameters| parameters.iter().any(|parameter| {
            parameter.get("$ref").and_then(serde_yaml::Value::as_str)
                == Some("#/components/parameters/IdempotencyKey")
        })));
    let tariff_approval = paths
        .get(serde_yaml::Value::from(
            "/v1/admin/catalog/tariffs/{id}/approve",
        ))
        .expect("tariff approval route is documented");
    let approve_operation = tariff_approval
        .get("post")
        .expect("tariff approval POST operation");
    assert!(approve_operation.get("security").is_some());
    assert!(approve_operation
        .get("parameters")
        .and_then(serde_yaml::Value::as_sequence)
        .is_some_and(|parameters| parameters.len() == 2));
    let schemas = document
        .get("components")
        .and_then(|value| value.get("schemas"))
        .and_then(serde_yaml::Value::as_mapping)
        .unwrap();
    for schema in [
        "CreateTariffDraftRequest",
        "ApproveTariffDraftRequest",
        "TariffDraft",
    ] {
        assert!(schemas.contains_key(serde_yaml::Value::from(schema)));
    }
    assert!(!paths
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .any(|path| path.to_ascii_lowercase().contains("pdf")));

    for (path, method) in [
        ("/v1/admin/users", "get"),
        ("/v1/admin/users", "post"),
        ("/v1/admin/users/{id}", "patch"),
        ("/v1/admin/devices", "get"),
        ("/v1/admin/offline-grants", "get"),
        ("/v1/admin/offline-grants/{id}/revoke", "post"),
    ] {
        let operation = paths
            .get(serde_yaml::Value::from(path))
            .and_then(|item| item.get(method))
            .unwrap_or_else(|| panic!("missing documented {method} {path}"));
        assert!(
            operation.get("security").is_some(),
            "{method} {path} must be authenticated"
        );
    }
    let schemas = document
        .get("components")
        .and_then(|value| value.get("schemas"))
        .unwrap();
    assert!(schemas.get("CreateAdminUserRequest").is_some());
    assert!(schemas.get("UpdateAdminUserRequest").is_some());
}
