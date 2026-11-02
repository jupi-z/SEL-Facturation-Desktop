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
    assert!(!paths
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .any(|path| path.to_ascii_lowercase().contains("pdf")));
}
