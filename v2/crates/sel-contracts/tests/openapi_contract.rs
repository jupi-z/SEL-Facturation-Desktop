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
    assert!(paths.contains_key(serde_yaml::Value::from("/v1/invoices")));
    let invoice_list = paths
        .get(serde_yaml::Value::from("/v1/invoices"))
        .and_then(|path| path.get("get"))
        .expect("invoice history route is documented");
    assert!(invoice_list
        .get("parameters")
        .and_then(serde_yaml::Value::as_sequence)
        .is_some_and(|parameters| parameters.iter().any(|parameter| {
            parameter.get("name").and_then(serde_yaml::Value::as_str) == Some("status")
                && parameter.get("in").and_then(serde_yaml::Value::as_str) == Some("query")
        })));
    for (path, method) in [
        ("/v1/customers/{id}/service-accounts", "get"),
        ("/v1/customers/{id}/service-accounts", "post"),
        ("/v1/service-accounts/{id}/meters", "get"),
        ("/v1/service-accounts/{id}/meters", "post"),
        ("/v1/meters/{id}/readings", "get"),
        ("/v1/meters/{id}/readings", "post"),
        ("/v1/meter-readings/{id}/corrections", "post"),
    ] {
        let operation = paths
            .get(serde_yaml::Value::from(path))
            .and_then(|item| item.get(method))
            .unwrap_or_else(|| panic!("missing documented {method} {path}"));
        assert!(operation.get("security").is_some());
    }
    let customer_list = paths
        .get(serde_yaml::Value::from("/v1/customers"))
        .and_then(|path| path.get("get"))
        .expect("customer list route is documented");
    assert!(customer_list
        .get("parameters")
        .and_then(serde_yaml::Value::as_sequence)
        .is_some_and(|parameters| parameters.iter().any(|parameter| {
            parameter.get("name").and_then(serde_yaml::Value::as_str) == Some("q")
                && parameter.get("in").and_then(serde_yaml::Value::as_str) == Some("query")
        })));
    for (path, method) in [
        ("/v1/admin/catalog/tax-profiles", "get"),
        ("/v1/admin/catalog/tax-profiles", "post"),
        ("/v1/admin/catalog/tax-profiles/{id}/approve", "post"),
        ("/v1/admin/catalog/tax-profiles/{id}/revoke", "post"),
        ("/v1/admin/catalog/fx-rates", "get"),
        ("/v1/admin/catalog/fx-rates", "post"),
        ("/v1/admin/catalog/fx-rates/{id}/approve", "post"),
        ("/v1/admin/catalog/fx-rates/{id}/revoke", "post"),
        ("/v1/catalog/snapshot", "get"),
        ("/v1/admin/catalog/products", "get"),
        ("/v1/admin/catalog/products", "post"),
        ("/v1/admin/catalog/products/{id}/approve", "post"),
        ("/v1/admin/catalog/products/{id}/revoke", "post"),
    ] {
        let operation = paths
            .get(serde_yaml::Value::from(path))
            .and_then(|item| item.get(method))
            .unwrap_or_else(|| panic!("missing documented {method} {path}"));
        assert!(operation.get("security").is_some());
    }
    let tariff_draft = paths
        .get(serde_yaml::Value::from("/v1/admin/catalog/tariffs"))
        .expect("tariff administration route is documented");
    assert!(
        tariff_draft.get("get").is_some(),
        "draft list must be documented"
    );
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
    let tariff_revoke = paths
        .get(serde_yaml::Value::from(
            "/v1/admin/catalog/tariffs/{id}/revoke",
        ))
        .expect("tariff revocation route is documented");
    let revoke_operation = tariff_revoke
        .get("post")
        .expect("tariff revocation POST operation");
    assert!(revoke_operation.get("security").is_some());
    assert!(revoke_operation
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
        "RevokeTariffRequest",
        "CreateSolarProductDraftRequest",
        "ApproveSolarProductRequest",
        "CreateTaxProfileDraftRequest",
        "CreateFxRateDraftRequest",
        "ApproveReferenceDraftRequest",
        "CatalogSnapshot",
        "CatalogSnapshotPayload",
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
