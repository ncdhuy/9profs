use std::collections::BTreeSet;

use nineprofs_common::now_ms;
use serde::Serialize;
use serde_json::json;

use super::{ResearchService, not_found, sha256_hash};
use crate::{
    CitationBindingMethod, CitationTargetBinding, ContentHash, CreateCitationTargetBinding,
    MAX_REFERENCE_RESOLUTION_CANDIDATES, ManuscriptReferenceCatalogRun, ManuscriptReferenceEntry,
    ManuscriptReferenceResolutionCandidate, ManuscriptReferenceResolutionCandidateId,
    ManuscriptReferenceResolutionEntry, ManuscriptReferenceResolutionEntryId,
    ManuscriptReferenceResolutionMatchKind, ManuscriptReferenceResolutionOutcome,
    ManuscriptReferenceResolutionRun, ManuscriptReferenceResolutionRunId,
    ManuscriptReferenceResolutionStatus, ManuscriptReferenceResolutionWrite,
    ManuscriptReferenceTargetMapping, PdfExtractionStatus, REFERENCE_RESOLVER_POLICY_VERSION,
    ResearchCaseId, ResearchError, ResearchPdfExtraction, ResearchSource, ResearchSourceId,
    ResearchSourceSnapshot, ResearchSourceSnapshotId, SourceKind,
};

#[derive(Clone, Debug)]
struct CandidateDraft {
    source_id: ResearchSourceId,
    source_snapshot_id: Option<ResearchSourceSnapshotId>,
    extraction_id: Option<crate::ResearchPdfExtractionId>,
    match_kind: ManuscriptReferenceResolutionMatchKind,
    automatic_binding_permitted: bool,
}

#[derive(Clone, Debug)]
struct EntryPlan {
    outcome: ManuscriptReferenceResolutionOutcome,
    match_kind: Option<ManuscriptReferenceResolutionMatchKind>,
    chosen_source_id: Option<ResearchSourceId>,
    chosen_source_snapshot_id: Option<ResearchSourceSnapshotId>,
    chosen_extraction_id: Option<crate::ResearchPdfExtractionId>,
    automatic_binding_permitted: bool,
    candidates: Vec<CandidateDraft>,
}

#[derive(Clone, Debug)]
enum MappingValidation {
    Valid(Vec<ManuscriptReferenceTargetMapping>),
    Invalid,
}

#[derive(Serialize)]
struct SourceState {
    sources: Vec<SourceStateItem>,
}

#[derive(Serialize)]
struct SourceStateItem {
    source: ResearchSource,
    snapshots: Vec<SourceStateSnapshot>,
}

#[derive(Serialize)]
struct SourceStateSnapshot {
    snapshot: ResearchSourceSnapshot,
    extractions: Vec<ResearchPdfExtraction>,
}

