use super::{ResearchService, not_found, sha256_hash};
use crate::{
    ClaimReviewKind, MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_CONTRACT_VERSION,
    MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_SCOPE, MAX_CLAIM_TEXT_BYTES,
    MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCK_TEXT_BYTES, MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCKS,
    MAX_MANUSCRIPT_CLAIM_INVENTORY_CITATIONS_PER_BLOCK,
    MAX_MANUSCRIPT_CLAIM_INVENTORY_CLAIMS_PER_BLOCK, MAX_MANUSCRIPT_CLAIM_INVENTORY_CONTEXT_BYTES,
    ManuscriptClaimInventoryBlockInput, ManuscriptClaimInventoryCoverage,
    ManuscriptClaimInventoryCoverageId, ManuscriptClaimInventoryCoverageStatus,
    ManuscriptClaimInventoryIdentity, ManuscriptClaimInventoryItem, ManuscriptClaimInventoryItemId,
    ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProviderError,
    ManuscriptClaimInventoryRun, ManuscriptClaimInventoryRunId, ManuscriptClaimInventoryStatus,
    ManuscriptClaimInventoryWrite, ResearchError, ResearchRepository, SourceKind,
    StartManuscriptClaimInventory, bounded_text,
};
use nineprofs_common::now_ms;
use serde_json::json;
use std::collections::BTreeSet;

const COVERAGE_LIMITATIONS: &[&str] = &[
    "tables",
    "textboxes",
    "footnotes",
    "endnotes",
    "headers",
    "footers",
    "equations",
    "captions",
    "cross_block_propositions",
];

