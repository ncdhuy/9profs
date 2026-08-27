use super::common::research_content_hash_dto;
use crate::api::ApiError;
use crate::api::AppState;
use crate::api::proposals::authorize_trusted_decision;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::routing::post;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::CreateManuscriptClaimExtractionRequest;
use nineprofs_api_types::CreateManuscriptReferenceCatalogRequest;
use nineprofs_api_types::ManuscriptCitationFormatDto;
use nineprofs_api_types::ManuscriptCitationSyncCitationRequest;
use nineprofs_api_types::ManuscriptCitationSyncOccurrenceDto;
use nineprofs_api_types::ManuscriptCitationSyncRunDto;
use nineprofs_api_types::ManuscriptCitationSyncStatusDto;
use nineprofs_api_types::ManuscriptCitationSyncTargetDto;
use nineprofs_api_types::ManuscriptCitationSyncTargetRequest;
use nineprofs_api_types::ManuscriptClaimExtractionCoverageDto;
use nineprofs_api_types::ManuscriptClaimExtractionCoverageStatusDto;
use nineprofs_api_types::ManuscriptClaimExtractionItemDto;
use nineprofs_api_types::ManuscriptClaimExtractionRunDto;
use nineprofs_api_types::ManuscriptClaimExtractionStatusDto;
use nineprofs_api_types::ManuscriptReferenceCatalogCitationRequest;
use nineprofs_api_types::ManuscriptReferenceCatalogRunDto;
use nineprofs_api_types::ManuscriptReferenceCatalogStatusDto;
use nineprofs_api_types::ManuscriptReferenceCatalogTargetRequest;
use nineprofs_api_types::ManuscriptReferenceEntryDto;
use nineprofs_api_types::ManuscriptReferenceTargetMappingDto;
use nineprofs_api_types::ManuscriptReferenceWordSourceDto;
use nineprofs_api_types::ManuscriptReferenceZoteroDto;
use nineprofs_api_types::SyncManuscriptCitationsRequest;
use nineprofs_research::ExtractManuscriptClaims;
use nineprofs_research::ManuscriptClaimExtractionBlockInput;
use nineprofs_research::ManuscriptClaimExtractionCitationInput;

async fn sync_manuscript_citations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((case_id, manuscript_source_id)): Path<(String, String)>,
    axum::Json(request): axum::Json<SyncManuscriptCitationsRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationSyncRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .sync_manuscript_citations(nineprofs_research::SyncManuscriptCitations {
            research_case_id: nineprofs_research::ResearchCaseId::parse(case_id)?,
            manuscript_source_id: nineprofs_research::ResearchSourceId::parse(
                manuscript_source_id,
            )?,
            document_id: request.document_id,
            document_version: request.document_version,
            citations: request
                .citations
                .into_iter()
                .map(manuscript_citation_sync_citation)
                .collect::<Result<Vec<_>, _>>()?,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_sync_run_dto(run),
    )))
}

async fn latest_manuscript_citation_sync(
    State(state): State<AppState>,
    Path((case_id, manuscript_source_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationSyncRunDto>>, ApiError> {
    let run = state
        .runtime
        .research_service()
        .latest_manuscript_citation_sync(&case_id, &manuscript_source_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_sync_run_dto(run),
    )))
}

async fn get_manuscript_citation_sync(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationSyncRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_sync_run_dto(
            state
                .runtime
                .research_service()
                .get_manuscript_citation_sync(&id)
                .await?,
        ),
    )))
}

async fn list_manuscript_citation_sync_occurrences(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCitationSyncOccurrenceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_citation_sync_occurrences(&id)
            .await?
            .into_iter()
            .map(manuscript_citation_sync_occurrence_dto)
            .collect(),
    )))
}

async fn list_manuscript_citation_sync_targets(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCitationSyncTargetDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_citation_sync_targets(&id)
            .await?
            .into_iter()
            .map(manuscript_citation_sync_target_dto)
            .collect(),
    )))
}

async fn create_manuscript_reference_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sync_run_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptReferenceCatalogRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .sync_manuscript_reference_catalog(nineprofs_research::SyncManuscriptReferenceCatalog {
            citation_sync_run_id: nineprofs_research::ManuscriptCitationSyncRunId::parse(
                sync_run_id,
            )?,
            document_id: request.document_id,
            document_version: request.document_version,
            citations: request
                .citations
                .into_iter()
                .map(manuscript_reference_catalog_citation)
                .collect::<Result<Vec<_>, _>>()?,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(run),
    )))
}

