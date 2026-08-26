//! Rebuildable Dify retrieval index for the provider-neutral research domain.
//!
//! Dify stores derived search data only. Local chunk/range rows and the
//! `ResearchService` remain canonical for provenance and excerpt resolution.

use std::{fmt, sync::Arc, time::Duration};

use nineprofs_api_types::EventEnvelope;
use nineprofs_common::{new_id, now_ms};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    ContentHash, HashAlgorithm, ResearchError, ResearchPdfPage, ResearchService,
};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use thiserror::Error;

pub const CHUNKER_VERSION: &str = "pdf-page-v1";
pub const CHUNK_TARGET_CODE_POINTS: usize = 1_600;
pub const CHUNK_MAX_CODE_POINTS: usize = 2_000;
pub const CHUNK_OVERLAP_CODE_POINTS: usize = 200;
pub const MAX_QUERY_CODE_POINTS: usize = 250;
pub const MAX_TOP_K: u32 = 20;
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_BATCH_SIZE: usize = 50;

#[derive(Clone)]
pub struct DifyConfig {
    pub base_url: String,
    api_key: Arc<str>,
    pub timeout: Duration,
    pub poll_interval: Duration,
    pub max_poll_attempts: u32,
    pub batch_size: usize,
    pub indexing_technique: String,
}

impl fmt::Debug for DifyConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DifyConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &"[redacted]")
            .field("timeout", &self.timeout)
            .field("poll_interval", &self.poll_interval)
            .field("max_poll_attempts", &self.max_poll_attempts)
            .field("batch_size", &self.batch_size)
            .field("indexing_technique", &self.indexing_technique)
            .finish()
    }
}