impl ResearchService {
    pub async fn start_manuscript_claim_inventory(
        &self,
        input: StartManuscriptClaimInventory,
    ) -> Result<ManuscriptClaimInventoryRun, ResearchError> {
        let Some(provider) = self.claim_inventory_extractor.clone() else {
            return Err(ResearchError::ManuscriptClaimInventoryExtractorNotConfigured);
        };
        self.ensure_case(&input.research_case_id).await?;
        let source = self
            .repository
            .get_source(&input.manuscript_source_id)
            .await?
            .ok_or_else(|| not_found("research source", input.manuscript_source_id.as_str()))?;
        if source.research_case_id != input.research_case_id
            || source.kind != SourceKind::Manuscript
        {
            return Err(ResearchError::Invalid(
                "manuscript source must belong to case and have manuscript kind".to_owned(),
            ));
        }
        bounded_text(
            "claim inventory document ID",
            &input.document_id,
            crate::MAX_PROVENANCE_TEXT_BYTES,
        )?;
        if input.document_version < 0 {
            return Err(ResearchError::Invalid(
                "document version must not be negative".to_owned(),
            ));
        }
        validate_blocks(&input.blocks)?;
        let identity = provider.identity();
        validate_inventory_identity(&identity)?;
        let context_hash = sha256_hash(&serde_json::to_vec(&(
            &input.document_id,
            input.document_version,
            &input.blocks,
        ))?);
        if let Some(existing) = self
            .repository
            .find_completed_manuscript_claim_inventory(
                &input.research_case_id,
                &input.manuscript_source_id,
                &input.document_id,
                input.document_version,
                &context_hash,
                &identity.provider,
                &identity.extractor_version,
                identity.model_id.as_deref(),
                &identity.extraction_contract_version,
            )
            .await?
        {
            return Ok(existing);
        }

        let timestamp = now_ms();
        let run_id = ManuscriptClaimInventoryRunId::new();
        let base_run = || ManuscriptClaimInventoryRun {
            id: run_id.clone(),
            research_case_id: input.research_case_id.clone(),
            manuscript_source_id: input.manuscript_source_id.clone(),
            document_id: input.document_id.clone(),
            document_version: input.document_version,
            document_context_hash: context_hash.clone(),
            extractor_provider: identity.provider.clone(),
            extractor_version: identity.extractor_version.clone(),
            extractor_model_id: identity.model_id.clone(),
            extraction_contract_version: identity.extraction_contract_version.clone(),
            coverage_contract_version: MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_CONTRACT_VERSION
                .to_owned(),
            coverage_scope: MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_SCOPE.to_owned(),
            coverage_limitations: COVERAGE_LIMITATIONS
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            status: ManuscriptClaimInventoryStatus::Failed,
            item_count: 0,
            covered_block_count: input.blocks.len() as u32,
            created_at_ms: timestamp,
            completed_at_ms: Some(timestamp),
            failure_code: None,
        };

        let mut outputs = Vec::with_capacity(input.blocks.len());
        for block in &input.blocks {
            let output = match provider.extract(block.clone()).await {
                Ok(output) => output,
                Err(error) => {
                    let code = inventory_failure_code(&error);
                    self.persist_failed_inventory(base_run(), code).await?;
                    return Err(ResearchError::ManuscriptClaimInventoryFailed(
                        code.to_owned(),
                    ));
                }
            };
            if output.claims.len() > MAX_MANUSCRIPT_CLAIM_INVENTORY_CLAIMS_PER_BLOCK {
                self.persist_failed_inventory(base_run(), "claims_per_block_limit")
                    .await?;
                return Err(ResearchError::ManuscriptClaimInventoryFailed(
                    "claims_per_block_limit".to_owned(),
                ));
            }
            outputs.push((block, output));
        }

        let mut items = Vec::new();
        let mut coverage = Vec::with_capacity(input.blocks.len());
        let mut seen = BTreeSet::new();
        for (block, output) in outputs {
            let before = items.len();
            for claim in output.claims {
                let excerpt = match validate_claim_output(block, &claim) {
                    Ok(excerpt) => excerpt,
                    Err(_) => {
                        self.persist_failed_inventory(base_run(), "invalid_structured_output")
                            .await?;
                        return Err(ResearchError::ManuscriptClaimInventoryFailed(
                            "invalid_structured_output".to_owned(),
                        ));
                    }
                };
                let duplicate_key = (
                    block.block_id.clone(),
                    claim.source_start,
                    claim.source_end,
                    claim.claim_text.clone(),
                );
                if !seen.insert(duplicate_key) {
                    continue;
                }
                let overlapping_citation_count = block
                    .citations
                    .iter()
                    .filter(|citation| {
                        claim.source_start < citation.end && citation.start < claim.source_end
                    })
                    .count() as u32;
                items.push(ManuscriptClaimInventoryItem {
                    id: ManuscriptClaimInventoryItemId::new(),
                    inventory_run_id: run_id.clone(),
                    ordinal: items.len() as u32,
                    document_block_id: block.block_id.clone(),
                    block_ordinal: block.block_ordinal,
                    block_kind: block.block_kind.clone(),
                    source_start: claim.source_start,
                    source_end: claim.source_end,
                    source_excerpt: excerpt.clone(),
                    source_excerpt_hash: sha256_hash(excerpt.as_bytes()),
                    claim_text: claim.claim_text,
                    review_kind: claim.review_kind,
                    overlapping_citation_count,
                });
            }
            coverage.push(ManuscriptClaimInventoryCoverage {
                id: ManuscriptClaimInventoryCoverageId::new(),
                inventory_run_id: run_id.clone(),
                document_block_id: block.block_id.clone(),
                block_ordinal: block.block_ordinal,
                block_kind: block.block_kind.clone(),
                status: if items.len() == before {
                    ManuscriptClaimInventoryCoverageStatus::NoClaims
                } else {
                    ManuscriptClaimInventoryCoverageStatus::Processed
                },
                reason: (items.len() == before).then(|| "no_claims_returned".to_owned()),
            });
        }

        let mut run = base_run();
        run.status = ManuscriptClaimInventoryStatus::Completed;
        run.item_count = items.len() as u32;
        run.failure_code = None;
        let result = self
            .repository
            .persist_manuscript_claim_inventory(&ManuscriptClaimInventoryWrite {
                run,
                items,
                coverage,
            })
            .await?;
        self.publish(
            "research.manuscriptClaimInventoryCompleted",
            json!({
                "inventory_run_id": result.id,
                "research_case_id": result.research_case_id,
                "manuscript_source_id": result.manuscript_source_id,
                "document_id": result.document_id,
                "document_version": result.document_version,
                "item_count": result.item_count,
                "covered_block_count": result.covered_block_count,
                "status": result.status,
            }),
        );
        Ok(result)
    }