async fn get_manuscript_reference_catalog_for_sync(
    State(state): State<AppState>,
    Path(sync_run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(
            state
                .runtime
                .research_service()
                .manuscript_reference_catalog_for_sync(&sync_run_id)
                .await?,
        ),
    )))
}

async fn latest_manuscript_reference_catalog(
    State(state): State<AppState>,
    Path((case_id, manuscript_source_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(
            state
                .runtime
                .research_service()
                .latest_manuscript_reference_catalog(&case_id, &manuscript_source_id)
                .await?,
        ),
    )))
}

async fn get_manuscript_reference_catalog(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(
            state
                .runtime
                .research_service()
                .get_manuscript_reference_catalog(&id)
                .await?,
        ),
    )))
}

async fn list_manuscript_reference_entries(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptReferenceEntryDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_reference_entries(&id)
            .await?
            .into_iter()
            .map(manuscript_reference_entry_dto)
            .collect(),
    )))
}

async fn list_manuscript_reference_target_mappings(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptReferenceTargetMappingDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_reference_target_mappings(&id)
            .await?
            .into_iter()
            .map(manuscript_reference_target_mapping_dto)
            .collect(),
    )))
}

async fn create_manuscript_claim_extraction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sync_run_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptClaimExtractionRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptClaimExtractionRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .extract_manuscript_claims(ExtractManuscriptClaims {
            citation_sync_run_id: nineprofs_research::ManuscriptCitationSyncRunId::parse(
                sync_run_id,
            )?,
            document_id: request.document_id,
            document_version: request.document_version,
            blocks: request
                .blocks
                .into_iter()
                .map(|block| ManuscriptClaimExtractionBlockInput {
                    block_id: block.block_id,
                    text: block.text,
                    citations: block
                        .citations
                        .into_iter()
                        .map(|citation| ManuscriptClaimExtractionCitationInput {
                            citation_occurrence_id: citation.citation_occurrence_id,
                            start: citation.start,
                            end: citation.end,
                            rendered_text: citation.rendered_text,
                        })
                        .collect(),
                })
                .collect(),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_claim_extraction_run_dto(run),
    )))
}

async fn list_manuscript_claim_extractions(
    State(state): State<AppState>,
    Path(sync_run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimExtractionRunDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_claim_extractions(Some(&sync_run_id))
            .await?
            .into_iter()
            .map(manuscript_claim_extraction_run_dto)
            .collect(),
    )))
}

async fn get_manuscript_claim_extraction(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptClaimExtractionRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_claim_extraction_run_dto(
            state
                .runtime
                .research_service()
                .get_manuscript_claim_extraction(&id)
                .await?,
        ),
    )))
}

async fn list_manuscript_claim_extraction_items(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimExtractionItemDto>>>, ApiError> {
    let service = state.runtime.research_service();
    let items = service.list_manuscript_claim_extraction_items(&id).await?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let claim = service.get_claim(item.research_claim_id.as_str()).await?;
        let links = service
            .list_claim_citation_links(None, Some(item.research_claim_id.as_str()), None)
            .await?;
        result.push(manuscript_claim_extraction_item_dto(
            item,
            claim.text,
            links
                .iter()
                .map(|link| link.citation_occurrence_id.to_string())
                .collect(),
            links.iter().map(|link| link.id.to_string()).collect(),
        ));
    }
    Ok(axum::Json(ApiResponse::ok(result)))
}

async fn list_manuscript_claim_extraction_coverage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimExtractionCoverageDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_claim_extraction_coverage(&id)
            .await?
            .into_iter()
            .map(manuscript_claim_extraction_coverage_dto)
            .collect(),
    )))
}

