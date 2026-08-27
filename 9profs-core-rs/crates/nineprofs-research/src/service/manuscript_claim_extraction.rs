use super::ResearchService;
use super::{not_found, sha256_hash};
use crate::{
    CitationOccurrenceId, CitationOccurrenceOrigin, ClaimCitationLink, ClaimOrigin, ContentHash,
    EvidenceLocator, ExtractManuscriptClaims, HashAlgorithm, MAX_CLAIM_EXTRACTION_BLOCKS,
    MAX_CLAIM_EXTRACTION_CITATIONS_PER_BLOCK, MAX_CLAIM_EXTRACTION_CONTEXT_BYTES,
    MAX_CLAIM_TEXT_BYTES, MAX_PROVENANCE_TEXT_BYTES, ManuscriptCitationSyncRunId,
    ManuscriptCitationSyncStatus, ManuscriptClaimExtractionBlockInput,
    ManuscriptClaimExtractionClaimOutput, ManuscriptClaimExtractionCoverage,
    ManuscriptClaimExtractionCoverageId, ManuscriptClaimExtractionCoverageStatus,
    ManuscriptClaimExtractionIdentity, ManuscriptClaimExtractionItem,
    ManuscriptClaimExtractionItemId, ManuscriptClaimExtractionRun, ManuscriptClaimExtractionRunId,
    ManuscriptClaimExtractionStatus, ManuscriptClaimExtractionWrite, ResearchClaim,
    ResearchClaimId, ResearchError, ResearchRepository, bounded_text,
};
use nineprofs_common::now_ms;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