    async fn persist_failed_inventory(
        &self,
        mut run: ManuscriptClaimInventoryRun,
        code: &str,
    ) -> Result<(), ResearchError> {
        run.status = ManuscriptClaimInventoryStatus::Failed;
        run.item_count = 0;
        run.failure_code = Some(code.to_owned());
        let result = self
            .repository
            .persist_manuscript_claim_inventory(&ManuscriptClaimInventoryWrite {
                run,
                items: Vec::new(),
                coverage: Vec::new(),
            })
            .await?;
        self.publish(
            "research.manuscriptClaimInventoryFailed",
            json!({
                "inventory_run_id": result.id,
                "research_case_id": result.research_case_id,
                "manuscript_source_id": result.manuscript_source_id,
                "document_id": result.document_id,
                "document_version": result.document_version,
                "status": result.status,
                "failure_code": result.failure_code,
            }),
        );
        Ok(())
    }

    pub async fn get_manuscript_claim_inventory(
        &self,
        id: &str,
    ) -> Result<ManuscriptClaimInventoryRun, ResearchError> {
        let id = ManuscriptClaimInventoryRunId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_claim_inventory_run(&id)
            .await?
            .ok_or_else(|| not_found("manuscript claim inventory run", id.as_str()))
    }

    pub async fn list_manuscript_claim_inventory_items(
        &self,
        inventory_run_id: &str,
    ) -> Result<Vec<ManuscriptClaimInventoryItem>, ResearchError> {
        let run = self
            .get_manuscript_claim_inventory(inventory_run_id)
            .await?;
        self.repository
            .list_manuscript_claim_inventory_items(&run.id)
            .await
    }

    pub async fn list_manuscript_claim_inventory_coverage(
        &self,
        inventory_run_id: &str,
    ) -> Result<Vec<ManuscriptClaimInventoryCoverage>, ResearchError> {
        let run = self
            .get_manuscript_claim_inventory(inventory_run_id)
            .await?;
        self.repository
            .list_manuscript_claim_inventory_coverage(&run.id)
            .await
    }
}

