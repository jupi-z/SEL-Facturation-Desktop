use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use sel_sync::live_update::{
    verify_manifest, verify_resource, BundleManager, LiveUpdateError, ResourceEntry, UpdateManifest,
};
use sha2::{Digest, Sha256};

fn fixture() -> (Vec<u8>, Vec<u8>, Vec<(String, ed25519_dalek::VerifyingKey)>) {
    let signing = SigningKey::from_bytes(&[7u8; 32]);
    let bytes = b"configuration-v1";
    let mut hash = Sha256::new();
    hash.update(bytes);
    let manifest = UpdateManifest {
        channel: "stable".into(),
        key_id: "test-key".into(),
        sequence: 2,
        expires_at: (Utc::now() + Duration::hours(1)).to_rfc3339(),
        min_client_version: "0.1.0".into(),
        resources: vec![ResourceEntry {
            bundle_id: "bundle-1".into(),
            version: "1".into(),
            resource_type: "configuration".into(),
            path: "config/settings.json".into(),
            size: bytes.len() as u64,
            sha256: hex::encode(hash.finalize()),
        }],
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let signature = signing.sign(&manifest_bytes).to_bytes().to_vec();
    (
        manifest_bytes,
        signature,
        vec![("test-key".into(), signing.verifying_key())],
    )
}

#[test]
fn valid_manifest_and_resource_are_accepted() {
    let (manifest, signature, keys) = fixture();
    let verified =
        verify_manifest(&manifest, &signature, "test-key", &keys, 1, Utc::now()).unwrap();
    verify_resource(&verified.resources[0], b"configuration-v1").unwrap();
}

#[test]
fn forged_signature_hash_traversal_expiry_and_rollback_are_rejected() {
    let (manifest, signature, keys) = fixture();
    let mut forged = signature.clone();
    forged[0] ^= 1;
    assert_eq!(
        verify_manifest(&manifest, &forged, "test-key", &keys, 1, Utc::now()),
        Err(LiveUpdateError::InvalidSignature)
    );
    let verified =
        verify_manifest(&manifest, &signature, "test-key", &keys, 1, Utc::now()).unwrap();
    assert_eq!(
        verify_resource(&verified.resources[0], b"tampered"),
        Err(LiveUpdateError::PackageMismatch)
    );
    assert_eq!(
        verify_manifest(&manifest, &signature, "test-key", &keys, 2, Utc::now()),
        Err(LiveUpdateError::Rollback)
    );
    assert_eq!(
        verify_manifest(
            &manifest,
            &signature,
            "test-key",
            &keys,
            0,
            Utc::now() + Duration::hours(2)
        ),
        Err(LiveUpdateError::Expired)
    );
    let mut traversal: UpdateManifest = serde_json::from_slice(&manifest).unwrap();
    traversal.resources[0].path = "../escape.json".into();
    let traversal_bytes = serde_json::to_vec(&traversal).unwrap();
    let signing = SigningKey::from_bytes(&[7u8; 32]);
    let traversal_signature = signing.sign(&traversal_bytes).to_bytes();
    assert_eq!(
        verify_manifest(
            &traversal_bytes,
            &traversal_signature,
            "test-key",
            &keys,
            0,
            Utc::now()
        ),
        Err(LiveUpdateError::PathEscape)
    );

    for path in [
        r"..\escape.json",
        r"C:\escape.json",
        r"\absolute\escape.json",
        "/absolute/escape.json",
        "config//escape.json",
    ] {
        let mut hostile: UpdateManifest = serde_json::from_slice(&manifest).unwrap();
        hostile.resources[0].path = path.into();
        let hostile_bytes = serde_json::to_vec(&hostile).unwrap();
        let hostile_signature = signing.sign(&hostile_bytes).to_bytes();
        assert_eq!(
            verify_manifest(
                &hostile_bytes,
                &hostile_signature,
                "test-key",
                &keys,
                0,
                Utc::now()
            ),
            Err(LiveUpdateError::PathEscape),
            "hostile path must be rejected: {path}"
        );
    }
}

#[test]
fn bundle_manager_keeps_a_known_good_version_for_rollback() {
    let (manifest_bytes, signature, keys) = fixture();
    let manifest: UpdateManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    let mut manager = BundleManager::default();
    manager
        .activate(
            &manifest_bytes,
            &signature,
            "test-key",
            &keys,
            &[(&manifest.resources[0], b"configuration-v1")],
            Utc::now(),
        )
        .unwrap();
    assert_eq!(manager.current().unwrap().sequence, 2);
    let mut next = manifest.clone();
    next.sequence = 3;
    next.resources[0].version = "2".into();
    let bytes = serde_json::to_vec(b"configuration-v2").unwrap();
    let mut hash = Sha256::new();
    hash.update(&bytes);
    next.resources[0].size = bytes.len() as u64;
    next.resources[0].sha256 = hex::encode(hash.finalize());
    let next_bytes = serde_json::to_vec(&next).unwrap();
    let signing = SigningKey::from_bytes(&[7u8; 32]);
    let next_signature = signing.sign(&next_bytes).to_bytes();
    manager
        .activate(
            &next_bytes,
            &next_signature,
            "test-key",
            &keys,
            &[(&next.resources[0], &bytes)],
            Utc::now(),
        )
        .unwrap();
    assert_eq!(manager.current().unwrap().sequence, 3);
    manager.rollback().unwrap();
    assert_eq!(manager.current().unwrap().sequence, 2);
}
