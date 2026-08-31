use async_trait::async_trait;
use thiserror::Error;

use crate::{
    ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionIdentity,
    ManuscriptClaimExtractionOutput, ManuscriptClaimInventoryBlockInput,
    ManuscriptClaimInventoryIdentity, ManuscriptClaimInventoryOutput,
    RegulationRequirementCandidateExtractionIdentity, RegulationRequirementCandidateOutput,
    RegulationRequirementExtractionInput,
};

#[derive(Debug, Error)]
pub enum ManuscriptClaimExtractionProviderError {
    #[error("claim extractor is not configured")]
    NotConfigured,
    #[error("claim extractor configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("claim extractor request timed out")]
    Timeout,
    #[error("claim extractor transport failed")]
    Transport,
    #[error("claim extractor response was malformed")]
    MalformedResponse,
    #[error("claim extractor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("claim extractor response exceeded size limit")]
    ResponseTooLarge,
}

#[async_trait]
pub trait ManuscriptClaimExtractionProvider: Send + Sync {
    fn identity(&self) -> ManuscriptClaimExtractionIdentity;

    async fn extract(
        &self,
        block: ManuscriptClaimExtractionBlockInput,
    ) -> Result<ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProviderError>;
}

#[derive(Debug, Error)]
pub enum ManuscriptClaimInventoryProviderError {
    #[error("claim inventory extractor is not configured")]
    NotConfigured,
    #[error("claim inventory extractor configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("claim inventory extractor request timed out")]
    Timeout,
    #[error("claim inventory extractor transport failed")]
    Transport,
    #[error("claim inventory extractor response was malformed")]
    MalformedResponse,
    #[error("claim inventory extractor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("claim inventory extractor response exceeded size limit")]
    ResponseTooLarge,
}

#[async_trait]
pub trait ManuscriptClaimInventoryProvider: Send + Sync {
    fn identity(&self) -> ManuscriptClaimInventoryIdentity;

    async fn extract(
        &self,
        block: ManuscriptClaimInventoryBlockInput,
    ) -> Result<ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProviderError>;
}

#[derive(Debug, Error)]
pub enum RegulationRequirementCandidateExtractionProviderError {
    #[error("regulation requirement candidate extractor is not configured")]
    NotConfigured,
    #[error("regulation requirement candidate extractor configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("regulation requirement candidate extractor request timed out")]
    Timeout,
    #[error("regulation requirement candidate extractor transport failed")]
    Transport,
    #[error("regulation requirement candidate extractor response was malformed")]
    MalformedResponse,
    #[error("regulation requirement candidate extractor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("regulation requirement candidate extractor response exceeded size limit")]
    ResponseTooLarge,
    #[error("regulation requirement candidate extraction input is invalid")]
    InvalidInput,
}

#[async_trait]
pub trait RegulationRequirementCandidateExtractionProvider: Send + Sync {
    fn identity(&self) -> RegulationRequirementCandidateExtractionIdentity;

    async fn extract(
        &self,
        input: RegulationRequirementExtractionInput,
    ) -> Result<
        Vec<RegulationRequirementCandidateOutput>,
        RegulationRequirementCandidateExtractionProviderError,
    >;
}