impl ResearchService {
    pub async fn resolve_manuscript_references(
        &self,
        catalog_run_id: &str,
    ) -> Result<ManuscriptReferenceResolutionRun, ResearchError> {
        let catalog_id = crate::ManuscriptReferenceCatalogRunId::parse(catalog_run_id.to_owned())?;
        let catalog = self
            .repository
            .get_manuscript_reference_catalog_run(&catalog_id)
            .await?
            .ok_or_else(|| not_found("manuscript reference catalog run", catalog_id.as_str()))?;
        if !matches!(
            catalog.status,
            crate::ManuscriptReferenceCatalogStatus::Completed
        ) {
            return Err(ResearchError::Invalid(
                "manuscript reference catalog run must be completed before resolution".to_owned(),
            ));
        }

        let entries = self
            .repository
            .list_manuscript_reference_entries(&catalog.id)
            .await?;
        let sources = self
            .repository
            .list_sources(Some(&catalog.research_case_id))
            .await?;
        let source_state_hash = self.source_state_hash(&sources).await?;
        if let Some(existing) = self
            .repository
            .get_manuscript_reference_resolution_for_catalog(
                &catalog.id,
                &catalog.catalog_hash,
                &source_state_hash,
                REFERENCE_RESOLVER_POLICY_VERSION,
            )
            .await?
        {
            return Ok(existing);
        }

        let mut resolution_entries = Vec::with_capacity(entries.len());
        let mut candidates = Vec::new();
        let mut binding_inputs = Vec::new();
        let mut plans = Vec::with_capacity(entries.len());

        for entry in &entries {
            let mapping_validation = self.validate_entry_mappings(&catalog, entry).await?;
            let plan = match mapping_validation {
                MappingValidation::Invalid => EntryPlan {
                    outcome: ManuscriptReferenceResolutionOutcome::Failed,
                    match_kind: Some(ManuscriptReferenceResolutionMatchKind::MappingIntegrity),
                    chosen_source_id: None,
                    chosen_source_snapshot_id: None,
                    chosen_extraction_id: None,
                    automatic_binding_permitted: false,
                    candidates: Vec::new(),
                },
                MappingValidation::Valid(mappings) => {
                    self.resolve_entry(entry, &sources, &mappings).await?
                }
            };
            plans.push((entry.clone(), plan));
        }

        for (entry, mut plan) in plans {
            if plan.automatic_binding_permitted {
                let mappings = match self.validate_entry_mappings(&catalog, &entry).await? {
                    MappingValidation::Valid(mappings) => mappings,
                    MappingValidation::Invalid => {
                        plan.outcome = ManuscriptReferenceResolutionOutcome::Failed;
                        plan.match_kind =
                            Some(ManuscriptReferenceResolutionMatchKind::MappingIntegrity);
                        plan.chosen_source_id = None;
                        plan.chosen_source_snapshot_id = None;
                        plan.chosen_extraction_id = None;
                        plan.automatic_binding_permitted = false;
                        plan.candidates.clear();
                        Vec::new()
                    }
                };
                if !mappings.is_empty() {
                    let binding_states = self
                        .target_binding_states(&mappings, &plan, &catalog.research_case_id)
                        .await?;
                    if binding_states
                        .iter()
                        .any(|state| matches!(state, TargetBindingState::Conflict))
                    {
                        plan.outcome =
                            ManuscriptReferenceResolutionOutcome::ConflictWithExistingBinding;
                        plan.automatic_binding_permitted = false;
                        plan.candidates.iter_mut().for_each(|candidate| {
                            candidate.automatic_binding_permitted = false;
                        });
                    } else if binding_states
                        .iter()
                        .all(|state| matches!(state, TargetBindingState::Equivalent))
                    {
                        plan.outcome = ManuscriptReferenceResolutionOutcome::AlreadyBound;
                        plan.automatic_binding_permitted = false;
                        plan.candidates.iter_mut().for_each(|candidate| {
                            candidate.automatic_binding_permitted = false;
                        });
                    } else {
                        for (mapping, state) in mappings.iter().zip(binding_states) {
                            if matches!(state, TargetBindingState::Unbound) {
                                binding_inputs.push(CreateCitationTargetBinding {
                                    research_case_id: catalog.research_case_id.clone(),
                                    citation_target_id: mapping.citation_target_id.clone(),
                                    source_id: plan
                                        .chosen_source_id
                                        .clone()
                                        .expect("automatic plan has a source"),
                                    source_snapshot_id: plan.chosen_source_snapshot_id.clone(),
                                    extraction_id: plan.chosen_extraction_id.clone(),
                                    method: CitationBindingMethod::DeterministicResolver,
                                });
                            }
                        }
                    }
                }
            }
            resolution_entries.push(ManuscriptReferenceResolutionEntry {
                id: ManuscriptReferenceResolutionEntryId::new(),
                resolution_run_id: ManuscriptReferenceResolutionRunId::new(),
                reference_entry_id: entry.id,
                outcome: plan.outcome,
                match_kind: plan.match_kind,
                chosen_source_id: plan.chosen_source_id,
                chosen_source_snapshot_id: plan.chosen_source_snapshot_id,
                chosen_extraction_id: plan.chosen_extraction_id,
                automatic_binding_permitted: plan.automatic_binding_permitted,
                candidate_count: plan.candidates.len() as u32,
            });
            let resolution_entry_id = resolution_entries
                .last()
                .expect("resolution entry was just pushed")
                .id
                .clone();
            candidates.extend(plan.candidates.into_iter().enumerate().map(
                |(ordinal, candidate)| ManuscriptReferenceResolutionCandidate {
                    id: ManuscriptReferenceResolutionCandidateId::new(),
                    resolution_entry_id: resolution_entry_id.clone(),
                    ordinal: ordinal as u32,
                    source_id: candidate.source_id,
                    source_snapshot_id: candidate.source_snapshot_id,
                    extraction_id: candidate.extraction_id,
                    match_kind: candidate.match_kind,
                    automatic_binding_permitted: candidate.automatic_binding_permitted,
                },
            ));
        }

        let resolution_run_id = ManuscriptReferenceResolutionRunId::new();
        for entry in &mut resolution_entries {
            entry.resolution_run_id = resolution_run_id.clone();
        }
        for candidate in &mut candidates {
            let entry = resolution_entries
                .iter()
                .find(|entry| entry.id == candidate.resolution_entry_id)
                .expect("candidate entry was just created");
            candidate.resolution_entry_id = entry.id.clone();
        }

        let binding_values = self
            .prepare_citation_target_bindings(binding_inputs)
            .await?;
        let created_at_ms = now_ms();
        let resolved_entry_count = resolution_entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.outcome,
                    ManuscriptReferenceResolutionOutcome::ResolvedExact
                        | ManuscriptReferenceResolutionOutcome::AlreadyBound
                )
            })
            .count() as u32;
        let candidate_entry_count = resolution_entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.outcome,
                    ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation
                        | ManuscriptReferenceResolutionOutcome::AmbiguousSource
                        | ManuscriptReferenceResolutionOutcome::AmbiguousSnapshotOrExtraction
                        | ManuscriptReferenceResolutionOutcome::SourceMatchedButNotVerificationReady
                )
            })
            .count() as u32;
        let unresolved_entry_count = resolution_entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.outcome,
                    ManuscriptReferenceResolutionOutcome::Unresolved
                        | ManuscriptReferenceResolutionOutcome::Failed
                )
            })
            .count() as u32;
        let conflict_entry_count = resolution_entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.outcome,
                    ManuscriptReferenceResolutionOutcome::ConflictWithExistingBinding
                )
            })
            .count() as u32;
        let run = ManuscriptReferenceResolutionRun {
            id: resolution_run_id,
            research_case_id: catalog.research_case_id.clone(),
            catalog_run_id: catalog.id,
            catalog_hash: catalog.catalog_hash,
            source_state_hash,
            resolver_policy_version: REFERENCE_RESOLVER_POLICY_VERSION.to_owned(),
            status: ManuscriptReferenceResolutionStatus::Completed,
            entry_count: resolution_entries.len() as u32,
            resolved_entry_count,
            candidate_entry_count,
            unresolved_entry_count,
            conflict_entry_count,
            created_at_ms,
            completed_at_ms: Some(now_ms()),
            failure_code: None,
        };
        let resolution_write = ManuscriptReferenceResolutionWrite {
            run,
            entries: resolution_entries,
            candidates,
        };
        let resolution_run_id = resolution_write.run.id.clone();
        let (persisted, created_bindings) = self
            .repository
            .persist_manuscript_reference_resolution_with_bindings(
                &resolution_write,
                &binding_values,
            )
            .await?;
        for binding in created_bindings {
            self.publish_citation_target_bound(&binding);
        }
        if persisted.id == resolution_run_id {
            self.publish(
                "research.manuscriptReferenceResolutionCompleted",
                json!({
                    "resolution_run_id": persisted.id,
                    "research_case_id": persisted.research_case_id,
                    "catalog_run_id": persisted.catalog_run_id,
                    "status": persisted.status,
                    "entry_count": persisted.entry_count,
                    "resolved_entry_count": persisted.resolved_entry_count,
                    "candidate_entry_count": persisted.candidate_entry_count,
                    "unresolved_entry_count": persisted.unresolved_entry_count,
                    "conflict_entry_count": persisted.conflict_entry_count,
                }),
            );
        }
        Ok(persisted)
    }

    pub async fn get_manuscript_reference_resolution(
        &self,
        id: &str,
    ) -> Result<ManuscriptReferenceResolutionRun, ResearchError> {
        let id = ManuscriptReferenceResolutionRunId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_reference_resolution_run(&id)
            .await?
            .ok_or_else(|| not_found("manuscript reference resolution run", id.as_str()))
    }

    pub async fn list_manuscript_reference_resolution_entries(
        &self,
        run_id: &str,
    ) -> Result<Vec<ManuscriptReferenceResolutionEntry>, ResearchError> {
        let id = ManuscriptReferenceResolutionRunId::parse(run_id.to_owned())?;
        self.get_manuscript_reference_resolution(id.as_str())
            .await?;
        self.repository
            .list_manuscript_reference_resolution_entries(&id)
            .await
    }

    pub async fn list_manuscript_reference_resolution_candidates(
        &self,
        entry_id: &str,
    ) -> Result<Vec<ManuscriptReferenceResolutionCandidate>, ResearchError> {
        let id = ManuscriptReferenceResolutionEntryId::parse(entry_id.to_owned())?;
        self.repository
            .get_manuscript_reference_resolution_entry(&id)
            .await?
            .ok_or_else(|| not_found("manuscript reference resolution entry", id.as_str()))?;
        self.repository
            .list_manuscript_reference_resolution_candidates(&id)
            .await
    }

    pub async fn confirm_manuscript_reference_candidate(
        &self,
        run_id: &str,
        entry_id: &str,
        candidate_id: &str,
    ) -> Result<Vec<CitationTargetBinding>, ResearchError> {
        let run_id = ManuscriptReferenceResolutionRunId::parse(run_id.to_owned())?;
        let entry_id = ManuscriptReferenceResolutionEntryId::parse(entry_id.to_owned())?;
        let candidate_id =
            ManuscriptReferenceResolutionCandidateId::parse(candidate_id.to_owned())?;
        let run = self
            .repository
            .get_manuscript_reference_resolution_run(&run_id)
            .await?
            .ok_or_else(|| not_found("manuscript reference resolution run", run_id.as_str()))?;
        if !matches!(run.status, ManuscriptReferenceResolutionStatus::Completed) {
            return Err(ResearchError::Invalid(
                "only a completed resolution run can be confirmed".to_owned(),
            ));
        }
        let resolution_entry = self
            .repository
            .get_manuscript_reference_resolution_entry(&entry_id)
            .await?
            .ok_or_else(|| not_found("manuscript reference resolution entry", entry_id.as_str()))?;
        if resolution_entry.resolution_run_id != run.id {
            return Err(ResearchError::Invalid(
                "resolution entry does not belong to resolution run".to_owned(),
            ));
        }
        let candidate = self
            .repository
            .get_manuscript_reference_resolution_candidate(&candidate_id)
            .await?
            .ok_or_else(|| {
                not_found(
                    "manuscript reference resolution candidate",
                    candidate_id.as_str(),
                )
            })?;
        if candidate.resolution_entry_id != resolution_entry.id {
            return Err(ResearchError::Invalid(
                "resolution candidate does not belong to resolution entry".to_owned(),
            ));
        }
        let catalog = self
            .repository
            .get_manuscript_reference_catalog_run(&run.catalog_run_id)
            .await?
            .ok_or_else(|| {
                not_found(
                    "manuscript reference catalog run",
                    run.catalog_run_id.as_str(),
                )
            })?;
        if catalog.research_case_id != run.research_case_id {
            return Err(ResearchError::Invalid(
                "resolution run case does not match catalog case".to_owned(),
            ));
        }
        let entry = self
            .repository
            .get_manuscript_reference_entry(&resolution_entry.reference_entry_id)
            .await?
            .ok_or_else(|| {
                not_found(
                    "manuscript reference entry",
                    resolution_entry.reference_entry_id.as_str(),
                )
            })?;
        if entry.catalog_run_id != catalog.id {
            return Err(ResearchError::Invalid(
                "resolution entry does not belong to catalog run".to_owned(),
            ));
        }
        let mappings = match self.validate_entry_mappings(&catalog, &entry).await? {
            MappingValidation::Valid(mappings) => mappings,
            MappingValidation::Invalid => {
                return Err(ResearchError::Invalid(
                    "reference entry target mappings are invalid".to_owned(),
                ));
            }
        };

        let mut existing_bindings = Vec::new();
        let mut inputs = Vec::new();
        for mapping in &mappings {
            match self
                .repository
                .latest_citation_target_binding(&mapping.citation_target_id)
                .await?
            {
                Some(binding)
                    if binding_matches_candidate(&binding, &candidate, &run.research_case_id) =>
                {
                    existing_bindings.push(binding);
                }
                Some(_) => {
                    return Err(ResearchError::Invalid(
                        "candidate confirmation conflicts with existing citation binding"
                            .to_owned(),
                    ));
                }
                None => inputs.push(CreateCitationTargetBinding {
                    research_case_id: run.research_case_id.clone(),
                    citation_target_id: mapping.citation_target_id.clone(),
                    source_id: candidate.source_id.clone(),
                    source_snapshot_id: candidate.source_snapshot_id.clone(),
                    extraction_id: candidate.extraction_id.clone(),
                    method: CitationBindingMethod::Human,
                }),
            }
        }
        let mut created = if inputs.is_empty() {
            Vec::new()
        } else {
            self.create_citation_target_bindings(inputs).await?
        };
        existing_bindings.append(&mut created);
        Ok(existing_bindings)
    }

    async fn source_state_hash(
        &self,
        sources: &[ResearchSource],
    ) -> Result<ContentHash, ResearchError> {
        let mut state = Vec::with_capacity(sources.len());
        for source in sources {
            let mut snapshots = self.repository.list_snapshots(Some(&source.id)).await?;
            snapshots.sort_by_key(|snapshot| snapshot.id.as_str().to_owned());
            let mut state_snapshots = Vec::with_capacity(snapshots.len());
            for snapshot in snapshots {
                let mut extractions = if matches!(source.kind, SourceKind::ReferencePdf) {
                    self.repository.list_pdf_extractions(&snapshot.id).await?
                } else {
                    Vec::new()
                };
                extractions.sort_by_key(|extraction| extraction.id.as_str().to_owned());
                state_snapshots.push(SourceStateSnapshot {
                    snapshot,
                    extractions,
                });
            }
            state.push(SourceStateItem {
                source: source.clone(),
                snapshots: state_snapshots,
            });
        }
        state.sort_by_key(|item| item.source.id.as_str().to_owned());
        Ok(sha256_hash(&serde_json::to_vec(&SourceState {
            sources: state,
        })?))
    }

    async fn validate_entry_mappings(
        &self,
        catalog: &ManuscriptReferenceCatalogRun,
        entry: &ManuscriptReferenceEntry,
    ) -> Result<MappingValidation, ResearchError> {
        if entry.catalog_run_id != catalog.id {
            return Ok(MappingValidation::Invalid);
        }
        let mappings = self
            .repository
            .list_manuscript_reference_target_mappings(&entry.id)
            .await?;
        if mappings.len() != entry.target_count as usize {
            return Ok(MappingValidation::Invalid);
        }
        let mut target_ids = BTreeSet::new();
        for mapping in &mappings {
            if mapping.catalog_run_id != catalog.id
                || mapping.reference_entry_id != entry.id
                || !target_ids.insert(mapping.citation_target_id.as_str().to_owned())
            {
                return Ok(MappingValidation::Invalid);
            }
            let Some(target) = self
                .repository
                .get_citation_target(&mapping.citation_target_id)
                .await?
            else {
                return Ok(MappingValidation::Invalid);
            };
            if target.citation_occurrence_id != mapping.citation_occurrence_id
                || target.ordinal != mapping.document_target_ordinal
                || target.reference_key != entry.reference_key
            {
                return Ok(MappingValidation::Invalid);
            }
            let Some(occurrence) = self
                .repository
                .get_citation_occurrence(&mapping.citation_occurrence_id)
                .await?
            else {
                return Ok(MappingValidation::Invalid);
            };
            if occurrence.research_case_id != catalog.research_case_id {
                return Ok(MappingValidation::Invalid);
            }
        }
        Ok(MappingValidation::Valid(mappings))
    }

    async fn resolve_entry(
        &self,
        entry: &ManuscriptReferenceEntry,
        sources: &[ResearchSource],
        mappings: &[ManuscriptReferenceTargetMapping],
    ) -> Result<EntryPlan, ResearchError> {
        let mut exact_sources = sources
            .iter()
            .filter_map(|source| exact_source_match(entry, source).map(|kind| (source, kind)))
            .collect::<Vec<_>>();
        exact_sources.sort_by_key(|(source, _)| source.id.as_str().to_owned());
        if exact_sources.len() > MAX_REFERENCE_RESOLUTION_CANDIDATES {
            return Ok(failed_plan());
        }
        if exact_sources.len() > 1 {
            let candidates = exact_sources
                .into_iter()
                .map(|(source, match_kind)| CandidateDraft {
                    source_id: source.id.clone(),
                    source_snapshot_id: None,
                    extraction_id: None,
                    match_kind,
                    automatic_binding_permitted: false,
                })
                .collect();
            return Ok(candidate_plan(
                ManuscriptReferenceResolutionOutcome::AmbiguousSource,
                candidates,
            ));
        }
        if let Some((source, match_kind)) = exact_sources.into_iter().next() {
            let mut plan = self.exact_source_plan(source, match_kind).await?;
            if plan.automatic_binding_permitted && mappings.is_empty() {
                plan.automatic_binding_permitted = false;
                plan.outcome = ManuscriptReferenceResolutionOutcome::AlreadyBound;
            }
            return Ok(plan);
        }

        let mut candidates = sources
            .iter()
            .filter_map(|source| {
                weak_source_match(entry, source).map(|match_kind| CandidateDraft {
                    source_id: source.id.clone(),
                    source_snapshot_id: None,
                    extraction_id: None,
                    match_kind,
                    automatic_binding_permitted: false,
                })
            })
            .collect::<Vec<_>>();
        sort_candidates(&mut candidates);
        if candidates.len() > MAX_REFERENCE_RESOLUTION_CANDIDATES {
            return Ok(failed_plan());
        }
        let outcome = match candidates.len() {
            0 => ManuscriptReferenceResolutionOutcome::Unresolved,
            1 => ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation,
            _ => ManuscriptReferenceResolutionOutcome::AmbiguousSource,
        };
        Ok(candidate_plan(outcome, candidates))
    }

    async fn exact_source_plan(
        &self,
        source: &ResearchSource,
        match_kind: ManuscriptReferenceResolutionMatchKind,
    ) -> Result<EntryPlan, ResearchError> {
        if !matches!(source.kind, SourceKind::ReferencePdf) {
            let candidate = CandidateDraft {
                source_id: source.id.clone(),
                source_snapshot_id: None,
                extraction_id: None,
                match_kind: match_kind.clone(),
                automatic_binding_permitted: true,
            };
            return Ok(EntryPlan {
                outcome: ManuscriptReferenceResolutionOutcome::ResolvedExact,
                match_kind: Some(match_kind),
                chosen_source_id: Some(source.id.clone()),
                chosen_source_snapshot_id: None,
                chosen_extraction_id: None,
                automatic_binding_permitted: true,
                candidates: vec![candidate],
            });
        }

        let mut snapshots = self.repository.list_snapshots(Some(&source.id)).await?;
        snapshots.sort_by_key(|snapshot| snapshot.id.as_str().to_owned());
        let mut ready = Vec::new();
        let mut not_ready = Vec::new();
        for snapshot in snapshots {
            let mut extractions = self.repository.list_pdf_extractions(&snapshot.id).await?;
            extractions.sort_by_key(|extraction| extraction.id.as_str().to_owned());
            if extractions.is_empty() {
                not_ready.push(CandidateDraft {
                    source_id: source.id.clone(),
                    source_snapshot_id: Some(snapshot.id),
                    extraction_id: None,
                    match_kind: match_kind.clone(),
                    automatic_binding_permitted: false,
                });
                continue;
            }
            for extraction in extractions {
                let candidate = CandidateDraft {
                    source_id: source.id.clone(),
                    source_snapshot_id: Some(snapshot.id.clone()),
                    extraction_id: Some(extraction.id.clone()),
                    match_kind: match_kind.clone(),
                    automatic_binding_permitted: matches!(
                        extraction.status,
                        PdfExtractionStatus::Ready
                    ),
                };
                if matches!(extraction.status, PdfExtractionStatus::Ready) {
                    ready.push(candidate);
                } else {
                    not_ready.push(candidate);
                }
            }
        }
        sort_candidates(&mut ready);
        sort_candidates(&mut not_ready);
        if ready.len() > MAX_REFERENCE_RESOLUTION_CANDIDATES
            || not_ready.len() > MAX_REFERENCE_RESOLUTION_CANDIDATES
        {
            return Ok(failed_plan());
        }
        if ready.len() == 1 {
            let candidate = ready[0].clone();
            return Ok(EntryPlan {
                outcome: ManuscriptReferenceResolutionOutcome::ResolvedExact,
                match_kind: Some(match_kind),
                chosen_source_id: Some(candidate.source_id.clone()),
                chosen_source_snapshot_id: candidate.source_snapshot_id.clone(),
                chosen_extraction_id: candidate.extraction_id.clone(),
                automatic_binding_permitted: true,
                candidates: vec![candidate],
            });
        }
        if ready.len() > 1 {
            return Ok(candidate_plan(
                ManuscriptReferenceResolutionOutcome::AmbiguousSnapshotOrExtraction,
                ready,
            ));
        }
        if not_ready.is_empty() {
            not_ready.push(CandidateDraft {
                source_id: source.id.clone(),
                source_snapshot_id: None,
                extraction_id: None,
                match_kind: match_kind.clone(),
                automatic_binding_permitted: false,
            });
        }
        Ok(candidate_plan(
            ManuscriptReferenceResolutionOutcome::SourceMatchedButNotVerificationReady,
            not_ready,
        ))
    }

    async fn target_binding_states(
        &self,
        mappings: &[ManuscriptReferenceTargetMapping],
        plan: &EntryPlan,
        case_id: &ResearchCaseId,
    ) -> Result<Vec<TargetBindingState>, ResearchError> {
        let mut states = Vec::with_capacity(mappings.len());
        for mapping in mappings {
            states.push(
                match self
                    .repository
                    .latest_citation_target_binding(&mapping.citation_target_id)
                    .await?
                {
                    Some(binding) if binding_matches_plan(&binding, plan, case_id) => {
                        TargetBindingState::Equivalent
                    }
                    Some(_) => TargetBindingState::Conflict,
                    None => TargetBindingState::Unbound,
                },
            );
        }
        Ok(states)
    }
}