impl ResearchService {
    pub async fn extract_manuscript_claims(
        &self,
        input: ExtractManuscriptClaims,
    ) -> Result<ManuscriptClaimExtractionRun, ResearchError> {
        let Some(provider) = self.claim_extractor.clone() else {
            return Err(ResearchError::ManuscriptClaimExtractorNotConfigured);
        };
        let sync_run = self
            .get_manuscript_citation_sync(input.citation_sync_run_id.as_str())
            .await?;
        if !matches!(sync_run.status, ManuscriptCitationSyncStatus::Completed)
            || sync_run.document_id != input.document_id
            || sync_run.document_version != input.document_version
        {
            return Err(ResearchError::ManuscriptClaimExtractionStale);
        }
        bounded_text(
            "claim extraction document ID",
            &input.document_id,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        if input.document_version < 0 {
            return Err(ResearchError::Invalid(
                "document version must not be negative".to_owned(),
            ));
        }
        if input.blocks.len() > MAX_CLAIM_EXTRACTION_BLOCKS {
            return Err(ResearchError::Invalid(format!(
                "claim extraction cannot contain more than {MAX_CLAIM_EXTRACTION_BLOCKS} blocks"
            )));
        }

        let sync_occurrences = self
            .repository
            .list_manuscript_citation_sync_occurrences(&sync_run.id)
            .await?;
        let mut expected = BTreeMap::new();
        for sync_occurrence in sync_occurrences {
            let citation = self
                .repository
                .get_citation_occurrence(&sync_occurrence.citation_occurrence_id)
                .await?
                .ok_or_else(|| {
                    not_found(
                        "citation occurrence",
                        sync_occurrence.citation_occurrence_id.as_str(),
                    )
                })?;
            let expected_origin = CitationOccurrenceOrigin::Manuscript {
                document_id: sync_run.document_id.clone(),
                document_version: sync_run.document_version.to_string(),
                locator: Some(EvidenceLocator::Manuscript {
                    block_id: sync_occurrence.document_block_id.clone(),
                    start: Some(sync_occurrence.start),
                    end: Some(sync_occurrence.end),
                }),
            };
            if citation.research_case_id != sync_run.research_case_id
                || citation.origin != expected_origin
            {
                return Err(ResearchError::Invalid(
                    "citation occurrence does not belong to completed citation sync".to_owned(),
                ));
            }
            if expected
                .insert(citation.id.to_string(), (sync_occurrence, citation))
                .is_some()
            {
                return Err(ResearchError::Invalid(
                    "citation sync contains duplicate citation occurrence".to_owned(),
                ));
            }
        }

        let mut seen_blocks = BTreeSet::new();
        let mut seen_citations = BTreeSet::new();
        let mut context_bytes = 0usize;
        for block in &input.blocks {
            bounded_text(
                "claim extraction block ID",
                &block.block_id,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
            bounded_text(
                "claim extraction block text",
                &block.text,
                MAX_CLAIM_EXTRACTION_CONTEXT_BYTES,
            )?;
            if !seen_blocks.insert(block.block_id.clone()) {
                return Err(ResearchError::Invalid(
                    "claim extraction contains duplicate block ID".to_owned(),
                ));
            }
            if block.citations.is_empty() {
                return Err(ResearchError::Invalid(
                    "claim extraction blocks must contain citations".to_owned(),
                ));
            }
            if block.citations.len() > MAX_CLAIM_EXTRACTION_CITATIONS_PER_BLOCK {
                return Err(ResearchError::Invalid(format!(
                    "claim extraction block cannot contain more than {MAX_CLAIM_EXTRACTION_CITATIONS_PER_BLOCK} citations"
                )));
            }
            let block_len = block.text.chars().count() as u64;
            for citation in &block.citations {
                if citation.start >= citation.end || citation.end > block_len {
                    return Err(ResearchError::Invalid(
                        "claim extraction citation range is outside block text".to_owned(),
                    ));
                }
                let Some((sync_occurrence, canonical)) =
                    expected.get(&citation.citation_occurrence_id)
                else {
                    return Err(ResearchError::Invalid(
                        "claim extraction references unknown citation occurrence".to_owned(),
                    ));
                };
                if sync_occurrence.document_block_id != block.block_id
                    || sync_occurrence.start != citation.start
                    || sync_occurrence.end != citation.end
                    || canonical.rendered_text != citation.rendered_text
                {
                    return Err(ResearchError::Invalid(
                        "claim extraction citation metadata does not match citation sync"
                            .to_owned(),
                    ));
                }
                if !seen_citations.insert(citation.citation_occurrence_id.clone()) {
                    return Err(ResearchError::Invalid(
                        "claim extraction contains duplicate citation occurrence".to_owned(),
                    ));
                }
            }
            context_bytes = context_bytes
                .checked_add(serde_json::to_vec(block)?.len())
                .ok_or_else(|| {
                    ResearchError::Invalid("claim extraction context is too large".to_owned())
                })?;
        }
        let expected_citations: BTreeSet<_> = expected.keys().cloned().collect();
        if seen_citations != expected_citations {
            return Err(ResearchError::Invalid(
                "claim extraction must include every citation occurrence from sync run".to_owned(),
            ));
        }
        if context_bytes > MAX_CLAIM_EXTRACTION_CONTEXT_BYTES {
            return Err(ResearchError::Invalid(format!(
                "claim extraction context exceeds {MAX_CLAIM_EXTRACTION_CONTEXT_BYTES} bytes"
            )));
        }

        let identity = provider.identity();
        validate_extractor_identity(&identity)?;
        let context_hash = ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: sha256_hex(&serde_json::to_vec(&(
                &input.document_id,
                input.document_version,
                &input.blocks,
            ))?),
        };
        if let Some(existing) = self
            .repository
            .find_completed_manuscript_claim_extraction(
                &sync_run.id,
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
        let run_id = ManuscriptClaimExtractionRunId::new();
        let base_run = || ManuscriptClaimExtractionRun {
            id: run_id.clone(),
            research_case_id: sync_run.research_case_id.clone(),
            manuscript_source_id: sync_run.manuscript_source_id.clone(),
            citation_sync_run_id: sync_run.id.clone(),
            document_id: sync_run.document_id.clone(),
            document_version: sync_run.document_version,
            context_hash: context_hash.clone(),
            extractor_provider: identity.provider.clone(),
            extractor_version: identity.extractor_version.clone(),
            extractor_model_id: identity.model_id.clone(),
            extraction_contract_version: identity.extraction_contract_version.clone(),
            status: ManuscriptClaimExtractionStatus::Failed,
            claim_count: 0,
            created_at_ms: timestamp,
            completed_at_ms: Some(timestamp),
            failure_code: None,
        };

        self.publish(
            "research.manuscriptClaimExtractionStarted",
            json!({
                "extraction_run_id": run_id.clone(),
                "research_case_id": sync_run.research_case_id.clone(),
                "citation_sync_run_id": sync_run.id.clone(),
                "status": "running",
            }),
        );

        let mut outputs = Vec::with_capacity(input.blocks.len());
        for block in &input.blocks {
            let output = match provider.extract(block.clone()).await {
                Ok(output) => output,
                Err(crate::ManuscriptClaimExtractionProviderError::NotConfigured) => {
                    return Err(ResearchError::ManuscriptClaimExtractorNotConfigured);
                }
                Err(crate::ManuscriptClaimExtractionProviderError::InvalidConfiguration(
                    reason,
                )) => {
                    return Err(ResearchError::ManuscriptClaimExtractorInvalidConfiguration(
                        reason,
                    ));
                }
                Err(error) => {
                    let mut failed = base_run();
                    failed.failure_code = Some(claim_extraction_failure_code(&error).to_owned());
                    self.repository
                        .persist_manuscript_claim_extraction(&ManuscriptClaimExtractionWrite {
                            run: failed.clone(),
                            claims: Vec::new(),
                            links: Vec::new(),
                            items: Vec::new(),
                            coverage: Vec::new(),
                        })
                        .await?;
                    self.publish(
                        "research.manuscriptClaimExtractionFailed",
                        json!({
                            "extraction_run_id": failed.id,
                            "research_case_id": failed.research_case_id,
                            "citation_sync_run_id": failed.citation_sync_run_id,
                            "status": failed.status,
                        }),
                    );
                    return Err(ResearchError::ManuscriptClaimExtractionFailed(
                        failed
                            .failure_code
                            .unwrap_or_else(|| "provider_failure".to_owned()),
                    ));
                }
            };
            outputs.push((block, output));
        }

        let mut claims = Vec::new();
        let mut links = Vec::new();
        let mut items = Vec::new();
        let mut coverage = Vec::with_capacity(expected.len());
        let mut associated = BTreeSet::new();
        let mut unassociated = BTreeMap::new();
        for (block, output) in outputs {
            for unassociated_citation in output.unassociated_citations {
                if !block.citations.iter().any(|citation| {
                    citation.citation_occurrence_id == unassociated_citation.citation_occurrence_id
                }) {
                    return Err(ResearchError::Invalid(
                        "claim extraction coverage references citation outside block".to_owned(),
                    ));
                }
                bounded_text(
                    "claim extraction coverage reason",
                    &unassociated_citation.reason,
                    MAX_PROVENANCE_TEXT_BYTES,
                )?;
                if unassociated
                    .insert(
                        unassociated_citation.citation_occurrence_id,
                        unassociated_citation.reason,
                    )
                    .is_some()
                {
                    return Err(ResearchError::Invalid(
                        "claim extraction contains duplicate coverage".to_owned(),
                    ));
                }
            }
            for claim_output in output.claims {
                validate_claim_output(&block, &claim_output)?;
                let excerpt = codepoint_slice(
                    &block.text,
                    claim_output.source_start,
                    claim_output.source_end,
                )?;
                bounded_text("claim source excerpt", &excerpt, MAX_CLAIM_TEXT_BYTES)?;
                let locator = EvidenceLocator::Manuscript {
                    block_id: block.block_id.clone(),
                    start: Some(claim_output.source_start),
                    end: Some(claim_output.source_end),
                };
                let claim_id = ResearchClaimId::new();
                let claim = ResearchClaim {
                    id: claim_id.clone(),
                    research_case_id: sync_run.research_case_id.clone(),
                    text: claim_output.claim_text,
                    origin: ClaimOrigin::Manuscript {
                        document_id: sync_run.document_id.clone(),
                        document_version: sync_run.document_version.to_string(),
                        locator: Some(locator),
                    },
                    created_at_ms: timestamp,
                };
                claim.origin.validate()?;
                claims.push(claim);
                let extraction_item_id = ManuscriptClaimExtractionItemId::new();
                for citation_id in claim_output.citation_occurrence_ids {
                    associated.insert(citation_id.clone());
                    let claim_citation_link_id = crate::ClaimCitationLinkId::new();
                    let citation_occurrence_id = CitationOccurrenceId::parse(citation_id)?;
                    links.push(ClaimCitationLink {
                        id: claim_citation_link_id.clone(),
                        research_case_id: sync_run.research_case_id.clone(),
                        claim_id: claim_id.clone(),
                        citation_occurrence_id: citation_occurrence_id.clone(),
                        created_at_ms: timestamp,
                    });
                    coverage.push(ManuscriptClaimExtractionCoverage {
                        id: ManuscriptClaimExtractionCoverageId::new(),
                        extraction_run_id: run_id.clone(),
                        extraction_item_id: Some(extraction_item_id.clone()),
                        claim_citation_link_id: Some(claim_citation_link_id),
                        citation_occurrence_id,
                        status: ManuscriptClaimExtractionCoverageStatus::AssociatedWithClaim,
                        reason: None,
                    });
                }
                items.push(ManuscriptClaimExtractionItem {
                    id: extraction_item_id,
                    extraction_run_id: run_id.clone(),
                    research_claim_id: claim_id,
                    document_block_id: block.block_id.clone(),
                    source_start: claim_output.source_start,
                    source_end: claim_output.source_end,
                    source_excerpt: excerpt.clone(),
                    source_excerpt_hash: ContentHash {
                        algorithm: HashAlgorithm::Sha256,
                        value: sha256_hex(excerpt.as_bytes()),
                    },
                    ordinal: items.len() as u32,
                });
            }
        }

        for citation_id in expected.keys() {
            if associated.contains(citation_id) {
                continue;
            }
            coverage.push(ManuscriptClaimExtractionCoverage {
                id: ManuscriptClaimExtractionCoverageId::new(),
                extraction_run_id: run_id.clone(),
                extraction_item_id: None,
                claim_citation_link_id: None,
                citation_occurrence_id: CitationOccurrenceId::parse(citation_id.clone())?,
                status: ManuscriptClaimExtractionCoverageStatus::NoVerifiableClaim,
                reason: unassociated.remove(citation_id),
            });
        }
        if !unassociated.is_empty() {
            return Err(ResearchError::Invalid(
                "claim extraction coverage contains unknown citation occurrence".to_owned(),
            ));
        }

        let mut run = base_run();
        run.status = ManuscriptClaimExtractionStatus::Completed;
        run.claim_count = claims.len() as u32;
        run.failure_code = None;
        let result = self
            .repository
            .persist_manuscript_claim_extraction(&ManuscriptClaimExtractionWrite {
                run,
                claims,
                links,
                items,
                coverage,
            })
            .await?;
        self.publish(
            "research.manuscriptClaimExtractionCompleted",
            json!({
                "extraction_run_id": result.id,
                "research_case_id": result.research_case_id,
                "citation_sync_run_id": result.citation_sync_run_id,
                "claim_count": result.claim_count,
                "status": result.status,
            }),
        );
        Ok(result)
    }

    pub async fn get_manuscript_claim_extraction(
        &self,
        id: &str,
    ) -> Result<ManuscriptClaimExtractionRun, ResearchError> {
        let id = ManuscriptClaimExtractionRunId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_claim_extraction_run(&id)
            .await?
            .ok_or_else(|| not_found("manuscript claim extraction run", id.as_str()))
    }

    pub async fn list_manuscript_claim_extractions(
        &self,
        citation_sync_run_id: Option<&str>,
    ) -> Result<Vec<ManuscriptClaimExtractionRun>, ResearchError> {
        let sync_run_id = citation_sync_run_id
            .map(|id| ManuscriptCitationSyncRunId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_manuscript_claim_extraction_runs(sync_run_id.as_ref())
            .await
    }

    pub async fn list_manuscript_claim_extraction_items(
        &self,
        extraction_run_id: &str,
    ) -> Result<Vec<ManuscriptClaimExtractionItem>, ResearchError> {
        let run = self
            .get_manuscript_claim_extraction(extraction_run_id)
            .await?;
        self.repository
            .list_manuscript_claim_extraction_items(&run.id)
            .await
    }

    pub async fn list_manuscript_claim_extraction_coverage(
        &self,
        extraction_run_id: &str,
    ) -> Result<Vec<ManuscriptClaimExtractionCoverage>, ResearchError> {
        let run = self
            .get_manuscript_claim_extraction(extraction_run_id)
            .await?;
        self.repository
            .list_manuscript_claim_extraction_coverage(&run.id)
            .await
    }
}

fn sha256_hex(value: &[u8]) -> String {
    sha256_hash(value).value
}

fn validate_extractor_identity(
    identity: &ManuscriptClaimExtractionIdentity,
) -> Result<(), ResearchError> {
    bounded_text(
        "claim extractor provider",
        &identity.provider,
        MAX_PROVENANCE_TEXT_BYTES,
    )?;
    bounded_text(
        "claim extractor version",
        &identity.extractor_version,
        MAX_PROVENANCE_TEXT_BYTES,
    )?;
    bounded_text(
        "claim extraction contract version",
        &identity.extraction_contract_version,
        MAX_PROVENANCE_TEXT_BYTES,
    )?;
    if identity.provider.trim().is_empty()
        || identity.extractor_version.trim().is_empty()
        || identity.extraction_contract_version.trim().is_empty()
    {
        return Err(ResearchError::ManuscriptClaimExtractorInvalidConfiguration(
            "provider, version, and contract version are required".to_owned(),
        ));
    }
    if let Some(model_id) = &identity.model_id {
        bounded_text("claim extractor model", model_id, MAX_PROVENANCE_TEXT_BYTES)?;
    }
    Ok(())
}

fn validate_claim_output(
    block: &ManuscriptClaimExtractionBlockInput,
    output: &ManuscriptClaimExtractionClaimOutput,
) -> Result<(), ResearchError> {
    bounded_text("normalized claim", &output.claim_text, MAX_CLAIM_TEXT_BYTES)?;
    if output.claim_text.trim().is_empty()
        || output
            .claim_text
            .chars()
            .any(|character| character.is_control() && !character.is_whitespace())
    {
        return Err(ResearchError::Invalid(
            "claim extraction returned invalid claim text".to_owned(),
        ));
    }
    let block_len = block.text.chars().count() as u64;
    if output.source_start >= output.source_end || output.source_end > block_len {
        return Err(ResearchError::Invalid(
            "claim extraction source range is outside block text".to_owned(),
        ));
    }
    let mut citation_ids = BTreeSet::new();
    for citation_id in &output.citation_occurrence_ids {
        if !citation_ids.insert(citation_id)
            || !block
                .citations
                .iter()
                .any(|citation| &citation.citation_occurrence_id == citation_id)
        {
            return Err(ResearchError::Invalid(
                "claim extraction references unknown or duplicate citation occurrence".to_owned(),
            ));
        }
    }
    if citation_ids.is_empty() {
        return Err(ResearchError::Invalid(
            "every extracted claim must have a citation occurrence".to_owned(),
        ));
    }
    for citation in &block.citations {
        if citation_ids.contains(&citation.citation_occurrence_id)
            && output.source_start < citation.end
            && citation.start < output.source_end
        {
            return Err(ResearchError::Invalid(
                "claim extraction source range overlaps citation atom".to_owned(),
            ));
        }
    }
    Ok(())
}

fn codepoint_slice(text: &str, start: u64, end: u64) -> Result<String, ResearchError> {
    let start = usize::try_from(start)
        .map_err(|_| ResearchError::Invalid("claim source range is too large".to_owned()))?;
    let end = usize::try_from(end)
        .map_err(|_| ResearchError::Invalid("claim source range is too large".to_owned()))?;
    if start >= end {
        return Err(ResearchError::Invalid(
            "claim source range must have start < end".to_owned(),
        ));
    }
    let mut offsets = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    if end >= offsets.len() || start >= offsets.len() {
        return Err(ResearchError::Invalid(
            "claim source range exceeds block text".to_owned(),
        ));
    }
    Ok(text[offsets[start]..offsets[end]].to_owned())
}

fn claim_extraction_failure_code(
    error: &crate::ManuscriptClaimExtractionProviderError,
) -> &'static str {
    match error {
        crate::ManuscriptClaimExtractionProviderError::Timeout => "timeout",
        crate::ManuscriptClaimExtractionProviderError::Transport => "transport_failure",
        crate::ManuscriptClaimExtractionProviderError::MalformedResponse => "malformed_response",
        crate::ManuscriptClaimExtractionProviderError::InvalidStructuredOutput => {
            "invalid_structured_output"
        }
        crate::ManuscriptClaimExtractionProviderError::ResponseTooLarge => "response_too_large",
        crate::ManuscriptClaimExtractionProviderError::NotConfigured => "extractor_not_configured",
        crate::ManuscriptClaimExtractionProviderError::InvalidConfiguration(_) => {
            "invalid_configuration"
        }
    }
}
