use super::ResearchService;
use super::{not_found, sha256_hash};
use crate::{
    CitationOccurrence, CitationOccurrenceId, CitationOccurrenceOrigin, CitationTarget,
    CitationTargetId, EvidenceLocator, MAX_CITATION_MARKER_BYTES, MAX_CITATION_REFERENCE_KEY_BYTES,
    MAX_CITATION_TARGETS_PER_OCCURRENCE, MAX_CITED_LOCATOR_BYTES,
    MAX_MANUSCRIPT_CITATION_OCCURRENCES, MAX_PROVENANCE_TEXT_BYTES,
    ManuscriptCitationSyncOccurrence, ManuscriptCitationSyncOccurrenceId,
    ManuscriptCitationSyncRun, ManuscriptCitationSyncStatus, ManuscriptCitationSyncTarget,
    ManuscriptCitationSyncWrite, ResearchCaseId, ResearchError, ResearchRepository,
    ResearchSourceId, SourceKind, SyncManuscriptCitations, bounded_text,
};
use nineprofs_common::now_ms;
use serde_json::json;
use std::collections::BTreeSet;

impl ResearchService {
    pub async fn sync_manuscript_citations(
        &self,
        input: SyncManuscriptCitations,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        let source = self
            .repository
            .get_source(&input.manuscript_source_id)
            .await?
            .ok_or_else(|| not_found("source", input.manuscript_source_id.as_str()))?;
        if source.research_case_id != input.research_case_id {
            return Err(ResearchError::Invalid(
                "manuscript source must belong to same research case".to_owned(),
            ));
        }
        if !matches!(&source.kind, SourceKind::Manuscript) {
            return Err(ResearchError::Invalid(
                "manuscript citation sync requires a Manuscript source".to_owned(),
            ));
        }
        if input.document_version < 0 {
            return Err(ResearchError::Invalid(
                "document version must not be negative".to_owned(),
            ));
        }
        CitationOccurrenceOrigin::Manuscript {
            document_id: input.document_id.clone(),
            document_version: input.document_version.to_string(),
            locator: None,
        }
        .validate()?;
        if input.citations.len() > MAX_MANUSCRIPT_CITATION_OCCURRENCES {
            return Err(ResearchError::Invalid(format!(
                "manuscript citation inventory cannot contain more than {MAX_MANUSCRIPT_CITATION_OCCURRENCES} occurrences"
            )));
        }

        let inventory_hash = sha256_hash(&serde_json::to_vec(&input.citations)?);
        let timestamp = now_ms();
        let run_id = crate::ManuscriptCitationSyncRunId::new();
        let mut citation_occurrences = Vec::with_capacity(input.citations.len());
        let mut citation_targets = Vec::new();
        let mut sync_occurrences = Vec::with_capacity(input.citations.len());
        let mut sync_targets = Vec::new();

        for (ordinal, citation) in input.citations.iter().enumerate() {
            bounded_text(
                "citation marker",
                &citation.rendered_text,
                MAX_CITATION_MARKER_BYTES,
            )?;
            bounded_text(
                "document block id",
                &citation.block_id,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
            if citation.start >= citation.end {
                return Err(ResearchError::Invalid(
                    "manuscript citation locator must have start < end".to_owned(),
                ));
            }
            let locator = EvidenceLocator::Manuscript {
                block_id: citation.block_id.clone(),
                start: Some(citation.start),
                end: Some(citation.end),
            };
            locator.validate()?;
            let occurrence_id = CitationOccurrenceId::new();
            let occurrence = CitationOccurrence {
                id: occurrence_id.clone(),
                research_case_id: input.research_case_id.clone(),
                origin: CitationOccurrenceOrigin::Manuscript {
                    document_id: input.document_id.clone(),
                    document_version: input.document_version.to_string(),
                    locator: Some(locator),
                },
                rendered_text: citation.rendered_text.clone(),
                created_at_ms: timestamp,
            };
            occurrence.origin.validate()?;
            citation_occurrences.push(occurrence);

            let sync_occurrence_id = crate::ManuscriptCitationSyncOccurrenceId::new();
            sync_occurrences.push(ManuscriptCitationSyncOccurrence {
                id: sync_occurrence_id.clone(),
                sync_run_id: run_id.clone(),
                ordinal: ordinal as u32,
                citation_occurrence_id: occurrence_id.clone(),
                document_block_id: citation.block_id.clone(),
                start: citation.start,
                end: citation.end,
                format: citation.format.clone(),
            });

            let mut target_ordinals = BTreeSet::new();
            for target in &citation.targets {
                if !target_ordinals.insert(target.ordinal) {
                    return Err(ResearchError::Invalid(
                        "manuscript citation target ordinals must be unique".to_owned(),
                    ));
                }
                if target_ordinals.len() > MAX_CITATION_TARGETS_PER_OCCURRENCE {
                    return Err(ResearchError::Invalid(format!(
                        "citation occurrence cannot contain more than {MAX_CITATION_TARGETS_PER_OCCURRENCE} targets"
                    )));
                }
                bounded_text(
                    "citation reference key",
                    &target.reference_key,
                    MAX_CITATION_REFERENCE_KEY_BYTES,
                )?;
                if let Some(cited_locator) = &target.cited_locator {
                    bounded_text("cited locator", cited_locator, MAX_CITED_LOCATOR_BYTES)?;
                }
                let target_id = CitationTargetId::new();
                citation_targets.push(CitationTarget {
                    id: target_id.clone(),
                    citation_occurrence_id: occurrence_id.clone(),
                    ordinal: target.ordinal,
                    reference_key: target.reference_key.clone(),
                    cited_locator: target.cited_locator.clone(),
                });
                sync_targets.push(ManuscriptCitationSyncTarget {
                    id: crate::ManuscriptCitationSyncTargetId::new(),
                    sync_occurrence_id: sync_occurrence_id.clone(),
                    document_target_ordinal: target.ordinal,
                    citation_target_id: target_id,
                });
            }
        }

        let run = ManuscriptCitationSyncRun {
            id: run_id,
            research_case_id: input.research_case_id,
            manuscript_source_id: input.manuscript_source_id,
            document_id: input.document_id,
            document_version: input.document_version,
            inventory_hash,
            status: ManuscriptCitationSyncStatus::Completed,
            occurrence_count: citation_occurrences.len() as u32,
            created_at_ms: timestamp,
            completed_at_ms: Some(timestamp),
            failure_code: None,
        };
        let result = self
            .repository
            .persist_manuscript_citation_sync(&ManuscriptCitationSyncWrite {
                run,
                citation_occurrences,
                citation_targets,
                sync_occurrences,
                sync_targets,
            })
            .await?;
        self.publish(
            "research.manuscriptCitationSyncCompleted",
            json!({
                "sync_run_id": result.id,
                "research_case_id": result.research_case_id,
                "manuscript_source_id": result.manuscript_source_id,
                "document_id": result.document_id,
                "document_version": result.document_version,
                "occurrence_count": result.occurrence_count,
                "status": result.status,
            }),
        );
        Ok(result)
    }

    pub async fn get_manuscript_citation_sync(
        &self,
        id: &str,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        let id = crate::ManuscriptCitationSyncRunId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_citation_sync(&id)
            .await?
            .ok_or_else(|| not_found("manuscript citation sync run", id.as_str()))
    }

    pub async fn latest_manuscript_citation_sync(
        &self,
        research_case_id: &str,
        manuscript_source_id: &str,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        let case_id = ResearchCaseId::parse(research_case_id.to_owned())?;
        let source_id = ResearchSourceId::parse(manuscript_source_id.to_owned())?;
        self.ensure_case(&case_id).await?;
        let source = self
            .repository
            .get_source(&source_id)
            .await?
            .ok_or_else(|| not_found("source", source_id.as_str()))?;
        if source.research_case_id != case_id {
            return Err(ResearchError::Invalid(
                "manuscript source must belong to same research case".to_owned(),
            ));
        }
        if !matches!(&source.kind, SourceKind::Manuscript) {
            return Err(ResearchError::Invalid(
                "manuscript citation sync requires a Manuscript source".to_owned(),
            ));
        }
        self.repository
            .latest_manuscript_citation_sync(&case_id, &source_id)
            .await?
            .ok_or_else(|| not_found("manuscript citation sync run", source_id.as_str()))
    }

    pub async fn list_manuscript_citation_sync_occurrences(
        &self,
        sync_run_id: &str,
    ) -> Result<Vec<ManuscriptCitationSyncOccurrence>, ResearchError> {
        let run_id = crate::ManuscriptCitationSyncRunId::parse(sync_run_id.to_owned())?;
        self.get_manuscript_citation_sync(run_id.as_str()).await?;
        self.repository
            .list_manuscript_citation_sync_occurrences(&run_id)
            .await
    }

    pub async fn get_manuscript_citation_sync_occurrence(
        &self,
        id: &str,
    ) -> Result<ManuscriptCitationSyncOccurrence, ResearchError> {
        let id = ManuscriptCitationSyncOccurrenceId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_citation_sync_occurrence(&id)
            .await?
            .ok_or_else(|| not_found("manuscript citation sync occurrence", id.as_str()))
    }

    pub async fn list_manuscript_citation_sync_targets(
        &self,
        sync_occurrence_id: &str,
    ) -> Result<Vec<ManuscriptCitationSyncTarget>, ResearchError> {
        let occurrence_id =
            ManuscriptCitationSyncOccurrenceId::parse(sync_occurrence_id.to_owned())?;
        self.get_manuscript_citation_sync_occurrence(occurrence_id.as_str())
            .await?;
        self.repository
            .list_manuscript_citation_sync_targets(&occurrence_id)
            .await
    }
}
