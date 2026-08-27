use super::ResearchService;
use super::{not_found, sha256_hash};
use crate::{
    CitationOccurrenceId, CitationTargetId, ContentHash, MAX_CITATION_REFERENCE_KEY_BYTES,
    MAX_CITATION_TARGETS_PER_OCCURRENCE, MAX_MANUSCRIPT_CITATION_OCCURRENCES,
    MAX_MANUSCRIPT_REFERENCE_CATALOG_BYTES, MAX_MANUSCRIPT_REFERENCE_CATALOG_ENTRIES,
    MAX_MANUSCRIPT_REFERENCE_CATALOG_TARGETS, MAX_MANUSCRIPT_REFERENCE_URI_BYTES,
    MAX_MANUSCRIPT_REFERENCE_URI_COUNT, MAX_PROVENANCE_TEXT_BYTES, ManuscriptCitationFormat,
    ManuscriptCitationSyncRunId, ManuscriptCitationSyncStatus, ManuscriptReferenceCatalogRun,
    ManuscriptReferenceCatalogStatus, ManuscriptReferenceCatalogTargetInput,
    ManuscriptReferenceCatalogWrite, ManuscriptReferenceEntry, ManuscriptReferenceTargetMapping,
    ResearchCaseId, ResearchError, ResearchRepository, ResearchSourceId,
    SyncManuscriptReferenceCatalog, bounded_text,
};
use nineprofs_common::now_ms;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

