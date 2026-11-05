pub mod live_update;

use sel_contracts::{SyncEnvelope, SyncEventResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CentralChange {
    pub sequence: i64,
    pub entity: String,
    pub entity_id: Uuid,
    pub revision: i64,
    pub change: serde_json::Value,
}

#[derive(Default)]
pub struct SyncEngine {
    inbox: HashMap<(Uuid, Uuid), StoredResult>,
    outbox: HashMap<Uuid, SyncEnvelope>,
    changes: Vec<CentralChange>,
    next_sequence: i64,
}

#[derive(Clone)]
struct StoredResult {
    payload_sha256: String,
    result: SyncEventResult,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SyncError {
    #[error("payload hash does not match the event body")]
    PayloadHashMismatch,
    #[error("same event id was submitted with another payload")]
    EventPayloadMismatch,
    #[error("event protocol version is unsupported")]
    ProtocolUnsupported,
}

impl SyncEngine {
    pub fn queue(&mut self, event: SyncEnvelope) -> Result<(), SyncError> {
        if event.protocol_version != sel_contracts::PROTOCOL_VERSION {
            return Err(SyncError::ProtocolUnsupported);
        }
        if payload_digest(&event.payload) != event.payload_sha256 {
            return Err(SyncError::PayloadHashMismatch);
        }
        self.outbox.insert(event.event_id, event);
        Ok(())
    }

    pub fn pending(&self) -> usize {
        self.outbox.len()
    }

    pub fn push(&mut self, events: &[SyncEnvelope]) -> Result<Vec<SyncEventResult>, SyncError> {
        let mut results = Vec::with_capacity(events.len());
        for event in events {
            if event.protocol_version != sel_contracts::PROTOCOL_VERSION {
                return Err(SyncError::ProtocolUnsupported);
            }
            let digest = payload_digest(&event.payload);
            if digest != event.payload_sha256 {
                return Err(SyncError::PayloadHashMismatch);
            }
            if let Some(original) = self.inbox.get(&(event.device_id, event.event_id)) {
                if original.payload_sha256 != digest {
                    results.push(SyncEventResult::Conflict {
                        reason: "EVENT_PAYLOAD_MISMATCH".into(),
                        central_revision: None,
                    });
                } else {
                    results.push(SyncEventResult::Duplicate {
                        original: Box::new(original.result.clone()),
                    });
                }
                continue;
            }
            self.next_sequence += 1;
            let result = SyncEventResult::Accepted {
                server_sequence: self.next_sequence,
                server_entity_id: event.aggregate_id,
            };
            self.inbox.insert(
                (event.device_id, event.event_id),
                StoredResult {
                    payload_sha256: digest,
                    result: result.clone(),
                },
            );
            self.changes.push(CentralChange {
                sequence: self.next_sequence,
                entity: event.command_type.clone(),
                entity_id: event.aggregate_id,
                revision: event.aggregate_expected_version + 1,
                change: event.payload.clone(),
            });
            self.outbox.remove(&event.event_id);
            results.push(result);
        }
        Ok(results)
    }

    pub fn pull(&self, cursor: i64, limit: usize) -> (Vec<CentralChange>, i64) {
        let changes: Vec<_> = self
            .changes
            .iter()
            .filter(|change| change.sequence > cursor)
            .take(limit)
            .cloned()
            .collect();
        let next_cursor = changes
            .last()
            .map(|change| change.sequence)
            .unwrap_or(cursor);
        (changes, next_cursor)
    }
}

pub fn payload_digest(payload: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(payload).expect("JSON serialization cannot fail"));
    hex::encode(digest.finalize())
}