fn validate_blocks(blocks: &[ManuscriptClaimInventoryBlockInput]) -> Result<(), ResearchError> {
    if blocks.len() > MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCKS {
        return Err(ResearchError::Invalid(format!(
            "claim inventory cannot contain more than {MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCKS} blocks"
        )));
    }
    let mut seen_ids = BTreeSet::new();
    let mut previous_ordinal = None;
    let mut context_bytes = 0usize;
    for block in blocks {
        bounded_text(
            "claim inventory block ID",
            &block.block_id,
            crate::MAX_PROVENANCE_TEXT_BYTES,
        )?;
        bounded_text(
            "claim inventory block text",
            &block.text,
            MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCK_TEXT_BYTES,
        )?;
        if !seen_ids.insert(block.block_id.clone()) {
            return Err(ResearchError::Invalid(
                "claim inventory contains duplicate block ID".to_owned(),
            ));
        }
        if previous_ordinal.is_some_and(|previous| block.block_ordinal <= previous) {
            return Err(ResearchError::Invalid(
                "claim inventory block ordinals must be strictly increasing".to_owned(),
            ));
        }
        previous_ordinal = Some(block.block_ordinal);
        if block.citations.len() > MAX_MANUSCRIPT_CLAIM_INVENTORY_CITATIONS_PER_BLOCK {
            return Err(ResearchError::Invalid(format!(
                "claim inventory block cannot contain more than {MAX_MANUSCRIPT_CLAIM_INVENTORY_CITATIONS_PER_BLOCK} citations"
            )));
        }
        let block_len = block.text.chars().count() as u64;
        for citation in &block.citations {
            if citation.start >= citation.end || citation.end > block_len {
                return Err(ResearchError::Invalid(
                    "claim inventory citation range is outside block text".to_owned(),
                ));
            }
            bounded_text(
                "claim inventory citation rendered text",
                &citation.rendered_text,
                crate::MAX_PROVENANCE_TEXT_BYTES,
            )?;
        }
        context_bytes = context_bytes
            .checked_add(serde_json::to_vec(block)?.len())
            .ok_or_else(|| {
                ResearchError::Invalid("claim inventory context is too large".to_owned())
            })?;
    }
    if context_bytes > MAX_MANUSCRIPT_CLAIM_INVENTORY_CONTEXT_BYTES {
        return Err(ResearchError::Invalid(format!(
            "claim inventory context exceeds {MAX_MANUSCRIPT_CLAIM_INVENTORY_CONTEXT_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_inventory_identity(
    identity: &ManuscriptClaimInventoryIdentity,
) -> Result<(), ResearchError> {
    bounded_text(
        "claim inventory provider",
        &identity.provider,
        crate::MAX_PROVENANCE_TEXT_BYTES,
    )?;
    bounded_text(
        "claim inventory extractor version",
        &identity.extractor_version,
        crate::MAX_PROVENANCE_TEXT_BYTES,
    )?;
    bounded_text(
        "claim inventory extraction contract version",
        &identity.extraction_contract_version,
        crate::MAX_PROVENANCE_TEXT_BYTES,
    )?;
    if let Some(model_id) = &identity.model_id {
        bounded_text(
            "claim inventory extractor model",
            model_id,
            crate::MAX_PROVENANCE_TEXT_BYTES,
        )?;
    }
    Ok(())
}

fn validate_claim_output(
    block: &ManuscriptClaimInventoryBlockInput,
    claim: &crate::ManuscriptClaimInventoryClaimOutput,
) -> Result<String, ResearchError> {
    bounded_text(
        "normalized inventory claim",
        &claim.claim_text,
        MAX_CLAIM_TEXT_BYTES,
    )?;
    if claim
        .claim_text
        .chars()
        .any(|character| character.is_control() && !character.is_whitespace())
    {
        return Err(ResearchError::Invalid(
            "claim inventory returned invalid claim text".to_owned(),
        ));
    }
    let block_len = block.text.chars().count() as u64;
    if claim.source_start >= claim.source_end || claim.source_end > block_len {
        return Err(ResearchError::Invalid(
            "claim inventory source range is outside block text".to_owned(),
        ));
    }
    let excerpt = codepoint_slice(&block.text, claim.source_start, claim.source_end)?;
    bounded_text(
        "claim inventory source excerpt",
        &excerpt,
        MAX_CLAIM_TEXT_BYTES,
    )?;
    Ok(excerpt)
}

fn codepoint_slice(text: &str, start: u64, end: u64) -> Result<String, ResearchError> {
    let start = usize::try_from(start).map_err(|_| {
        ResearchError::Invalid("claim inventory source range is too large".to_owned())
    })?;
    let end = usize::try_from(end).map_err(|_| {
        ResearchError::Invalid("claim inventory source range is too large".to_owned())
    })?;
    let mut offsets = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    if start >= end || end >= offsets.len() {
        return Err(ResearchError::Invalid(
            "claim inventory source range exceeds block text".to_owned(),
        ));
    }
    Ok(text[offsets[start]..offsets[end]].to_owned())
}

fn inventory_failure_code(error: &ManuscriptClaimInventoryProviderError) -> &'static str {
    match error {
        ManuscriptClaimInventoryProviderError::Timeout => "timeout",
        ManuscriptClaimInventoryProviderError::Transport => "transport_failure",
        ManuscriptClaimInventoryProviderError::MalformedResponse => "malformed_response",
        ManuscriptClaimInventoryProviderError::InvalidStructuredOutput => {
            "invalid_structured_output"
        }
        ManuscriptClaimInventoryProviderError::ResponseTooLarge => "response_too_large",
        ManuscriptClaimInventoryProviderError::NotConfigured => "extractor_not_configured",
        ManuscriptClaimInventoryProviderError::InvalidConfiguration(_) => "invalid_configuration",
    }
}