impl ResearchService {
    pub async fn sync_manuscript_reference_catalog(
        &self,
        input: SyncManuscriptReferenceCatalog,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
        let sync_run = self
            .repository
            .get_manuscript_citation_sync(&input.citation_sync_run_id)
            .await?
            .ok_or_else(|| {
                not_found(
                    "manuscript citation sync run",
                    input.citation_sync_run_id.as_str(),
                )
            })?;
        if !matches!(sync_run.status, ManuscriptCitationSyncStatus::Completed) {
            return Err(ResearchError::Invalid(
                "reference catalog requires a completed citation sync run".to_owned(),
            ));
        }
        if sync_run.document_id != input.document_id
            || sync_run.document_version != input.document_version
        {
            return Err(ResearchError::ManuscriptReferenceCatalogStale);
        }
        bounded_text(
            "reference catalog document id",
            &input.document_id,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        let serialized_input = serde_json::to_vec(&input)?;
        if serialized_input.len() > MAX_MANUSCRIPT_REFERENCE_CATALOG_BYTES {
            return Err(ResearchError::Invalid(format!(
                "reference catalog exceeds {MAX_MANUSCRIPT_REFERENCE_CATALOG_BYTES} bytes"
            )));
        }
        if input.citations.len() > MAX_MANUSCRIPT_CITATION_OCCURRENCES {
            return Err(ResearchError::Invalid(format!(
                "reference catalog cannot contain more than {MAX_MANUSCRIPT_CITATION_OCCURRENCES} occurrences"
            )));
        }

        let sync_occurrences = self
            .repository
            .list_manuscript_citation_sync_occurrences(&sync_run.id)
            .await?;
        let mut occurrences_by_citation_id = BTreeMap::new();
        let mut expected_targets = BTreeMap::new();
        for occurrence in &sync_occurrences {
            if occurrence.sync_run_id != sync_run.id {
                return Err(ResearchError::Invalid(
                    "citation sync occurrence belongs to another sync run".to_owned(),
                ));
            }
            if occurrences_by_citation_id
                .insert(occurrence.citation_occurrence_id.to_string(), occurrence)
                .is_some()
            {
                return Err(ResearchError::Invalid(
                    "citation sync contains duplicate citation occurrences".to_owned(),
                ));
            }
            for sync_target in self
                .repository
                .list_manuscript_citation_sync_targets(&occurrence.id)
                .await?
            {
                if sync_target.sync_occurrence_id != occurrence.id {
                    return Err(ResearchError::Invalid(
                        "citation sync target belongs to another occurrence".to_owned(),
                    ));
                }
                let target = self
                    .repository
                    .get_citation_target(&sync_target.citation_target_id)
                    .await?
                    .ok_or_else(|| {
                        not_found("citation target", sync_target.citation_target_id.as_str())
                    })?;
                if target.citation_occurrence_id != occurrence.citation_occurrence_id
                    || target.ordinal != sync_target.document_target_ordinal
                {
                    return Err(ResearchError::Invalid(
                        "citation sync target mapping is inconsistent".to_owned(),
                    ));
                }
                if expected_targets
                    .insert(target.id.to_string(), (occurrence, sync_target, target))
                    .is_some()
                {
                    return Err(ResearchError::Invalid(
                        "citation sync contains duplicate citation targets".to_owned(),
                    ));
                }
            }
        }
        if sync_run.occurrence_count as usize != sync_occurrences.len() {
            return Err(ResearchError::Invalid(
                "citation sync occurrence count is inconsistent".to_owned(),
            ));
        }

        let mut seen_occurrences = BTreeSet::new();
        let mut seen_targets = BTreeSet::new();
        let mut entries_by_identity = BTreeMap::<(String, String), ReferenceEntryDraft>::new();
        let mut mapping_drafts = Vec::new();
        let mut target_mapping_count = 0usize;
        for citation in &input.citations {
            let citation_occurrence_id =
                CitationOccurrenceId::parse(citation.citation_occurrence_id.clone())?;
            let occurrence = occurrences_by_citation_id
                .get(citation_occurrence_id.as_str())
                .ok_or_else(|| {
                    ResearchError::Invalid(
                        "reference catalog citation occurrence is outside sync run".to_owned(),
                    )
                })?;
            if !seen_occurrences.insert(citation_occurrence_id.to_string()) {
                return Err(ResearchError::Invalid(
                    "reference catalog contains duplicate citation occurrences".to_owned(),
                ));
            }
            if occurrence.document_block_id != citation.block_id
                || occurrence.start != citation.start
                || occurrence.end != citation.end
                || occurrence.format != citation.format
            {
                return Err(ResearchError::Invalid(
                    "reference catalog citation does not match sync occurrence".to_owned(),
                ));
            }
            bounded_text(
                "reference catalog block id",
                &citation.block_id,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
            if citation.start >= citation.end {
                return Err(ResearchError::Invalid(
                    "reference catalog locator must have start < end".to_owned(),
                ));
            }
            if citation.targets.len() > MAX_CITATION_TARGETS_PER_OCCURRENCE {
                return Err(ResearchError::Invalid(format!(
                    "reference catalog occurrence cannot contain more than {MAX_CITATION_TARGETS_PER_OCCURRENCE} targets"
                )));
            }
            for target_input in &citation.targets {
                target_mapping_count = target_mapping_count.checked_add(1).ok_or_else(|| {
                    ResearchError::Invalid("reference catalog target count overflow".to_owned())
                })?;
                if target_mapping_count > MAX_MANUSCRIPT_REFERENCE_CATALOG_TARGETS {
                    return Err(ResearchError::Invalid(format!(
                        "reference catalog cannot contain more than {MAX_MANUSCRIPT_REFERENCE_CATALOG_TARGETS} targets"
                    )));
                }
                let citation_target_id =
                    CitationTargetId::parse(target_input.citation_target_id.clone())?;
                let (expected_occurrence, expected_sync_target, expected_target) = expected_targets
                    .get(citation_target_id.as_str())
                    .ok_or_else(|| {
                        ResearchError::Invalid(
                            "reference catalog citation target is outside sync run".to_owned(),
                        )
                    })?;
                if *expected_occurrence != *occurrence
                    || expected_target.citation_occurrence_id != citation_occurrence_id
                    || expected_sync_target.document_target_ordinal != target_input.ordinal
                    || expected_target.ordinal != target_input.ordinal
                    || expected_target.reference_key != target_input.reference_key
                {
                    return Err(ResearchError::Invalid(
                        "reference catalog target does not match sync target".to_owned(),
                    ));
                }
                if !seen_targets.insert(citation_target_id.to_string()) {
                    return Err(ResearchError::Invalid(
                        "reference catalog contains duplicate citation target mappings".to_owned(),
                    ));
                }
                let descriptor = reference_descriptor(
                    &citation.format,
                    &target_input.reference_key,
                    target_input,
                )?;
                let descriptor_hash = sha256_hash(&serde_json::to_vec(&descriptor)?);
                let identity = (descriptor.format.clone(), descriptor.reference_key.clone());
                match entries_by_identity.get_mut(&identity) {
                    Some(entry) if entry.descriptor != descriptor => {
                        return Err(ResearchError::ManuscriptReferenceDescriptorConflict {
                            format: descriptor.format,
                            reference_key: descriptor.reference_key,
                        });
                    }
                    Some(entry) => {
                        entry.target_count =
                            entry.target_count.checked_add(1).ok_or_else(|| {
                                ResearchError::Invalid(
                                    "reference entry target count overflow".to_owned(),
                                )
                            })?;
                    }
                    None => {
                        entries_by_identity.insert(
                            identity.clone(),
                            ReferenceEntryDraft {
                                descriptor,
                                descriptor_hash,
                                target_count: 1,
                            },
                        );
                    }
                }
                mapping_drafts.push(ReferenceMappingDraft {
                    identity,
                    citation_occurrence_id: citation_occurrence_id.clone(),
                    citation_target_id,
                    document_target_ordinal: target_input.ordinal,
                    occurrence_ordinal: occurrence.ordinal,
                });
            }
        }
        if seen_occurrences.len() != occurrences_by_citation_id.len()
            || seen_targets.len() != expected_targets.len()
        {
            return Err(ResearchError::Invalid(
                "reference catalog must account for every citation sync target".to_owned(),
            ));
        }
        if entries_by_identity.len() > MAX_MANUSCRIPT_REFERENCE_CATALOG_ENTRIES {
            return Err(ResearchError::Invalid(format!(
                "reference catalog cannot contain more than {MAX_MANUSCRIPT_REFERENCE_CATALOG_ENTRIES} entries"
            )));
        }

        let catalog_run_id = crate::ManuscriptReferenceCatalogRunId::new();
        let mut entries = Vec::with_capacity(entries_by_identity.len());
        let mut entry_ids = BTreeMap::new();
        let mut entry_ordinals = BTreeMap::new();
        for (ordinal, (identity, draft)) in entries_by_identity.iter().enumerate() {
            let id = crate::ManuscriptReferenceEntryId::new();
            entry_ids.insert(identity.clone(), id.clone());
            entry_ordinals.insert(identity.clone(), ordinal as u32);
            entries.push(ManuscriptReferenceEntry {
                id,
                catalog_run_id: catalog_run_id.clone(),
                ordinal: ordinal as u32,
                format: if identity.0 == "word_native" {
                    ManuscriptCitationFormat::WordNative
                } else {
                    ManuscriptCitationFormat::Zotero
                },
                reference_key: draft.descriptor.reference_key.clone(),
                descriptor_hash: draft.descriptor_hash.clone(),
                word_tag: draft.descriptor.word_tag.clone(),
                word_title: draft.descriptor.word_title.clone(),
                word_author: draft.descriptor.word_author.clone(),
                word_year: draft.descriptor.word_year.clone(),
                zotero_item_id: draft.descriptor.zotero_item_id.clone(),
                zotero_uris: draft.descriptor.zotero_uris.clone(),
                target_count: draft.target_count,
            });
        }
        mapping_drafts.sort_by_key(|mapping| {
            (
                mapping.occurrence_ordinal,
                mapping.document_target_ordinal,
                mapping.citation_target_id.to_string(),
            )
        });
        let mut hash_entries = Vec::with_capacity(entries.len());
        for entry in &entries {
            hash_entries.push(ReferenceCatalogHashEntry {
                ordinal: entry.ordinal,
                descriptor: ReferenceDescriptor {
                    format: catalog_format_key(&entry.format).to_owned(),
                    reference_key: entry.reference_key.clone(),
                    word_tag: entry.word_tag.clone(),
                    word_title: entry.word_title.clone(),
                    word_author: entry.word_author.clone(),
                    word_year: entry.word_year.clone(),
                    zotero_item_id: entry.zotero_item_id.clone(),
                    zotero_uris: entry.zotero_uris.clone(),
                },
                descriptor_hash: entry.descriptor_hash.value.clone(),
                target_count: entry.target_count,
            });
        }
        let hash_mappings = mapping_drafts
            .iter()
            .map(|mapping| ReferenceCatalogHashMapping {
                occurrence_ordinal: mapping.occurrence_ordinal,
                citation_occurrence_id: mapping.citation_occurrence_id.to_string(),
                citation_target_id: mapping.citation_target_id.to_string(),
                document_target_ordinal: mapping.document_target_ordinal,
                entry_ordinal: entry_ordinals[&mapping.identity],
            })
            .collect::<Vec<_>>();
        let catalog_hash = sha256_hash(&serde_json::to_vec(&ReferenceCatalogHashPayload {
            citation_sync_run_id: sync_run.id.to_string(),
            inventory_hash: sync_run.inventory_hash.value.clone(),
            document_id: sync_run.document_id.clone(),
            document_version: sync_run.document_version,
            entries: hash_entries,
            mappings: hash_mappings,
        })?);
        if let Some(existing) = self
            .repository
            .get_manuscript_reference_catalog_for_sync(&sync_run.id)
            .await?
        {
            if existing.catalog_hash == catalog_hash
                && matches!(existing.status, ManuscriptReferenceCatalogStatus::Completed)
            {
                return Ok(existing);
            }
            return Err(ResearchError::ManuscriptReferenceCatalogConflict {
                citation_sync_run_id: sync_run.id.to_string(),
            });
        }

        let timestamp = now_ms();
        let run = ManuscriptReferenceCatalogRun {
            id: catalog_run_id.clone(),
            research_case_id: sync_run.research_case_id.clone(),
            manuscript_source_id: sync_run.manuscript_source_id.clone(),
            citation_sync_run_id: sync_run.id.clone(),
            document_id: sync_run.document_id.clone(),
            document_version: sync_run.document_version,
            catalog_hash,
            entry_count: entries.len() as u32,
            target_mapping_count: mapping_drafts.len() as u32,
            status: ManuscriptReferenceCatalogStatus::Completed,
            created_at_ms: timestamp,
            completed_at_ms: Some(timestamp),
            failure_code: None,
        };
        let mappings = mapping_drafts
            .into_iter()
            .map(|mapping| ManuscriptReferenceTargetMapping {
                id: crate::ManuscriptReferenceTargetMappingId::new(),
                catalog_run_id: catalog_run_id.clone(),
                reference_entry_id: entry_ids[&mapping.identity].clone(),
                citation_occurrence_id: mapping.citation_occurrence_id,
                citation_target_id: mapping.citation_target_id,
                document_target_ordinal: mapping.document_target_ordinal,
            })
            .collect::<Vec<_>>();
        let result = self
            .repository
            .persist_manuscript_reference_catalog(&ManuscriptReferenceCatalogWrite {
                run,
                entries,
                mappings,
            })
            .await?;
        self.publish(
            "research.manuscriptReferenceCatalogCompleted",
            json!({
                "catalog_run_id": result.id,
                "citation_sync_run_id": result.citation_sync_run_id,
                "manuscript_source_id": result.manuscript_source_id,
                "entry_count": result.entry_count,
                "target_count": result.target_mapping_count,
                "status": result.status,
            }),
        );
        Ok(result)
    }

    pub async fn get_manuscript_reference_catalog(
        &self,
        id: &str,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
        let id = crate::ManuscriptReferenceCatalogRunId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_reference_catalog_run(&id)
            .await?
            .ok_or_else(|| not_found("manuscript reference catalog run", id.as_str()))
    }

    pub async fn manuscript_reference_catalog_for_sync(
        &self,
        sync_run_id: &str,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
        let sync_run_id = ManuscriptCitationSyncRunId::parse(sync_run_id.to_owned())?;
        self.repository
            .get_manuscript_reference_catalog_for_sync(&sync_run_id)
            .await?
            .ok_or_else(|| not_found("manuscript reference catalog run", sync_run_id.as_str()))
    }

    pub async fn latest_manuscript_reference_catalog(
        &self,
        research_case_id: &str,
        manuscript_source_id: &str,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
        let case_id = ResearchCaseId::parse(research_case_id.to_owned())?;
        let source_id = ResearchSourceId::parse(manuscript_source_id.to_owned())?;
        self.repository
            .latest_manuscript_reference_catalog(&case_id, &source_id)
            .await?
            .ok_or_else(|| not_found("manuscript reference catalog run", source_id.as_str()))
    }

    pub async fn list_manuscript_reference_entries(
        &self,
        catalog_run_id: &str,
    ) -> Result<Vec<ManuscriptReferenceEntry>, ResearchError> {
        let run = self
            .get_manuscript_reference_catalog(catalog_run_id)
            .await?;
        self.repository
            .list_manuscript_reference_entries(&run.id)
            .await
    }

    pub async fn list_manuscript_reference_target_mappings(
        &self,
        reference_entry_id: &str,
    ) -> Result<Vec<ManuscriptReferenceTargetMapping>, ResearchError> {
        let entry_id = crate::ManuscriptReferenceEntryId::parse(reference_entry_id.to_owned())?;
        self.repository
            .get_manuscript_reference_entry(&entry_id)
            .await?
            .ok_or_else(|| not_found("manuscript reference entry", entry_id.as_str()))?;
        self.repository
            .list_manuscript_reference_target_mappings(&entry_id)
            .await
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ReferenceDescriptor {
    format: String,
    reference_key: String,
    word_tag: Option<String>,
    word_title: Option<String>,
    word_author: Option<String>,
    word_year: Option<String>,
    zotero_item_id: Option<String>,
    zotero_uris: Vec<String>,
}

#[derive(Clone, Debug)]
struct ReferenceEntryDraft {
    descriptor: ReferenceDescriptor,
    descriptor_hash: ContentHash,
    target_count: u32,
}

#[derive(Clone, Debug)]
struct ReferenceMappingDraft {
    identity: (String, String),
    citation_occurrence_id: CitationOccurrenceId,
    citation_target_id: CitationTargetId,
    document_target_ordinal: u32,
    occurrence_ordinal: u32,
}

#[derive(Serialize)]
struct ReferenceCatalogHashEntry {
    ordinal: u32,
    descriptor: ReferenceDescriptor,
    descriptor_hash: String,
    target_count: u32,
}

#[derive(Serialize)]
struct ReferenceCatalogHashMapping {
    occurrence_ordinal: u32,
    citation_occurrence_id: String,
    citation_target_id: String,
    document_target_ordinal: u32,
    entry_ordinal: u32,
}

#[derive(Serialize)]
struct ReferenceCatalogHashPayload {
    citation_sync_run_id: String,
    inventory_hash: String,
    document_id: String,
    document_version: i64,
    entries: Vec<ReferenceCatalogHashEntry>,
    mappings: Vec<ReferenceCatalogHashMapping>,
}

fn catalog_format_key(format: &ManuscriptCitationFormat) -> &'static str {
    match format {
        ManuscriptCitationFormat::WordNative => "word_native",
        ManuscriptCitationFormat::Zotero => "zotero",
    }
}

fn reference_descriptor(
    format: &ManuscriptCitationFormat,
    reference_key: &str,
    target: &ManuscriptReferenceCatalogTargetInput,
) -> Result<ReferenceDescriptor, ResearchError> {
    bounded_text(
        "reference catalog reference key",
        reference_key,
        MAX_CITATION_REFERENCE_KEY_BYTES,
    )?;
    match format {
        ManuscriptCitationFormat::WordNative => {
            if target.zotero.is_some() {
                return Err(ResearchError::Invalid(
                    "Word Native reference cannot contain Zotero hints".to_owned(),
                ));
            }
            let source = target.word_source.as_ref();
            if let Some(source) = source {
                bounded_text(
                    "Word source tag",
                    &source.tag,
                    MAX_MANUSCRIPT_REFERENCE_URI_BYTES,
                )?;
                bounded_reference_hint("Word source title", &source.title)?;
                bounded_reference_hint("Word source author", &source.author)?;
                bounded_reference_hint("Word source year", &source.year)?;
                if source.tag != reference_key {
                    return Err(ResearchError::Invalid(
                        "Word source tag does not match reference key".to_owned(),
                    ));
                }
            }
            Ok(ReferenceDescriptor {
                format: catalog_format_key(format).to_owned(),
                reference_key: reference_key.to_owned(),
                word_tag: source.map(|value| value.tag.clone()),
                word_title: source.map(|value| value.title.clone()),
                word_author: source.map(|value| value.author.clone()),
                word_year: source.map(|value| value.year.clone()),
                zotero_item_id: None,
                zotero_uris: Vec::new(),
            })
        }
        ManuscriptCitationFormat::Zotero => {
            if target.word_source.is_some() {
                return Err(ResearchError::Invalid(
                    "Zotero reference cannot contain Word Native hints".to_owned(),
                ));
            }
            let zotero = target.zotero.as_ref();
            if let Some(zotero) = zotero {
                if zotero.uris.len() > MAX_MANUSCRIPT_REFERENCE_URI_COUNT {
                    return Err(ResearchError::Invalid(format!(
                        "Zotero reference cannot contain more than {MAX_MANUSCRIPT_REFERENCE_URI_COUNT} URIs"
                    )));
                }
                if let Some(item_id) = &zotero.item_id {
                    bounded_text(
                        "Zotero item ID",
                        item_id,
                        MAX_MANUSCRIPT_REFERENCE_URI_BYTES,
                    )?;
                    if item_id.contains('/')
                        || item_id.contains('\\')
                        || item_id.chars().any(char::is_whitespace)
                    {
                        return Err(ResearchError::Invalid(
                            "Zotero item ID must not contain a host path".to_owned(),
                        ));
                    }
                }
                for uri in &zotero.uris {
                    bounded_reference_uri(uri)?;
                }
            }
            Ok(ReferenceDescriptor {
                format: catalog_format_key(format).to_owned(),
                reference_key: reference_key.to_owned(),
                word_tag: None,
                word_title: None,
                word_author: None,
                word_year: None,
                zotero_item_id: zotero.and_then(|value| value.item_id.clone()),
                zotero_uris: zotero.map(|value| value.uris.clone()).unwrap_or_default(),
            })
        }
    }
}

fn bounded_reference_hint(field: &str, value: &str) -> Result<(), ResearchError> {
    if value.len() > MAX_MANUSCRIPT_REFERENCE_URI_BYTES {
        return Err(ResearchError::Invalid(format!(
            "{field} exceeds {MAX_MANUSCRIPT_REFERENCE_URI_BYTES} bytes"
        )));
    }
    if value
        .chars()
        .any(|character| character == '\0' || character == '\u{7f}')
    {
        return Err(ResearchError::Invalid(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn bounded_reference_uri(value: &str) -> Result<(), ResearchError> {
    bounded_text("Zotero URI", value, MAX_MANUSCRIPT_REFERENCE_URI_BYTES)?;
    if matches!(value.chars().next(), Some('/' | '\\')) {
        return Err(ResearchError::Invalid(
            "Zotero URI must not be a host path".to_owned(),
        ));
    }
    let Some(scheme_end) = value.find(':') else {
        return Err(ResearchError::Invalid(
            "Zotero URI must include a URI scheme".to_owned(),
        ));
    };
    let scheme = &value[..scheme_end];
    if scheme.is_empty()
        || !scheme.chars().enumerate().all(|(index, character)| {
            (index == 0 && character.is_ascii_alphabetic())
                || (index > 0
                    && (character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')))
        })
    {
        return Err(ResearchError::Invalid(
            "Zotero URI has an invalid scheme".to_owned(),
        ));
    }
    let remainder = &value[scheme_end + 1..];
    if scheme.eq_ignore_ascii_case("file")
        || (scheme.len() == 1 && matches!(remainder.chars().next(), Some('/' | '\\')))
    {
        return Err(ResearchError::Invalid(
            "Zotero URI must not be a host path".to_owned(),
        ));
    }
    if let Some(authority) = remainder.strip_prefix("//") {
        let authority = authority
            .split(|character| matches!(character, '/' | '?' | '#'))
            .next()
            .unwrap_or_default();
        if authority.contains('@') {
            return Err(ResearchError::Invalid(
                "Zotero URI must not contain credentials".to_owned(),
            ));
        }
    }
    Ok(())
}