pub(crate) fn manuscript_citation_sync_run_dto(
    value: nineprofs_research::ManuscriptCitationSyncRun,
) -> ManuscriptCitationSyncRunDto {
    ManuscriptCitationSyncRunDto {
        sync_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        manuscript_source_id: value.manuscript_source_id.to_string(),
        document_id: value.document_id,
        document_version: value.document_version,
        inventory_hash: research_content_hash_dto(value.inventory_hash),
        status: manuscript_citation_sync_status_dto(value.status),
        occurrence_count: value.occurrence_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

pub(crate) fn manuscript_citation_sync_occurrence_dto(
    value: nineprofs_research::ManuscriptCitationSyncOccurrence,
) -> ManuscriptCitationSyncOccurrenceDto {
    ManuscriptCitationSyncOccurrenceDto {
        sync_occurrence_id: value.id.to_string(),
        sync_run_id: value.sync_run_id.to_string(),
        ordinal: value.ordinal,
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        document_block_id: value.document_block_id,
        start: value.start,
        end: value.end,
        format: manuscript_citation_sync_format_dto(value.format),
    }
}

pub(crate) fn manuscript_citation_sync_target_dto(
    value: nineprofs_research::ManuscriptCitationSyncTarget,
) -> ManuscriptCitationSyncTargetDto {
    ManuscriptCitationSyncTargetDto {
        sync_target_id: value.id.to_string(),
        sync_occurrence_id: value.sync_occurrence_id.to_string(),
        document_target_ordinal: value.document_target_ordinal,
        citation_target_id: value.citation_target_id.to_string(),
    }
}

pub(crate) fn manuscript_reference_catalog_run_dto(
    value: nineprofs_research::ManuscriptReferenceCatalogRun,
) -> ManuscriptReferenceCatalogRunDto {
    ManuscriptReferenceCatalogRunDto {
        catalog_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        manuscript_source_id: value.manuscript_source_id.to_string(),
        citation_sync_run_id: value.citation_sync_run_id.to_string(),
        document_id: value.document_id,
        document_version: value.document_version,
        catalog_hash: research_content_hash_dto(value.catalog_hash),
        entry_count: value.entry_count,
        target_mapping_count: value.target_mapping_count,
        status: manuscript_reference_catalog_status_dto(value.status),
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

pub(crate) fn manuscript_reference_catalog_status_dto(
    value: nineprofs_research::ManuscriptReferenceCatalogStatus,
) -> ManuscriptReferenceCatalogStatusDto {
    match value {
        nineprofs_research::ManuscriptReferenceCatalogStatus::Running => {
            ManuscriptReferenceCatalogStatusDto::Running
        }
        nineprofs_research::ManuscriptReferenceCatalogStatus::Completed => {
            ManuscriptReferenceCatalogStatusDto::Completed
        }
        nineprofs_research::ManuscriptReferenceCatalogStatus::Failed => {
            ManuscriptReferenceCatalogStatusDto::Failed
        }
    }
}

pub(crate) fn manuscript_reference_entry_dto(
    value: nineprofs_research::ManuscriptReferenceEntry,
) -> ManuscriptReferenceEntryDto {
    ManuscriptReferenceEntryDto {
        entry_id: value.id.to_string(),
        catalog_run_id: value.catalog_run_id.to_string(),
        ordinal: value.ordinal,
        format: manuscript_citation_sync_format_dto(value.format),
        reference_key: value.reference_key,
        descriptor_hash: research_content_hash_dto(value.descriptor_hash),
        word_source: value.word_tag.map(|tag| ManuscriptReferenceWordSourceDto {
            tag,
            title: value.word_title.unwrap_or_default(),
            author: value.word_author.unwrap_or_default(),
            year: value.word_year.unwrap_or_default(),
        }),
        zotero: if value.zotero_item_id.is_some() || !value.zotero_uris.is_empty() {
            Some(ManuscriptReferenceZoteroDto {
                item_id: value.zotero_item_id,
                uris: value.zotero_uris,
            })
        } else {
            None
        },
        target_count: value.target_count,
    }
}

pub(crate) fn manuscript_reference_target_mapping_dto(
    value: nineprofs_research::ManuscriptReferenceTargetMapping,
) -> ManuscriptReferenceTargetMappingDto {
    ManuscriptReferenceTargetMappingDto {
        mapping_id: value.id.to_string(),
        catalog_run_id: value.catalog_run_id.to_string(),
        reference_entry_id: value.reference_entry_id.to_string(),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        citation_target_id: value.citation_target_id.to_string(),
        document_target_ordinal: value.document_target_ordinal,
    }
}

pub(crate) fn manuscript_citation_sync_format_dto(
    value: nineprofs_research::ManuscriptCitationFormat,
) -> ManuscriptCitationFormatDto {
    match value {
        nineprofs_research::ManuscriptCitationFormat::WordNative => {
            ManuscriptCitationFormatDto::WordNative
        }
        nineprofs_research::ManuscriptCitationFormat::Zotero => ManuscriptCitationFormatDto::Zotero,
    }
}

pub(crate) fn manuscript_citation_sync_status_dto(
    value: nineprofs_research::ManuscriptCitationSyncStatus,
) -> ManuscriptCitationSyncStatusDto {
    match value {
        nineprofs_research::ManuscriptCitationSyncStatus::Running => {
            ManuscriptCitationSyncStatusDto::Running
        }
        nineprofs_research::ManuscriptCitationSyncStatus::Completed => {
            ManuscriptCitationSyncStatusDto::Completed
        }
        nineprofs_research::ManuscriptCitationSyncStatus::Failed => {
            ManuscriptCitationSyncStatusDto::Failed
        }
    }
}

pub(crate) fn manuscript_claim_extraction_run_dto(
    value: nineprofs_research::ManuscriptClaimExtractionRun,
) -> ManuscriptClaimExtractionRunDto {
    ManuscriptClaimExtractionRunDto {
        extraction_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        manuscript_source_id: value.manuscript_source_id.to_string(),
        citation_sync_run_id: value.citation_sync_run_id.to_string(),
        document_id: value.document_id,
        document_version: value.document_version,
        context_hash: research_content_hash_dto(value.context_hash),
        extractor_provider: value.extractor_provider,
        extractor_version: value.extractor_version,
        extractor_model_id: value.extractor_model_id,
        extraction_contract_version: value.extraction_contract_version,
        status: manuscript_claim_extraction_status_dto(value.status),
        claim_count: value.claim_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

pub(crate) fn manuscript_claim_extraction_item_dto(
    value: nineprofs_research::ManuscriptClaimExtractionItem,
    claim_text: String,
    citation_occurrence_ids: Vec<String>,
    claim_citation_link_ids: Vec<String>,
) -> ManuscriptClaimExtractionItemDto {
    ManuscriptClaimExtractionItemDto {
        item_id: value.id.to_string(),
        extraction_run_id: value.extraction_run_id.to_string(),
        research_claim_id: value.research_claim_id.to_string(),
        document_block_id: value.document_block_id,
        source_start: value.source_start,
        source_end: value.source_end,
        source_excerpt: value.source_excerpt,
        source_excerpt_hash: research_content_hash_dto(value.source_excerpt_hash),
        ordinal: value.ordinal,
        claim_text,
        citation_occurrence_ids,
        claim_citation_link_ids,
    }
}

pub(crate) fn manuscript_claim_extraction_status_dto(
    value: nineprofs_research::ManuscriptClaimExtractionStatus,
) -> ManuscriptClaimExtractionStatusDto {
    match value {
        nineprofs_research::ManuscriptClaimExtractionStatus::Running => {
            ManuscriptClaimExtractionStatusDto::Running
        }
        nineprofs_research::ManuscriptClaimExtractionStatus::Completed => {
            ManuscriptClaimExtractionStatusDto::Completed
        }
        nineprofs_research::ManuscriptClaimExtractionStatus::Failed => {
            ManuscriptClaimExtractionStatusDto::Failed
        }
    }
}

pub(crate) fn manuscript_claim_extraction_coverage_dto(
    value: nineprofs_research::ManuscriptClaimExtractionCoverage,
) -> ManuscriptClaimExtractionCoverageDto {
    ManuscriptClaimExtractionCoverageDto {
        coverage_id: value.id.to_string(),
        extraction_run_id: value.extraction_run_id.to_string(),
        extraction_item_id: value.extraction_item_id.map(|id| id.to_string()),
        claim_citation_link_id: value.claim_citation_link_id.map(|id| id.to_string()),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        status: match value.status {
            nineprofs_research::ManuscriptClaimExtractionCoverageStatus::AssociatedWithClaim => {
                ManuscriptClaimExtractionCoverageStatusDto::AssociatedWithClaim
            }
            nineprofs_research::ManuscriptClaimExtractionCoverageStatus::NoVerifiableClaim => {
                ManuscriptClaimExtractionCoverageStatusDto::NoVerifiableClaim
            }
        },
        reason: value.reason,
    }
}

pub(crate) fn manuscript_citation_sync_citation(
    value: ManuscriptCitationSyncCitationRequest,
) -> Result<nineprofs_research::ManuscriptCitationSyncCitationInput, ApiError> {
    Ok(nineprofs_research::ManuscriptCitationSyncCitationInput {
        format: match value.format {
            ManuscriptCitationFormatDto::WordNative => {
                nineprofs_research::ManuscriptCitationFormat::WordNative
            }
            ManuscriptCitationFormatDto::Zotero => {
                nineprofs_research::ManuscriptCitationFormat::Zotero
            }
        },
        rendered_text: value.rendered_text,
        block_id: value.block_id,
        start: value.start,
        end: value.end,
        targets: value
            .targets
            .into_iter()
            .map(|target: ManuscriptCitationSyncTargetRequest| {
                nineprofs_research::ManuscriptCitationSyncTargetInput {
                    ordinal: target.ordinal,
                    reference_key: target.reference_key,
                    cited_locator: target.cited_locator,
                }
            })
            .collect(),
    })
}

pub(crate) fn manuscript_reference_catalog_citation(
    value: ManuscriptReferenceCatalogCitationRequest,
) -> Result<nineprofs_research::ManuscriptReferenceCatalogCitationInput, ApiError> {
    Ok(
        nineprofs_research::ManuscriptReferenceCatalogCitationInput {
            citation_occurrence_id: value.citation_occurrence_id,
            block_id: value.block_id,
            start: value.start,
            end: value.end,
            format: match value.format {
                ManuscriptCitationFormatDto::WordNative => {
                    nineprofs_research::ManuscriptCitationFormat::WordNative
                }
                ManuscriptCitationFormatDto::Zotero => {
                    nineprofs_research::ManuscriptCitationFormat::Zotero
                }
            },
            targets: value
                .targets
                .into_iter()
                .map(manuscript_reference_catalog_target)
                .collect::<Result<Vec<_>, _>>()?,
        },
    )
}

pub(crate) fn manuscript_reference_catalog_target(
    value: ManuscriptReferenceCatalogTargetRequest,
) -> Result<nineprofs_research::ManuscriptReferenceCatalogTargetInput, ApiError> {
    Ok(nineprofs_research::ManuscriptReferenceCatalogTargetInput {
        citation_target_id: value.citation_target_id,
        ordinal: value.ordinal,
        reference_key: value.reference_key,
        word_source: value.word_source.map(|source| {
            nineprofs_research::ManuscriptReferenceCatalogWordSourceInput {
                tag: source.tag,
                title: source.title,
                author: source.author,
                year: source.year,
            }
        }),
        zotero: value.zotero.map(|zotero| {
            nineprofs_research::ManuscriptReferenceCatalogZoteroInput {
                item_id: zotero.item_id,
                uris: zotero.uris,
            }
        }),
    })
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{case_id}/manuscripts/{manuscript_source_id}/citations/sync",
            post(sync_manuscript_citations),
        )
        .route(
            "/api/research/cases/{case_id}/manuscripts/{manuscript_source_id}/citations/sync/latest",
            get(latest_manuscript_citation_sync),
        )
        .route(
            "/api/research/manuscript-citation-sync-runs/{id}",
            get(get_manuscript_citation_sync),
        )
        .route(
            "/api/research/manuscript-citation-sync-runs/{id}/occurrences",
            get(list_manuscript_citation_sync_occurrences),
        )
        .route(
            "/api/research/manuscript-citation-sync-occurrences/{id}/targets",
            get(list_manuscript_citation_sync_targets),
        )
        .route(
            "/api/research/manuscript-citation-syncs/{sync_run_id}/reference-catalog",
            get(get_manuscript_reference_catalog_for_sync)
                .post(create_manuscript_reference_catalog),
        )
        .route(
            "/api/research/cases/{case_id}/manuscripts/{manuscript_source_id}/reference-catalog/latest",
            get(latest_manuscript_reference_catalog),
        )
        .route(
            "/api/research/manuscript-reference-catalog-runs/{id}",
            get(get_manuscript_reference_catalog),
        )
        .route(
            "/api/research/manuscript-reference-catalog-runs/{id}/entries",
            get(list_manuscript_reference_entries),
        )
        .route(
            "/api/research/manuscript-reference-entries/{id}/mappings",
            get(list_manuscript_reference_target_mappings),
        )
        .route(
            "/api/research/manuscript-citation-syncs/{sync_run_id}/claim-extractions",
            get(list_manuscript_claim_extractions).post(create_manuscript_claim_extraction),
        )
        .route(
            "/api/research/manuscript-claim-extractions/{id}",
            get(get_manuscript_claim_extraction),
        )
        .route(
            "/api/research/manuscript-claim-extractions/{id}/items",
            get(list_manuscript_claim_extraction_items),
        )
        .route(
            "/api/research/manuscript-claim-extractions/{id}/coverage",
            get(list_manuscript_claim_extraction_coverage),
        )
}