impl DifyConfig {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<Arc<str>>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
            timeout: Duration::from_secs(30),
            poll_interval: Duration::from_millis(250),
            max_poll_attempts: 120,
            batch_size: DEFAULT_BATCH_SIZE,
            indexing_technique: "high_quality".to_owned(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("NINEPROFS_DIFY_BASE_URL").ok()?;
        let api_key = std::env::var("NINEPROFS_DIFY_API_KEY").ok()?;
        if base_url.trim().is_empty() || api_key.is_empty() {
            return None;
        }
        let mut config = Self::new(base_url, Arc::<str>::from(api_key));
        if let Ok(value) = std::env::var("NINEPROFS_DIFY_TIMEOUT_MS")
            && let Ok(milliseconds) = value.parse::<u64>()
        {
            config.timeout = Duration::from_millis(milliseconds.clamp(100, 120_000));
        }
        if let Ok(value) = std::env::var("NINEPROFS_DIFY_INDEXING_TECHNIQUE")
            && matches!(value.as_str(), "high_quality" | "economy")
        {
            config.indexing_technique = value;
        }
        Some(config)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DifyIndexStatus {
    NotConfigured,
    Provisioning,
    Ready,
    Syncing,
    Failed,
    Degraded,
}

#[derive(Clone, Debug, Serialize)]
pub struct DifyReadiness {
    pub provider: &'static str,
    pub qualification_target: &'static str,
    pub configured: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DifyCaseIndex {
    pub index_id: String,
    pub research_case_id: String,
    pub dataset_id: String,
    pub status: DifyIndexStatus,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DifyExtractionIndex {
    pub index_id: String,
    pub case_index_id: String,
    pub research_case_id: String,
    pub extraction_id: String,
    pub source_snapshot_id: String,
    pub document_id: Option<String>,
    pub chunker_version: String,
    pub status: DifyIndexStatus,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalIndexState {
    pub readiness: DifyReadiness,
    pub case_index: Option<DifyCaseIndex>,
    pub extraction_indexes: Vec<DifyExtractionIndex>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalCandidate {
    pub retrieval_chunk_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub verbatim_excerpt: String,
    pub retrieval_score: f64,
    pub provider: &'static str,
    pub rank: u32,
}

#[derive(Debug, Error)]
pub enum DifyError {
    #[error("Dify is not configured")]
    NotConfigured,
    #[error("Dify is unreachable")]
    Unreachable,
    #[error("Dify authorization failed")]
    Unauthorized,
    #[error("Dify rate limit exceeded")]
    RateLimited,
    #[error("Dify provider is not initialized")]
    ProviderNotInitialized,
    #[error("Dify indexing failed")]
    IndexingFailed,
    #[error("Dify request timed out")]
    Timeout,
    #[error("Dify response was malformed")]
    MalformedResponse,
    #[error("Dify resource was not found")]
    RemoteNotFound,
    #[error("Dify index drift detected")]
    IndexDrift,
    #[error("canonical retrieval integrity check failed")]
    Integrity,
    #[error("invalid retrieval request: {0}")]
    Invalid(String),
    #[error("retrieval index database query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Research(#[from] ResearchError),
}

#[derive(Clone)]
struct DifyClient {
    http: Client,
    config: DifyConfig,
}

#[derive(Debug, Deserialize)]
struct DifyIndexingStatus {
    #[serde(default)]
    indexing_status: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DifyDatasetResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DifyErrorResponse {
    #[serde(default)]
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DifyDocumentResponse {
    document: DifyDocumentPayload,
    batch: String,
}

#[derive(Debug, Deserialize)]
struct DifyDocumentPayload {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DifyIndexingStatusResponse {
    data: Vec<DifyIndexingStatus>,
}

#[derive(Debug)]
struct DifyDocument {
    id: String,
    batch: String,
}

#[derive(Debug, Deserialize)]
struct DifySegment {
    id: String,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DifySegmentListResponse {
    data: Vec<DifySegment>,
}

#[derive(Debug, Deserialize)]
struct DifyRetrieveResponse {
    records: Vec<DifyHitRecord>,
}

#[derive(Debug, Deserialize)]
struct DifyHitRecord {
    segment: DifyHitSegment,
    score: f64,
}

#[derive(Debug, Deserialize)]
struct DifyHitSegment {
    id: String,
}

#[derive(Debug)]
struct DifyHit {
    segment_id: String,
    score: f64,
}

impl DifyClient {
    fn new(config: DifyConfig) -> Result<Self, DifyError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|_| DifyError::Unreachable)?;
        Ok(Self { http, config })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.config.base_url, path.trim_start_matches('/'))
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Vec<u8>, DifyError> {
        let mut request = self
            .http
            .request(method, self.url(path))
            .bearer_auth(self.config.api_key.as_ref());
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(DifyError::MalformedResponse);
        }
        if !status.is_success() {
            return Err(map_remote_error(status, &bytes));
        }
        Ok(bytes.to_vec())
    }

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, DifyError> {
        let bytes = self.send(method, path, body).await?;
        serde_json::from_slice(&bytes).map_err(|_| DifyError::MalformedResponse)
    }

    async fn create_dataset(&self, name: &str) -> Result<String, DifyError> {
        let response: DifyDatasetResponse = self
            .send_json(
                Method::POST,
                "datasets",
                Some(json!({
                    "name": name,
                    "description": "9Profs derived research retrieval index",
                    "indexing_technique": self.config.indexing_technique,
                    "permission": "only_me"
                })),
            )
            .await?;
        Ok(response.id)
    }

    async fn get_dataset(&self, dataset_id: &str) -> Result<(), DifyError> {
        let _: DifyDatasetResponse = self
            .send_json(Method::GET, &format!("datasets/{dataset_id}"), None)
            .await?;
        Ok(())
    }

    async fn create_text_document(
        &self,
        dataset_id: &str,
        name: &str,
        text: &str,
    ) -> Result<DifyDocument, DifyError> {
        let response: DifyDocumentResponse = self
            .send_json(
                Method::POST,
                &format!("datasets/{dataset_id}/document/create-by-text"),
                Some(json!({
                    "name": name,
                    "text": text,
                    "doc_form": "text_model",
                    "doc_language": "English"
                })),
            )
            .await?;
        Ok(DifyDocument {
            id: response.document.id,
            batch: response.batch,
        })
    }

    async fn indexing_status(
        &self,
        dataset_id: &str,
        batch: &str,
    ) -> Result<Vec<DifyIndexingStatus>, DifyError> {
        let response: DifyIndexingStatusResponse = self
            .send_json(
                Method::GET,
                &format!("datasets/{dataset_id}/documents/{batch}/indexing-status"),
                None,
            )
            .await?;
        Ok(response.data)
    }

    async fn list_segments(
        &self,
        dataset_id: &str,
        document_id: &str,
        page: u32,
    ) -> Result<Vec<DifySegment>, DifyError> {
        let response: DifySegmentListResponse = self
            .send_json(
                Method::GET,
                &format!(
                    "datasets/{dataset_id}/documents/{document_id}/segments?page={page}&limit=100"
                ),
                None,
            )
            .await?;
        Ok(response.data)
    }

    async fn list_all_segments(
        &self,
        dataset_id: &str,
        document_id: &str,
    ) -> Result<Vec<DifySegment>, DifyError> {
        let mut all = Vec::new();
        let mut page = 1;
        loop {
            let segments = self.list_segments(dataset_id, document_id, page).await?;
            let count = segments.len();
            all.extend(segments);
            if count < 100 {
                return Ok(all);
            }
            page = page.checked_add(1).ok_or(DifyError::MalformedResponse)?;
        }
    }

    async fn delete_segment(
        &self,
        dataset_id: &str,
        document_id: &str,
        segment_id: &str,
    ) -> Result<(), DifyError> {
        self.send(
            Method::DELETE,
            &format!("datasets/{dataset_id}/documents/{document_id}/segments/{segment_id}"),
            None,
        )
        .await
        .map(|_| ())
    }

    async fn create_segments(
        &self,
        dataset_id: &str,
        document_id: &str,
        texts: &[String],
    ) -> Result<Vec<DifySegment>, DifyError> {
        let response: DifySegmentListResponse = self
            .send_json(
                Method::POST,
                &format!("datasets/{dataset_id}/documents/{document_id}/segments"),
                Some(json!({
                    "segments": texts.iter().map(|text| json!({"content": text})).collect::<Vec<_>>()
                })),
            )
            .await?;
        Ok(response.data)
    }

    async fn retrieve(
        &self,
        dataset_id: &str,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<DifyHit>, DifyError> {
        let response: DifyRetrieveResponse = self
            .send_json(
                Method::POST,
                &format!("datasets/{dataset_id}/retrieve"),
                Some(json!({
                    "query": query,
                    "retrieval_model": {"top_k": top_k}
                })),
            )
            .await?;
        Ok(response
            .records
            .into_iter()
            .map(|record| DifyHit {
                segment_id: record.segment.id,
                score: record.score,
            })
            .collect())
    }

    async fn wait_for_batch(&self, dataset_id: &str, batch: &str) -> Result<(), DifyError> {
        for _ in 0..self.config.max_poll_attempts {
            let statuses = self.indexing_status(dataset_id, batch).await?;
            if statuses.iter().any(|status| {
                status
                    .indexing_status
                    .as_deref()
                    .is_some_and(|value| value == "error" || value == "failed")
                    || status.error.is_some()
            }) {
                return Err(DifyError::IndexingFailed);
            }
            if !statuses.is_empty()
                && statuses.iter().all(|status| {
                    status
                        .indexing_status
                        .as_deref()
                        .is_some_and(|value| value == "completed")
                })
            {
                return Ok(());
            }
            tokio::time::sleep(self.config.poll_interval).await;
        }
        Err(DifyError::Timeout)
    }

    async fn wait_for_segments(
        &self,
        dataset_id: &str,
        document_id: &str,
        expected: usize,
    ) -> Result<Vec<DifySegment>, DifyError> {
        for _ in 0..self.config.max_poll_attempts {
            let segments = self.list_all_segments(dataset_id, document_id).await?;
            if segments.len() >= expected
                && segments.iter().all(|segment| {
                    segment
                        .status
                        .as_deref()
                        .is_none_or(|status| status == "completed")
                })
            {
                return Ok(segments);
            }
            tokio::time::sleep(self.config.poll_interval).await;
        }
        Err(DifyError::Timeout)
    }
}

pub struct DifyResearchService {
    pool: SqlitePool,
    research: Arc<ResearchService>,
    events: Arc<BroadcastEventBus>,
    config: Option<DifyConfig>,
    client: Option<DifyClient>,
}

impl DifyResearchService {
    pub fn new(
        pool: SqlitePool,
        research: Arc<ResearchService>,
        events: Arc<BroadcastEventBus>,
        config: Option<DifyConfig>,
    ) -> Result<Self, DifyError> {
        let client = config.clone().map(DifyClient::new).transpose()?;
        Ok(Self {
            pool,
            research,
            events,
            config,
            client,
        })
    }

    pub fn readiness(&self) -> DifyReadiness {
        DifyReadiness {
            provider: "dify",
            qualification_target: "1.16.1",
            configured: self.config.is_some(),
        }
    }

    pub async fn state(&self, research_case_id: &str) -> Result<RetrievalIndexState, DifyError> {
        let case_index = self.case_index(research_case_id).await?;
        let extraction_indexes = match case_index.as_ref() {
            Some(index) => self.extraction_indexes(&index.index_id).await?,
            None => Vec::new(),
        };
        Ok(RetrievalIndexState {
            readiness: self.readiness(),
            case_index,
            extraction_indexes,
        })
    }

    pub async fn ensure_case_index(
        &self,
        research_case_id: &str,
    ) -> Result<DifyCaseIndex, DifyError> {
        let client = self.client.as_ref().ok_or(DifyError::NotConfigured)?;
        let case = self.research.get_case(research_case_id).await?;
        if let Some(index) = self.case_index(research_case_id).await? {
            client.get_dataset(&index.dataset_id).await?;
            return Ok(index);
        }

        let dataset_id = client
            .create_dataset(&dataset_name(&case.id.to_string()))
            .await?;
        let timestamp = now_ms();
        let index = DifyCaseIndex {
            index_id: new_id(),
            research_case_id: case.id.to_string(),
            dataset_id,
            status: DifyIndexStatus::Ready,
            failure_code: None,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        sqlx::query(
            "INSERT INTO research_dify_case_indexes \
             (id, research_case_id, dataset_id, status, failure_code, created_at_ms, updated_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&index.index_id)
        .bind(&index.research_case_id)
        .bind(&index.dataset_id)
        .bind(status_text(&index.status))
        .bind(Option::<String>::None)
        .bind(timestamp)
        .bind(timestamp)
        .execute(&self.pool)
        .await?;
        self.publish(
            "research.retrievalIndexCreated",
            json!({
                "index_id": index.index_id,
                "research_case_id": index.research_case_id,
                "dataset_id": index.dataset_id,
                "provider": "dify"
            }),
        );
        Ok(index)
    }

    pub async fn sync_extraction(
        &self,
        index_id: &str,
        extraction_id: &str,
    ) -> Result<DifyExtractionIndex, DifyError> {
        let client = self.client.as_ref().ok_or(DifyError::NotConfigured)?;
        let case_index = self
            .case_index_by_id(index_id)
            .await?
            .ok_or(DifyError::RemoteNotFound)?;
        let extraction = self
            .research
            .require_ready_pdf_extraction(extraction_id)
            .await?;
        let snapshot = self
            .research
            .get_snapshot(&extraction.source_snapshot_id.to_string())
            .await?;
        let source = self
            .research
            .get_source(&snapshot.source_id.to_string())
            .await?;
        if source.research_case_id.to_string() != case_index.research_case_id {
            return Err(DifyError::Invalid(
                "PDF extraction does not belong to research case".to_owned(),
            ));
        }

        if let Some(existing) = self
            .extraction_index(&case_index.index_id, extraction_id)
            .await?
            && matches!(existing.status, DifyIndexStatus::Ready)
        {
            return Ok(existing);
        }

        let index = self
            .begin_extraction_index(&case_index, &extraction, &snapshot)
            .await?;
        self.publish(
            "research.extractionIndexingStarted",
            json!({
                "index_id": index.index_id,
                "research_case_id": index.research_case_id,
                "extraction_id": index.extraction_id,
                "chunker_version": CHUNKER_VERSION
            }),
        );

        let result = self
            .sync_extraction_remote(
                client,
                &index,
                &case_index,
                &extraction,
                &source.id.to_string(),
            )
            .await;
        match result {
            Ok(index) => {
                self.update_extraction_status(&index.index_id, &DifyIndexStatus::Ready, None)
                    .await?;
                self.publish(
                    "research.extractionIndexed",
                    json!({
                        "index_id": index.index_id,
                        "research_case_id": index.research_case_id,
                        "extraction_id": index.extraction_id,
                        "chunk_count": self.chunk_count(&index.index_id).await?
                    }),
                );
                Ok(index)
            }
            Err(error) => {
                let status = status_for_error(&error);
                self.update_extraction_status(&index.index_id, &status, Some(error_code(&error)))
                    .await?;
                self.publish(
                    "research.extractionIndexFailed",
                    json!({
                        "index_id": index.index_id,
                        "research_case_id": index.research_case_id,
                        "extraction_id": index.extraction_id,
                        "error_code": error_code(&error)
                    }),
                );
                Err(error)
            }
        }
    }

    pub async fn retrieve(
        &self,
        research_case_id: &str,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<RetrievalCandidate>, DifyError> {
        let client = self.client.as_ref().ok_or(DifyError::NotConfigured)?;
        let query = bounded_query(query)?;
        let top_k = top_k.clamp(1, MAX_TOP_K);
        let case_index = self
            .case_index(research_case_id)
            .await?
            .ok_or(DifyError::RemoteNotFound)?;
        let hits = client
            .retrieve(&case_index.dataset_id, &query, top_k)
            .await?;
        let mut candidates = Vec::new();
        for (rank, hit) in hits.into_iter().take(top_k as usize).enumerate() {
            let Some(row) = sqlx::query(
                "SELECT c.id, c.research_source_id, c.source_snapshot_id, c.extraction_id, \
                 c.page, c.start_offset, c.end_offset, c.text, c.text_hash, c.extraction_index_id \
                 FROM research_dify_segment_mappings m \
                 JOIN research_retrieval_chunks c ON c.id = m.retrieval_chunk_id \
                 WHERE m.dataset_id = ? AND m.segment_id = ?",
            )
            .bind(&case_index.dataset_id)
            .bind(&hit.segment_id)
            .fetch_optional(&self.pool)
            .await?
            else {
                self.mark_case_degraded(research_case_id).await?;
                self.publish(
                    "research.retrievalIndexDegraded",
                    json!({
                        "research_case_id": research_case_id,
                        "error_code": "index_drift"
                    }),
                );
                return Err(DifyError::IndexDrift);
            };
            let extraction_index_id: String = row.get("extraction_index_id");
            if !self.extraction_index_is_ready(&extraction_index_id).await? {
                return Err(DifyError::IndexingFailed);
            }
            let extraction_id: String = row.get("extraction_id");
            let page_number: u32 = row.get::<i64, _>("page") as u32;
            let start: u64 = row.get::<i64, _>("start_offset") as u64;
            let end: u64 = row.get::<i64, _>("end_offset") as u64;
            let stored_text: String = row.get("text");
            let stored_hash: String = row.get("text_hash");
            let page = self
                .research
                .get_pdf_page(&extraction_id, page_number)
                .await?;
            let excerpt = canonical_excerpt(&page, start, end, &stored_text, &stored_hash)?;
            candidates.push(RetrievalCandidate {
                retrieval_chunk_id: row.get("id"),
                research_source_id: row.get("research_source_id"),
                source_snapshot_id: row.get("source_snapshot_id"),
                extraction_id,
                page: page_number,
                start,
                end,
                verbatim_excerpt: excerpt,
                retrieval_score: hit.score,
                provider: "dify",
                rank: rank as u32 + 1,
            });
        }
        Ok(candidates)
    }

    async fn sync_extraction_remote(
        &self,
        client: &DifyClient,
        index: &DifyExtractionIndex,
        case_index: &DifyCaseIndex,
        extraction: &nineprofs_research::ResearchPdfExtraction,
        source_id: &str,
    ) -> Result<DifyExtractionIndex, DifyError> {
        let pages = self
            .research
            .list_all_pdf_pages_for_indexing(&extraction.id.to_string())
            .await?;
        let chunks = chunk_pages(
            &pages,
            &index.research_case_id,
            source_id,
            &extraction.source_snapshot_id.to_string(),
            &extraction.id.to_string(),
        )?;
        self.clear_extraction_data(&index.index_id).await?;
        let document_id = if let Some(existing_document_id) = index.document_id.as_deref() {
            match client
                .list_all_segments(&case_index.dataset_id, existing_document_id)
                .await
            {
                Ok(segments) => {
                    for segment in segments {
                        client
                            .delete_segment(
                                &case_index.dataset_id,
                                existing_document_id,
                                &segment.id,
                            )
                            .await?;
                    }
                    existing_document_id.to_owned()
                }
                Err(DifyError::RemoteNotFound) => {
                    let document = client
                        .create_text_document(
                            &case_index.dataset_id,
                            &format!("9Profs extraction {}", extraction.id),
                            "9Profs retrieval bootstrap placeholder",
                        )
                        .await?;
                    client
                        .wait_for_batch(&case_index.dataset_id, &document.batch)
                        .await?;
                    for segment in client
                        .list_all_segments(&case_index.dataset_id, &document.id)
                        .await?
                    {
                        client
                            .delete_segment(&case_index.dataset_id, &document.id, &segment.id)
                            .await?;
                    }
                    document.id
                }
                Err(error) => return Err(error),
            }
        } else {
            let document = client
                .create_text_document(
                    &case_index.dataset_id,
                    &format!("9Profs extraction {}", extraction.id),
                    "9Profs retrieval bootstrap placeholder",
                )
                .await?;
            client
                .wait_for_batch(&case_index.dataset_id, &document.batch)
                .await?;
            for segment in client
                .list_all_segments(&case_index.dataset_id, &document.id)
                .await?
            {
                client
                    .delete_segment(&case_index.dataset_id, &document.id, &segment.id)
                    .await?;
            }
            document.id
        };
        if index.document_id.as_deref() != Some(document_id.as_str()) {
            self.update_document_id(&index.index_id, &document_id)
                .await?;
        }
        for chunk in &chunks {
            sqlx::query(
                "INSERT INTO research_retrieval_chunks \
                 (id, extraction_index_id, research_case_id, research_source_id, source_snapshot_id, extraction_id, \
                  page, start_offset, end_offset, text, hash_algorithm, text_hash) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&chunk.retrieval_chunk_id)
            .bind(&index.index_id)
            .bind(&chunk.research_case_id)
            .bind(&chunk.research_source_id)
            .bind(&chunk.source_snapshot_id)
            .bind(&chunk.extraction_id)
            .bind(chunk.page as i64)
            .bind(chunk.start as i64)
            .bind(chunk.end as i64)
            .bind(&chunk.text)
            .bind("sha256")
            .bind(&chunk.text_hash.value)
            .execute(&self.pool)
            .await?;
        }
        let batch_size = self
            .config
            .as_ref()
            .map_or(DEFAULT_BATCH_SIZE, |value| value.batch_size.clamp(1, 100));
        let mut mappings = Vec::new();
        for batch in chunks.chunks(batch_size) {
            let texts: Vec<String> = batch.iter().map(|chunk| chunk.text.clone()).collect();
            let segments = client
                .create_segments(&case_index.dataset_id, &document_id, &texts)
                .await?;
            if segments.len() != batch.len() {
                return Err(DifyError::MalformedResponse);
            }
            for (segment, chunk) in segments.into_iter().zip(batch) {
                mappings.push((segment.id, chunk.retrieval_chunk_id.clone()));
            }
        }
        let segments = client
            .wait_for_segments(&case_index.dataset_id, &document_id, chunks.len())
            .await?;
        let expected: std::collections::HashSet<&str> = mappings
            .iter()
            .map(|(segment_id, _)| segment_id.as_str())
            .collect();
        for segment in segments {
            if !expected.contains(segment.id.as_str()) {
                client
                    .delete_segment(&case_index.dataset_id, &document_id, &segment.id)
                    .await?;
            }
        }
        for (segment_id, chunk_id) in mappings {
            sqlx::query(
                "INSERT INTO research_dify_segment_mappings \
                 (dataset_id, document_id, segment_id, retrieval_chunk_id, created_at_ms) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&case_index.dataset_id)
            .bind(&document_id)
            .bind(segment_id)
            .bind(chunk_id)
            .bind(now_ms())
            .execute(&self.pool)
            .await?;
        }
        Ok(index.clone())
    }

    async fn begin_extraction_index(
        &self,
        case_index: &DifyCaseIndex,
        extraction: &nineprofs_research::ResearchPdfExtraction,
        snapshot: &nineprofs_research::ResearchSourceSnapshot,
    ) -> Result<DifyExtractionIndex, DifyError> {
        if let Some(existing) = self
            .extraction_index(&case_index.index_id, &extraction.id.to_string())
            .await?
        {
            self.clear_extraction_data(&existing.index_id).await?;
            self.update_extraction_status(&existing.index_id, &DifyIndexStatus::Syncing, None)
                .await?;
            return Ok(existing);
        }
        let timestamp = now_ms();
        let index = DifyExtractionIndex {
            index_id: new_id(),
            case_index_id: case_index.index_id.clone(),
            research_case_id: case_index.research_case_id.clone(),
            extraction_id: extraction.id.to_string(),
            source_snapshot_id: snapshot.id.to_string(),
            document_id: None,
            chunker_version: CHUNKER_VERSION.to_owned(),
            status: DifyIndexStatus::Syncing,
            failure_code: None,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        sqlx::query(
            "INSERT INTO research_dify_extraction_indexes \
             (id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, chunker_version, \
              status, failure_code, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&index.index_id)
        .bind(&index.case_index_id)
        .bind(&index.research_case_id)
        .bind(&index.extraction_id)
        .bind(&index.source_snapshot_id)
        .bind(Option::<String>::None)
        .bind(&index.chunker_version)
        .bind(status_text(&index.status))
        .bind(Option::<String>::None)
        .bind(timestamp)
        .bind(timestamp)
        .execute(&self.pool)
        .await?;
        Ok(index)
    }

    async fn case_index(&self, case_id: &str) -> Result<Option<DifyCaseIndex>, DifyError> {
        sqlx::query(
            "SELECT id, research_case_id, dataset_id, status, failure_code, created_at_ms, updated_at_ms \
             FROM research_dify_case_indexes WHERE research_case_id = ?",
        )
        .bind(case_id)
        .fetch_optional(&self.pool)
        .await?
        .map(map_case_index)
        .transpose()
    }

    async fn case_index_by_id(&self, id: &str) -> Result<Option<DifyCaseIndex>, DifyError> {
        sqlx::query(
            "SELECT id, research_case_id, dataset_id, status, failure_code, created_at_ms, updated_at_ms \
             FROM research_dify_case_indexes WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(map_case_index)
        .transpose()
    }

    async fn extraction_index(
        &self,
        case_index_id: &str,
        extraction_id: &str,
    ) -> Result<Option<DifyExtractionIndex>, DifyError> {
        sqlx::query(
            "SELECT id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, \
             chunker_version, status, failure_code, created_at_ms, updated_at_ms \
             FROM research_dify_extraction_indexes WHERE case_index_id = ? AND extraction_id = ? \
             AND chunker_version = ?",
        )
        .bind(case_index_id)
        .bind(extraction_id)
        .bind(CHUNKER_VERSION)
        .fetch_optional(&self.pool)
        .await?
        .map(map_extraction_index)
        .transpose()
    }

    async fn extraction_indexes(
        &self,
        case_index_id: &str,
    ) -> Result<Vec<DifyExtractionIndex>, DifyError> {
        sqlx::query(
            "SELECT id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, \
             chunker_version, status, failure_code, created_at_ms, updated_at_ms \
             FROM research_dify_extraction_indexes WHERE case_index_id = ? ORDER BY created_at_ms ASC, id ASC",
        )
        .bind(case_index_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(map_extraction_index)
        .collect()
    }

    async fn update_extraction_status(
        &self,
        id: &str,
        status: &DifyIndexStatus,
        failure_code: Option<&str>,
    ) -> Result<(), DifyError> {
        sqlx::query(
            "UPDATE research_dify_extraction_indexes SET status = ?, failure_code = ?, updated_at_ms = ? WHERE id = ?",
        )
        .bind(status_text(status))
        .bind(failure_code)
        .bind(now_ms())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_document_id(&self, id: &str, document_id: &str) -> Result<(), DifyError> {
        sqlx::query(
            "UPDATE research_dify_extraction_indexes SET document_id = ?, updated_at_ms = ? WHERE id = ?",
        )
        .bind(document_id)
        .bind(now_ms())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_extraction_data(&self, index_id: &str) -> Result<(), DifyError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "DELETE FROM research_dify_segment_mappings WHERE retrieval_chunk_id IN \
             (SELECT id FROM research_retrieval_chunks WHERE extraction_index_id = ?)",
        )
        .bind(index_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM research_retrieval_chunks WHERE extraction_index_id = ?")
            .bind(index_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn chunk_count(&self, index_id: &str) -> Result<u64, DifyError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS count FROM research_retrieval_chunks WHERE extraction_index_id = ?",
        )
        .bind(index_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("count") as u64)
    }

    async fn extraction_index_is_ready(&self, id: &str) -> Result<bool, DifyError> {
        let row = sqlx::query("SELECT status FROM research_dify_extraction_indexes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some_and(|row| row.get::<String, _>("status") == "ready"))
    }

    async fn mark_case_degraded(&self, case_id: &str) -> Result<(), DifyError> {
        sqlx::query(
            "UPDATE research_dify_case_indexes SET status = 'degraded', failure_code = 'index_drift', updated_at_ms = ? \
             WHERE research_case_id = ?",
        )
        .bind(now_ms())
        .bind(case_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn publish(&self, name: &'static str, payload: Value) {
        let _ = self.events.publish(EventEnvelope::new(name, payload));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetrievalChunk {
    pub retrieval_chunk_id: String,
    pub research_case_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub text: String,
    pub text_hash: ContentHash,
}

pub fn chunk_pages(
    pages: &[ResearchPdfPage],
    research_case_id: &str,
    research_source_id: &str,
    source_snapshot_id: &str,
    extraction_id: &str,
) -> Result<Vec<RetrievalChunk>, DifyError> {
    let mut chunks = Vec::new();
    for page in pages {
        let chars: Vec<char> = page.text.chars().collect();
        let mut start = 0usize;
        while start < chars.len() {
            let end = choose_chunk_end(&chars, start);
            if end <= start {
                return Err(DifyError::Invalid(
                    "chunker produced an empty range".to_owned(),
                ));
            }
            let text: String = chars[start..end].iter().collect();
            let text_hash = ContentHash {
                algorithm: HashAlgorithm::Sha256,
                value: sha256_hex(text.as_bytes()),
            };
            let identity = format!(
                "{extraction_id}\n{CHUNKER_VERSION}\n{}\n{start}\n{end}\n{}",
                page.page, text_hash.value
            );
            let chunk_id = format!("retrieval_chunk_{}", sha256_hex(identity.as_bytes()));
            chunks.push(RetrievalChunk {
                retrieval_chunk_id: chunk_id,
                research_case_id: research_case_id.to_owned(),
                research_source_id: research_source_id.to_owned(),
                source_snapshot_id: source_snapshot_id.to_owned(),
                extraction_id: extraction_id.to_owned(),
                page: page.page,
                start: start as u64,
                end: end as u64,
                text,
                text_hash,
            });
            if end == chars.len() {
                break;
            }
            start = end.saturating_sub(CHUNK_OVERLAP_CODE_POINTS);
        }
    }
    Ok(chunks)
}

fn choose_chunk_end(chars: &[char], start: usize) -> usize {
    let max_end = (start + CHUNK_MAX_CODE_POINTS).min(chars.len());
    if max_end == chars.len() {
        return max_end;
    }
    let target = (start + CHUNK_TARGET_CODE_POINTS).min(max_end);
    let minimum = start + ((CHUNK_TARGET_CODE_POINTS / 2).min(max_end.saturating_sub(start)));
    (minimum..=max_end)
        .rev()
        .find(|index| {
            chars
                .get(index.saturating_sub(1))
                .is_some_and(|value| value.is_whitespace())
        })
        .unwrap_or(target.max(minimum))
}

fn slice_code_points(text: &str, start: u64, end: u64) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    (start < end && end <= chars.len()).then(|| chars[start..end].iter().collect())
}

fn canonical_excerpt(
    page: &ResearchPdfPage,
    start: u64,
    end: u64,
    stored_text: &str,
    stored_hash: &str,
) -> Result<String, DifyError> {
    let excerpt = slice_code_points(&page.text, start, end).ok_or(DifyError::Integrity)?;
    if excerpt != stored_text || sha256_hex(excerpt.as_bytes()) != stored_hash {
        return Err(DifyError::Integrity);
    }
    Ok(excerpt)
}

fn bounded_query(query: &str) -> Result<String, DifyError> {
    let chars: Vec<char> = query.chars().collect();
    if chars.is_empty() || chars.len() > MAX_QUERY_CODE_POINTS {
        return Err(DifyError::Invalid(format!(
            "query must contain 1-{MAX_QUERY_CODE_POINTS} Unicode code points"
        )));
    }
    Ok(chars.into_iter().collect())
}

fn sha256_hex(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn map_reqwest_error(error: reqwest::Error) -> DifyError {
    if error.is_timeout() {
        DifyError::Timeout
    } else {
        DifyError::Unreachable
    }
}

fn map_status(status: StatusCode) -> DifyError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => DifyError::Unauthorized,
        StatusCode::NOT_FOUND => DifyError::RemoteNotFound,
        StatusCode::TOO_MANY_REQUESTS => DifyError::RateLimited,
        StatusCode::BAD_REQUEST => DifyError::Invalid("Dify rejected request".to_owned()),
        status if status.is_server_error() => DifyError::Unreachable,
        _ => DifyError::Invalid("Dify request failed".to_owned()),
    }
}

fn map_remote_error(status: StatusCode, body: &[u8]) -> DifyError {
    if let Ok(response) = serde_json::from_slice::<DifyErrorResponse>(body) {
        match response.code.as_deref() {
            Some("provider_not_initialize") | Some("provider_not_initialized") => {
                return DifyError::ProviderNotInitialized;
            }
            Some("dataset_not_initialized") => return DifyError::IndexingFailed,
            _ => {}
        }
    }
    map_status(status)
}

fn status_text(status: &DifyIndexStatus) -> &'static str {
    match status {
        DifyIndexStatus::NotConfigured => "not_configured",
        DifyIndexStatus::Provisioning => "provisioning",
        DifyIndexStatus::Ready => "ready",
        DifyIndexStatus::Syncing => "syncing",
        DifyIndexStatus::Failed => "failed",
        DifyIndexStatus::Degraded => "degraded",
    }
}

fn status_for_error(error: &DifyError) -> DifyIndexStatus {
    if matches!(error, DifyError::IndexDrift | DifyError::Integrity) {
        DifyIndexStatus::Degraded
    } else {
        DifyIndexStatus::Failed
    }
}

fn error_code(error: &DifyError) -> &'static str {
    match error {
        DifyError::NotConfigured => "not_configured",
        DifyError::Unreachable => "unreachable",
        DifyError::Unauthorized => "unauthorized",
        DifyError::RateLimited => "rate_limited",
        DifyError::ProviderNotInitialized => "provider_not_initialized",
        DifyError::IndexingFailed => "indexing_failed",
        DifyError::Timeout => "timeout",
        DifyError::MalformedResponse => "malformed_response",
        DifyError::RemoteNotFound => "remote_not_found",
        DifyError::IndexDrift => "index_drift",
        DifyError::Integrity => "integrity_failure",
        DifyError::Invalid(_) => "invalid_request",
        DifyError::Database(_) => "database_error",
        DifyError::Research(_) => "research_error",
    }
}

fn dataset_name(case_id: &str) -> String {
    let value = format!("9Profs research {case_id}");
    value.chars().take(40).collect()
}

fn map_case_index(row: sqlx::sqlite::SqliteRow) -> Result<DifyCaseIndex, DifyError> {
    Ok(DifyCaseIndex {
        index_id: row.get("id"),
        research_case_id: row.get("research_case_id"),
        dataset_id: row.get("dataset_id"),
        status: parse_status(row.get::<String, _>("status"))?,
        failure_code: row.get("failure_code"),
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}

fn map_extraction_index(row: sqlx::sqlite::SqliteRow) -> Result<DifyExtractionIndex, DifyError> {
    Ok(DifyExtractionIndex {
        index_id: row.get("id"),
        case_index_id: row.get("case_index_id"),
        research_case_id: row.get("research_case_id"),
        extraction_id: row.get("extraction_id"),
        source_snapshot_id: row.get("source_snapshot_id"),
        document_id: row.get("document_id"),
        chunker_version: row.get("chunker_version"),
        status: parse_status(row.get::<String, _>("status"))?,
        failure_code: row.get("failure_code"),
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}

fn parse_status(value: String) -> Result<DifyIndexStatus, DifyError> {
    match value.as_str() {
        "not_configured" => Ok(DifyIndexStatus::NotConfigured),
        "provisioning" => Ok(DifyIndexStatus::Provisioning),
        "ready" => Ok(DifyIndexStatus::Ready),
        "syncing" => Ok(DifyIndexStatus::Syncing),
        "failed" => Ok(DifyIndexStatus::Failed),
        "degraded" => Ok(DifyIndexStatus::Degraded),
        _ => Err(DifyError::MalformedResponse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn page(text: &str, number: u32) -> ResearchPdfPage {
        ResearchPdfPage {
            extraction_id: nineprofs_research::ResearchPdfExtractionId::new(),
            page: number,
            text: text.to_owned(),
            text_hash: ContentHash {
                algorithm: HashAlgorithm::Sha256,
                value: sha256_hex(text.as_bytes()),
            },
        }
    }

    #[test]
    fn chunking_is_deterministic_page_local_and_exact() {
        let pages = vec![
            page(&"Đoạn nghiên cứu. ".repeat(160), 1),
            page("lặp lại", 2),
        ];
        let first = chunk_pages(&pages, "case", "source", "snapshot", "extraction").unwrap();
        let second = chunk_pages(&pages, "case", "source", "snapshot", "extraction").unwrap();
        let rebuilt = chunk_pages(&pages, "case", "source", "snapshot", "extraction").unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|chunk| &chunk.retrieval_chunk_id)
                .collect::<Vec<_>>(),
            rebuilt
                .iter()
                .map(|chunk| &chunk.retrieval_chunk_id)
                .collect::<Vec<_>>()
        );
        assert!(first.iter().all(|chunk| chunk.start < chunk.end));
        for chunk in &first {
            let source = &pages[chunk.page as usize - 1].text;
            assert_eq!(
                slice_code_points(source, chunk.start, chunk.end).unwrap(),
                chunk.text
            );
        }
        assert!(first.iter().all(|chunk| chunk.page == 1 || chunk.page == 2));
    }

    #[test]
    fn query_and_config_do_not_leak_secret() {
        let config = DifyConfig::new("http://dify.test/v1", "secret-value");
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret-value"));
        assert!(bounded_query("治疗效果").is_ok());
        assert!(bounded_query("").is_err());
    }

    #[test]
    fn canonical_resolution_rejects_hash_mismatch() {
        let page = page("canonical excerpt", 1);
        let hash = sha256_hex("canonical excerpt".as_bytes());
        assert_eq!(
            canonical_excerpt(&page, 0, 17, "canonical excerpt", &hash).unwrap(),
            "canonical excerpt"
        );
        assert!(matches!(
            canonical_excerpt(&page, 0, 17, "canonical excerpt", "wrong-hash"),
            Err(DifyError::Integrity)
        ));
        assert!(matches!(
            canonical_excerpt(&page, 0, 17, "remote spoof", &hash),
            Err(DifyError::Integrity)
        ));
    }

    #[test]
    fn remote_error_codes_are_normalized_without_body_leakage() {
        let error = map_remote_error(
            StatusCode::BAD_REQUEST,
            br#"{"code":"provider_not_initialize","message":"sensitive"}"#,
        );
        assert!(matches!(error, DifyError::ProviderNotInitialized));
        assert!(!error.to_string().contains("sensitive"));
    }

    #[tokio::test]
    async fn mock_retrieve_uses_only_segment_id_and_score() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    panic!("mock request ended before headers");
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    break position + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then_some(value)
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("POST /v1/datasets/dataset/retrieve HTTP/1.1"));
            assert!(request.contains("\"query\":\"query\""));
            let body = r#"{"records":[{"score":0.91,"segment":{"id":"segment-1","content":"remote spoof"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let client =
            DifyClient::new(DifyConfig::new(format!("http://{address}/v1"), "test-key")).unwrap();
        let hits = client.retrieve("dataset", "query", 1).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].segment_id, "segment-1");
        assert_eq!(hits[0].score, 0.91);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn real_dify_qualification_is_explicitly_opt_in() {
        if std::env::var("NINEPROFS_DIFY_QUALIFICATION").as_deref() != Ok("1") {
            return;
        }
        let config = DifyConfig::from_env().expect("Dify base URL and API key are required");
        let dataset_id = std::env::var("NINEPROFS_DIFY_QUALIFICATION_DATASET_ID")
            .expect("qualification dataset id is required");
        let query = std::env::var("NINEPROFS_DIFY_QUALIFICATION_QUERY")
            .unwrap_or_else(|_| "qualification".to_owned());
        let client = DifyClient::new(config).expect("Dify HTTP client should initialize");
        client
            .get_dataset(&dataset_id)
            .await
            .expect("configured Dify dataset should be reachable");
        client
            .retrieve(&dataset_id, &query, 1)
            .await
            .expect("configured Dify dataset should support retrieval");
    }
}