#[derive(Clone, Copy, Debug)]
enum TargetBindingState {
    Unbound,
    Equivalent,
    Conflict,
}

fn candidate_plan(
    outcome: ManuscriptReferenceResolutionOutcome,
    mut candidates: Vec<CandidateDraft>,
) -> EntryPlan {
    sort_candidates(&mut candidates);
    let chosen = if candidates.len() == 1 {
        Some(candidates[0].clone())
    } else {
        None
    };
    EntryPlan {
        outcome,
        match_kind: chosen
            .as_ref()
            .map(|candidate| candidate.match_kind.clone()),
        chosen_source_id: chosen.as_ref().map(|candidate| candidate.source_id.clone()),
        chosen_source_snapshot_id: chosen
            .as_ref()
            .and_then(|candidate| candidate.source_snapshot_id.clone()),
        chosen_extraction_id: chosen
            .as_ref()
            .and_then(|candidate| candidate.extraction_id.clone()),
        automatic_binding_permitted: false,
        candidates,
    }
}

fn failed_plan() -> EntryPlan {
    EntryPlan {
        outcome: ManuscriptReferenceResolutionOutcome::Failed,
        match_kind: None,
        chosen_source_id: None,
        chosen_source_snapshot_id: None,
        chosen_extraction_id: None,
        automatic_binding_permitted: false,
        candidates: Vec::new(),
    }
}

