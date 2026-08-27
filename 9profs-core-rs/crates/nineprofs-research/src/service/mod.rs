use std::sync::Arc;

use nineprofs_api_types::EventEnvelope;
use nineprofs_realtime::BroadcastEventBus;
use sha2::{Digest, Sha256};

use crate::{ContentHash, HashAlgorithm, ResearchCaseId, ResearchError, ResearchRepository};

mod case_source_snapshot;
mod citation;
mod evidence_claims;
mod manuscript_citation_sync;
mod manuscript_claim_extraction;
mod pdf;
mod reference_catalog;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct ResearchService {
    repository: Arc<dyn ResearchRepository>,
    events: Arc<BroadcastEventBus>,
    artifact_store: Option<Arc<crate::ResearchArtifactStore>>,
    claim_extractor: Option<Arc<dyn crate::ManuscriptClaimExtractionProvider>>,
}

impl ResearchService {
    pub fn new(
        repository: crate::SqliteResearchRepository,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self::with_repository(Arc::new(repository), events)
    }

    pub fn with_repository(
        repository: Arc<dyn ResearchRepository>,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            repository,
            events,
            artifact_store: None,
            claim_extractor: None,
        }
    }

    pub fn with_artifact_store(mut self, store: Arc<crate::ResearchArtifactStore>) -> Self {
        self.artifact_store = Some(store);
        self
    }

    pub fn artifact_store(&self) -> Option<Arc<crate::ResearchArtifactStore>> {
        self.artifact_store.clone()
    }

    pub fn with_claim_extractor(
        mut self,
        extractor: Arc<dyn crate::ManuscriptClaimExtractionProvider>,
    ) -> Self {
        self.claim_extractor = Some(extractor);
        self
    }
    async fn ensure_case(&self, id: &ResearchCaseId) -> Result<(), ResearchError> {
        if self.repository.get_case(id).await?.is_none() {
            return Err(not_found("case", id.as_str()));
        }
        Ok(())
    }

    fn publish(&self, name: &str, payload: serde_json::Value) {
        let _ = self.events.publish(EventEnvelope::new(name, payload));
    }
}

pub(super) fn not_found(entity: &'static str, id: &str) -> ResearchError {
    ResearchError::NotFound {
        entity,
        id: id.to_owned(),
    }
}

pub(super) fn sha256_hash(value: &[u8]) -> ContentHash {
    let digest = Sha256::digest(value);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    ContentHash {
        algorithm: HashAlgorithm::Sha256,
        value: hex,
    }
}
