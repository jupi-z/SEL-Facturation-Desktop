use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResourceEntry {
    pub bundle_id: String,
    pub version: String,
    pub resource_type: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UpdateManifest {
    pub channel: String,
    pub key_id: String,
    pub sequence: u64,
    pub expires_at: String,
    pub min_client_version: String,
    pub resources: Vec<ResourceEntry>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LiveUpdateError {
    #[error("signature is invalid")]
    InvalidSignature,
    #[error("manifest key is not pinned")]
    UnknownKey,
    #[error("manifest is expired or malformed")]
    Expired,
    #[error("manifest sequence is not newer than the installed sequence")]
    Rollback,
    #[error("resource type is not declarative")]
    InvalidResourceType,
    #[error("resource path escapes its staging directory")]
    PathEscape,
    #[error("resource hash or size does not match")]
    PackageMismatch,
}

pub fn verify_manifest(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    key_id: &str,
    pinned: &[(String, VerifyingKey)],
    installed_sequence: u64,
    now: DateTime<Utc>,
) -> Result<UpdateManifest, LiveUpdateError> {
    let (_, key) = pinned
        .iter()
        .find(|(id, _)| id == key_id)
        .ok_or(LiveUpdateError::UnknownKey)?;
    let signature =
        Signature::from_slice(signature_bytes).map_err(|_| LiveUpdateError::InvalidSignature)?;
    key.verify(manifest_bytes, &signature)
        .map_err(|_| LiveUpdateError::InvalidSignature)?;
    let manifest: UpdateManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_| LiveUpdateError::InvalidSignature)?;
    if manifest.key_id != key_id {
        return Err(LiveUpdateError::UnknownKey);
    }
    let expires_at = DateTime::parse_from_rfc3339(&manifest.expires_at)
        .map_err(|_| LiveUpdateError::Expired)?
        .with_timezone(&Utc);
    if expires_at <= now {
        return Err(LiveUpdateError::Expired);
    }
    if manifest.sequence <= installed_sequence {
        return Err(LiveUpdateError::Rollback);
    }
    if manifest.resources.iter().any(|entry| {
        !matches!(
            entry.resource_type.as_str(),
            "configuration" | "invoice_template" | "ui_assets"
        )
    }) {
        return Err(LiveUpdateError::InvalidResourceType);
    }
    if manifest
        .resources
        .iter()
        .any(|entry| !safe_relative_path(&entry.path))
    {
        return Err(LiveUpdateError::PathEscape);
    }
    Ok(manifest)
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != ".." && !part.contains('\\'))
}

pub fn verify_resource(entry: &ResourceEntry, bytes: &[u8]) -> Result<(), LiveUpdateError> {
    if entry.size != bytes.len() as u64 {
        return Err(LiveUpdateError::PackageMismatch);
    }
    let mut digest = Sha256::new();
    digest.update(bytes);
    if hex::encode(digest.finalize()) != entry.sha256 {
        return Err(LiveUpdateError::PackageMismatch);
    }
    Ok(())
}

#[derive(Default)]
pub struct BundleManager {
    current: Option<UpdateManifest>,
    known_good: Option<UpdateManifest>,
}

impl BundleManager {
    pub fn current(&self) -> Option<&UpdateManifest> {
        self.current.as_ref()
    }

    pub fn activate(
        &mut self,
        manifest_bytes: &[u8],
        signature_bytes: &[u8],
        key_id: &str,
        pinned: &[(String, VerifyingKey)],
        resources: &[(&ResourceEntry, &[u8])],
        now: DateTime<Utc>,
    ) -> Result<(), LiveUpdateError> {
        let installed_sequence = self
            .current
            .as_ref()
            .map(|manifest| manifest.sequence)
            .unwrap_or(0);
        let manifest = verify_manifest(
            manifest_bytes,
            signature_bytes,
            key_id,
            pinned,
            installed_sequence,
            now,
        )?;
        for entry in &manifest.resources {
            let bytes = resources
                .iter()
                .find(|(candidate, _)| {
                    candidate.path == entry.path && candidate.bundle_id == entry.bundle_id
                })
                .map(|(_, bytes)| *bytes)
                .ok_or(LiveUpdateError::PackageMismatch)?;
            verify_resource(entry, bytes)?;
        }
        let previous = self.current.replace(manifest);
        self.known_good = previous.or_else(|| self.known_good.take());
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<(), LiveUpdateError> {
        let previous = self.known_good.take().ok_or(LiveUpdateError::Rollback)?;
        let current = self.current.replace(previous);
        self.known_good = current;
        Ok(())
    }
}