fn exact_source_match(
    entry: &ManuscriptReferenceEntry,
    source: &ResearchSource,
) -> Option<ManuscriptReferenceResolutionMatchKind> {
    let identity = source.identity.as_ref()?;
    if !identity.provider.eq_ignore_ascii_case("zotero") {
        return None;
    }
    if entry.zotero_item_id.as_deref() == Some(identity.external_reference.as_str()) {
        return Some(ManuscriptReferenceResolutionMatchKind::ExactZoteroItemId);
    }
    if entry
        .zotero_uris
        .iter()
        .any(|uri| uri == &identity.external_reference)
    {
        return Some(ManuscriptReferenceResolutionMatchKind::ExactZoteroUri);
    }
    None
}

fn weak_source_match(
    entry: &ManuscriptReferenceEntry,
    source: &ResearchSource,
) -> Option<ManuscriptReferenceResolutionMatchKind> {
    let label = normalize_text(&source.label);
    if normalize_text(&entry.reference_key) == label {
        return Some(ManuscriptReferenceResolutionMatchKind::ReferenceKeySourceLabel);
    }
    if entry
        .word_title
        .as_deref()
        .map(normalize_text)
        .is_some_and(|title| title == label)
    {
        return Some(ManuscriptReferenceResolutionMatchKind::ReferenceTitleSourceLabel);
    }
    None
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn sort_candidates(candidates: &mut [CandidateDraft]) {
    candidates.sort_by_key(|candidate| {
        (
            candidate.source_id.as_str().to_owned(),
            candidate
                .source_snapshot_id
                .as_ref()
                .map(|id| id.as_str().to_owned())
                .unwrap_or_default(),
            candidate
                .extraction_id
                .as_ref()
                .map(|id| id.as_str().to_owned())
                .unwrap_or_default(),
            match_kind_rank(&candidate.match_kind),
        )
    });
}

fn match_kind_rank(kind: &ManuscriptReferenceResolutionMatchKind) -> u8 {
    match kind {
        ManuscriptReferenceResolutionMatchKind::ExactZoteroItemId => 0,
        ManuscriptReferenceResolutionMatchKind::ExactZoteroUri => 1,
        ManuscriptReferenceResolutionMatchKind::ReferenceKeySourceLabel => 2,
        ManuscriptReferenceResolutionMatchKind::ReferenceTitleSourceLabel => 3,
        ManuscriptReferenceResolutionMatchKind::MappingIntegrity => 4,
    }
}

fn binding_matches_plan(
    binding: &CitationTargetBinding,
    plan: &EntryPlan,
    case_id: &ResearchCaseId,
) -> bool {
    binding.research_case_id == *case_id
        && binding.source_id
            == plan
                .chosen_source_id
                .clone()
                .expect("automatic plan has a source")
        && binding.source_snapshot_id == plan.chosen_source_snapshot_id
        && binding.extraction_id == plan.chosen_extraction_id
}

fn binding_matches_candidate(
    binding: &CitationTargetBinding,
    candidate: &ManuscriptReferenceResolutionCandidate,
    case_id: &ResearchCaseId,
) -> bool {
    binding.research_case_id == *case_id
        && binding.source_id == candidate.source_id
        && binding.source_snapshot_id == candidate.source_snapshot_id
        && binding.extraction_id == candidate.extraction_id
}
