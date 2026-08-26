//! Rebuildable Dify retrieval index for the provider-neutral research domain.
//!
//! Dify stores derived search data only. Local chunk/range rows and the
//! `ResearchService` remain canonical for provenance and excerpt resolution.

use std::{fmt, sync::Arc, time::Duration};

use nineprofs_api_types::EventEnvelope;
use nineprofs_common::{new_id, now_ms};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    ContentHash, HashAlgorithm, ResearchError, ResearchPdfPage, ResearchRetrievalScope,
    ResearchService,
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
pub const DIFY_EXTRACTION_METADATA_FIELD: &str = "nineprofs_extraction_id";
pub const DIFY_SOURCE_METADATA_FIELD: &str = "nineprofs_source_id";
pub const DIFY_SNAPSHOT_METADATA_FIELD: &str = "nineprofs_snapshot_id";

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
    pub status: DifyReadinessStatus,
    pub reachable: bool,
    pub authorized: bool,
    pub ready: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DifyReadinessStatus {
    NotConfigured,
    Configured,
    Unreachable,
    Reachable,
    Unauthorized,
    Ready,
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
    pub metadata_qualified: bool,
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
struct DifyDatasetListResponse {
    #[serde(default, rename = "data")]
    _data: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct DifyMetadataField {
    id: String,
    name: String,
    #[serde(rename = "type")]
    field_type: String,
}

#[derive(Debug, Deserialize)]
struct DifyMetadataListResponse {
    #[serde(default)]
    doc_metadata: Vec<DifyMetadataField>,
}

#[derive(Debug, Deserialize)]
struct DifyDocumentMetadata {
    id: String,
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    #[serde(default)]
    value: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct DifyDocumentMetadataResponse {
    id: String,
    #[serde(default)]
    doc_metadata: Vec<DifyDocumentMetadata>,
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

    #[cfg(test)]
    async fn delete_dataset(&self, dataset_id: &str) -> Result<(), DifyError> {
        self.send(Method::DELETE, &format!("datasets/{dataset_id}"), None)
            .await
            .map(|_| ())
    }

    async fn probe(&self) -> Result<(), DifyError> {
        let _: DifyDatasetListResponse = self
            .send_json(Method::GET, "datasets?page=1&limit=1", None)
            .await?;
        Ok(())
    }

    async fn list_metadata_fields(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<DifyMetadataField>, DifyError> {
        let response: DifyMetadataListResponse = self
            .send_json(
                Method::GET,
                &format!("datasets/{dataset_id}/metadata"),
                None,
            )
            .await?;
        Ok(response.doc_metadata)
    }

    async fn create_metadata_field(
        &self,
        dataset_id: &str,
        name: &str,
    ) -> Result<DifyMetadataField, DifyError> {
        self.send_json(
            Method::POST,
            &format!("datasets/{dataset_id}/metadata"),
            Some(json!({"name": name, "type": "string"})),
        )
        .await
    }

    async fn ensure_metadata_fields(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<DifyMetadataField>, DifyError> {
        let mut fields = self.list_metadata_fields(dataset_id).await?;
        let mut required = Vec::with_capacity(3);
        for name in [
            DIFY_EXTRACTION_METADATA_FIELD,
            DIFY_SOURCE_METADATA_FIELD,
            DIFY_SNAPSHOT_METADATA_FIELD,
        ] {
            let matches: Vec<_> = fields.iter().filter(|field| field.name == name).collect();
            let field = match matches.as_slice() {
                [field] if field.field_type == "string" => (*field).clone(),
                [] => {
                    let field = self.create_metadata_field(dataset_id, name).await?;
                    if field.name != name || field.field_type != "string" {
                        return Err(DifyError::IndexDrift);
                    }
                    fields.push(field.clone());
                    field
                }
                _ => return Err(DifyError::IndexDrift),
            };
            required.push(field);
        }
        Ok(required)
    }

    async fn update_document_metadata(
        &self,
        dataset_id: &str,
        document_id: &str,
        fields: &[DifyMetadataField],
        extraction_id: &str,
        source_id: &str,
        snapshot_id: &str,
    ) -> Result<(), DifyError> {
        let values = [
            (DIFY_EXTRACTION_METADATA_FIELD, extraction_id),
            (DIFY_SOURCE_METADATA_FIELD, source_id),
            (DIFY_SNAPSHOT_METADATA_FIELD, snapshot_id),
        ];
        let metadata_list: Vec<Value> = fields
            .iter()
            .zip(values)
            .map(|(field, (expected_name, value))| {
                json!({"id": field.id, "name": expected_name, "value": value})
            })
            .collect();
        self.send(
            Method::POST,
            &format!("datasets/{dataset_id}/documents/metadata"),
            Some(json!({
                "operation_data": [{
                    "document_id": document_id,
                    "metadata_list": metadata_list,
                    "partial_update": true
                }]
            })),
        )
        .await
        .map(|_| ())
    }

    async fn document_metadata(
        &self,
        dataset_id: &str,
        document_id: &str,
    ) -> Result<DifyDocumentMetadataResponse, DifyError> {
        self.send_json(
            Method::GET,
            &format!("datasets/{dataset_id}/documents/{document_id}?metadata=only"),
            None,
        )
        .await
    }

    fn verify_document_metadata(
        response: &DifyDocumentMetadataResponse,
        document_id: &str,
        fields: &[DifyMetadataField],
        extraction_id: &str,
        source_id: &str,
        snapshot_id: &str,
    ) -> Result<(), DifyError> {
        let expected = [
            (DIFY_EXTRACTION_METADATA_FIELD, extraction_id),
            (DIFY_SOURCE_METADATA_FIELD, source_id),
            (DIFY_SNAPSHOT_METADATA_FIELD, snapshot_id),
        ];
        if response.id.is_empty() {
            return Err(DifyError::MalformedResponse);
        }
        if response.id != document_id || fields.len() != expected.len() {
            return Err(DifyError::IndexDrift);
        }
        for ((name, value), expected_field) in expected.into_iter().zip(fields) {
            let Some(field) = response
                .doc_metadata
                .iter()
                .find(|field| field.name == name)
            else {
                return Err(DifyError::IndexDrift);
            };
            if field.id != expected_field.id
                || field.field_type != "string"
                || field.value.as_ref().and_then(Value::as_str) != Some(value)
            {
                return Err(DifyError::IndexDrift);
            }
        }
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
        extraction_ids: Option<&[String]>,
    ) -> Result<Vec<DifyHit>, DifyError> {
        let retrieval_model = match extraction_ids {
            None => json!({"top_k": top_k}),
            Some([extraction_id]) => json!({
                "top_k": top_k,
                "metadata_filtering_conditions": {
                    "logical_operator": "and",
                    "conditions": [{
                        "name": DIFY_EXTRACTION_METADATA_FIELD,
                        "comparison_operator": "is",
                        "value": extraction_id
                    }]
                }
            }),
            Some(extraction_ids) => json!({
                "top_k": top_k,
                "metadata_filtering_conditions": {
                    "logical_operator": "and",
                    "conditions": [{
                        "name": DIFY_EXTRACTION_METADATA_FIELD,
                        "comparison_operator": "in",
                        "value": extraction_ids
                    }]
                }
            }),
        };
        let response: DifyRetrieveResponse = self
            .send_json(
                Method::POST,
                &format!("datasets/{dataset_id}/retrieve"),
                Some(json!({
                    "query": query,
                    "retrieval_model": retrieval_model
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
            status: if self.config.is_some() {
                DifyReadinessStatus::Configured
            } else {
                DifyReadinessStatus::NotConfigured
            },
            reachable: false,
            authorized: false,
            ready: false,
        }
    }

    pub async fn qualified_readiness(&self) -> DifyReadiness {
        let mut readiness = self.readiness();
        let Some(client) = self.client.as_ref() else {
            return readiness;
        };
        match client.probe().await {
            Ok(()) => {
                readiness.status = DifyReadinessStatus::Ready;
                readiness.reachable = true;
                readiness.authorized = true;
                readiness.ready = true;
            }
            Err(DifyError::Unauthorized) => {
                readiness.status = DifyReadinessStatus::Unauthorized;
                readiness.reachable = true;
            }
            Err(DifyError::Unreachable | DifyError::Timeout) => {
                readiness.status = DifyReadinessStatus::Unreachable;
            }
            Err(_) => {
                readiness.status = DifyReadinessStatus::Reachable;
                readiness.reachable = true;
                readiness.authorized = true;
            }
        }
        readiness
    }

    pub async fn state(&self, research_case_id: &str) -> Result<RetrievalIndexState, DifyError> {
        let case_index = self.case_index(research_case_id).await?;
        let extraction_indexes = match case_index.as_ref() {
            Some(index) => self.extraction_indexes(&index.index_id).await?,
            None => Vec::new(),
        };
        Ok(RetrievalIndexState {
            readiness: self.qualified_readiness().await,
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
            self.ensure_metadata_fields(&index).await?;
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
        self.ensure_metadata_fields(&index).await?;
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
            && existing.metadata_qualified
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
                self.update_extraction_metadata_qualified(&index.index_id, true)
                    .await?;
                self.update_extraction_status(&index.index_id, &DifyIndexStatus::Ready, None)
                    .await?;
                let mut index = index;
                index.metadata_qualified = true;
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
        self.retrieve_with_scope(
            research_case_id,
            &ResearchRetrievalScope::Case,
            query,
            top_k,
        )
        .await
    }

    pub async fn retrieve_from_extractions(
        &self,
        research_case_id: &str,
        extraction_ids: &[String],
        query: &str,
        top_k: u32,
    ) -> Result<Vec<RetrievalCandidate>, DifyError> {
        let extraction_ids = extraction_ids
            .iter()
            .map(|id| nineprofs_research::ResearchPdfExtractionId::parse(id.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        self.retrieve_with_scope(
            research_case_id,
            &ResearchRetrievalScope::Extractions { extraction_ids },
            query,
            top_k,
        )
        .await
    }

    pub async fn retrieve_with_scope(
        &self,
        research_case_id: &str,
        scope: &ResearchRetrievalScope,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<RetrievalCandidate>, DifyError> {
        let client = self.client.as_ref().ok_or(DifyError::NotConfigured)?;
        scope.validate().map_err(DifyError::Research)?;
        let query = bounded_query(query)?;
        let top_k = top_k.clamp(1, MAX_TOP_K);
        let case_index = self
            .case_index(research_case_id)
            .await?
            .ok_or(DifyError::RemoteNotFound)?;
        let scoped_extraction_ids = self.resolve_scope(&case_index, scope).await?;
        let hits = client
            .retrieve(
                &case_index.dataset_id,
                &query,
                top_k,
                scoped_extraction_ids.as_deref(),
            )
            .await?;
        let mut candidates = Vec::new();
        for (rank, hit) in hits.into_iter().take(top_k as usize).enumerate() {
            let Some(row) = sqlx::query(
                "SELECT c.id, c.research_source_id, c.source_snapshot_id, c.extraction_id, \
                 c.page, c.start_offset, c.end_offset, c.text, c.text_hash, c.extraction_index_id \
                 FROM research_dify_segment_mappings m \
                 JOIN research_retrieval_chunks c ON c.id = m.retrieval_chunk_id \
                 JOIN research_dify_extraction_indexes i ON i.id = c.extraction_index_id \
                 WHERE m.dataset_id = ? AND m.segment_id = ? \
                   AND m.document_id = i.document_id \
                   AND i.case_index_id = ? AND i.research_case_id = ? \
                   AND c.research_case_id = i.research_case_id \
                   AND c.extraction_id = i.extraction_id \
                   AND c.source_snapshot_id = i.source_snapshot_id",
            )
            .bind(&case_index.dataset_id)
            .bind(&hit.segment_id)
            .bind(&case_index.index_id)
            .bind(research_case_id)
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
            if let Some(scoped_extraction_ids) = scoped_extraction_ids.as_ref()
                && !scoped_extraction_ids.contains(&extraction_id)
            {
                self.mark_extraction_degraded(&extraction_index_id).await?;
                self.mark_case_degraded(research_case_id).await?;
                return Err(DifyError::IndexDrift);
            }
            let page_number: u32 = row.get::<i64, _>("page") as u32;
            let start: u64 = row.get::<i64, _>("start_offset") as u64;
            let end: u64 = row.get::<i64, _>("end_offset") as u64;
            let stored_text: String = row.get("text");
            let stored_hash: String = row.get("text_hash");
            let page = self
                .research
                .get_pdf_page(&extraction_id, page_number)
                .await?;
            let excerpt = match canonical_excerpt(&page, start, end, &stored_text, &stored_hash) {
                Ok(excerpt) => excerpt,
                Err(error) => {
                    self.mark_extraction_degraded(&extraction_index_id).await?;
                    self.mark_case_degraded(research_case_id).await?;
                    return Err(error);
                }
            };
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
        let metadata_fields = self.ensure_metadata_fields(case_index).await?;
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
        client
            .update_document_metadata(
                &case_index.dataset_id,
                &document_id,
                &metadata_fields,
                &index.extraction_id,
                source_id,
                &index.source_snapshot_id,
            )
            .await?;
        let metadata = client
            .document_metadata(&case_index.dataset_id, &document_id)
            .await?;
        DifyClient::verify_document_metadata(
            &metadata,
            &document_id,
            &metadata_fields,
            &index.extraction_id,
            source_id,
            &index.source_snapshot_id,
        )?;
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
        let mut index = index.clone();
        index.document_id = Some(document_id);
        Ok(index)
    }

    async fn resolve_scope(
        &self,
        case_index: &DifyCaseIndex,
        scope: &ResearchRetrievalScope,
    ) -> Result<Option<Vec<String>>, DifyError> {
        match scope {
            ResearchRetrievalScope::Case => Ok(None),
            ResearchRetrievalScope::Extractions { extraction_ids } => {
                let mut resolved = Vec::with_capacity(extraction_ids.len());
                for extraction_id in extraction_ids {
                    let index = self
                        .require_scoped_extraction(case_index, extraction_id.as_str())
                        .await?;
                    resolved.push(index.extraction_id);
                }
                Ok(Some(resolved))
            }
            ResearchRetrievalScope::Sources { source_ids } => {
                let indexes = self.extraction_indexes(&case_index.index_id).await?;
                let mut resolved = Vec::with_capacity(source_ids.len());
                for source_id in source_ids {
                    let source = self.research.get_source(source_id.as_str()).await?;
                    if source.research_case_id.as_str() != case_index.research_case_id {
                        return Err(DifyError::Invalid(
                            "research source does not belong to research case".to_owned(),
                        ));
                    }
                    let mut matches = Vec::new();
                    for index in &indexes {
                        if !matches!(index.status, DifyIndexStatus::Ready)
                            || !index.metadata_qualified
                            || index.document_id.is_none()
                        {
                            continue;
                        }
                        let extraction = self
                            .research
                            .get_pdf_extraction_by_id(&index.extraction_id)
                            .await?;
                        let snapshot = self
                            .research
                            .get_snapshot(&extraction.source_snapshot_id.to_string())
                            .await?;
                        if snapshot.source_id == source.id {
                            matches.push(index.extraction_id.clone());
                        }
                    }
                    match matches.as_slice() {
                        [extraction_id] => resolved.push(
                            self.require_scoped_extraction(case_index, extraction_id)
                                .await?
                                .extraction_id,
                        ),
                        [] => {
                            return Err(DifyError::IndexingFailed);
                        }
                        _ => {
                            return Err(DifyError::Invalid(
                                "source retrieval scope is ambiguous; use exact extraction IDs"
                                    .to_owned(),
                            ));
                        }
                    }
                }
                Ok(Some(resolved))
            }
        }
    }

    async fn require_scoped_extraction(
        &self,
        case_index: &DifyCaseIndex,
        extraction_id: &str,
    ) -> Result<DifyExtractionIndex, DifyError> {
        let extraction = self
            .research
            .get_pdf_extraction_by_id(extraction_id)
            .await?;
        let snapshot = self
            .research
            .get_snapshot(&extraction.source_snapshot_id.to_string())
            .await?;
        let source = self
            .research
            .get_source(&snapshot.source_id.to_string())
            .await?;
        if source.research_case_id.as_str() != case_index.research_case_id {
            return Err(DifyError::Invalid(
                "PDF extraction does not belong to research case".to_owned(),
            ));
        }
        let index = self
            .extraction_index(&case_index.index_id, extraction_id)
            .await?
            .ok_or(DifyError::IndexingFailed)?;
        if index.research_case_id != case_index.research_case_id
            || index.source_snapshot_id != extraction.source_snapshot_id.to_string()
            || !matches!(index.status, DifyIndexStatus::Ready)
            || !index.metadata_qualified
            || index.document_id.is_none()
        {
            return Err(DifyError::IndexingFailed);
        }
        Ok(index)
    }

    async fn ensure_metadata_fields(
        &self,
        case_index: &DifyCaseIndex,
    ) -> Result<Vec<DifyMetadataField>, DifyError> {
        let client = self.client.as_ref().ok_or(DifyError::NotConfigured)?;
        let fields = client
            .ensure_metadata_fields(&case_index.dataset_id)
            .await?;
        for field in &fields {
            let existing = sqlx::query(
                "SELECT field_id FROM research_dify_metadata_fields WHERE dataset_id = ? AND field_name = ?",
            )
            .bind(&case_index.dataset_id)
            .bind(&field.name)
            .fetch_optional(&self.pool)
            .await?;
            if existing.is_some_and(|row| row.get::<String, _>("field_id") != field.id) {
                return Err(DifyError::IndexDrift);
            }
            let timestamp = now_ms();
            sqlx::query(
                "INSERT INTO research_dify_metadata_fields \
                 (dataset_id, field_name, field_id, field_type, created_at_ms, updated_at_ms) \
                 VALUES (?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(dataset_id, field_name) DO UPDATE SET field_id = excluded.field_id, \
                 field_type = excluded.field_type, updated_at_ms = excluded.updated_at_ms",
            )
            .bind(&case_index.dataset_id)
            .bind(&field.name)
            .bind(&field.id)
            .bind(&field.field_type)
            .bind(timestamp)
            .bind(timestamp)
            .execute(&self.pool)
            .await?;
        }
        Ok(fields)
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
            metadata_qualified: false,
            chunker_version: CHUNKER_VERSION.to_owned(),
            status: DifyIndexStatus::Syncing,
            failure_code: None,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        sqlx::query(
            "INSERT INTO research_dify_extraction_indexes \
             (id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, metadata_qualified, chunker_version, \
              status, failure_code, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&index.index_id)
        .bind(&index.case_index_id)
        .bind(&index.research_case_id)
        .bind(&index.extraction_id)
            .bind(&index.source_snapshot_id)
            .bind(Option::<String>::None)
            .bind(false)
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
            "SELECT id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, metadata_qualified, \
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
            "SELECT id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, metadata_qualified, \
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

    async fn update_extraction_metadata_qualified(
        &self,
        id: &str,
        qualified: bool,
    ) -> Result<(), DifyError> {
        sqlx::query(
            "UPDATE research_dify_extraction_indexes SET metadata_qualified = ?, updated_at_ms = ? WHERE id = ?",
        )
        .bind(qualified)
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

    async fn mark_extraction_degraded(&self, index_id: &str) -> Result<(), DifyError> {
        sqlx::query(
            "UPDATE research_dify_extraction_indexes SET status = 'degraded', failure_code = 'index_drift', updated_at_ms = ? WHERE id = ?",
        )
        .bind(now_ms())
        .bind(index_id)
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
        metadata_qualified: row.get::<i64, _>("metadata_qualified") != 0,
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
    use nineprofs_db::Database;
    use nineprofs_research::{ResearchService, SqliteResearchRepository};
    use serde_json::json;
    use std::sync::Arc;
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

    async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 2048];
        let header_end = loop {
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0, "mock request ended before headers");
            request.extend_from_slice(&buffer[..read]);
            if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then_some(value.trim().parse::<usize>().ok()?)
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0, "mock request ended before body");
            request.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8(request).unwrap()
    }

    async fn mock_response_server(
        responses: Vec<(u16, Value)>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::with_capacity(responses.len());
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                requests.push(read_request(&mut stream).await);
                let body = serde_json::to_string(&body).unwrap();
                let reason = if status == 200 { "OK" } else { "Unauthorized" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (format!("http://{address}/v1"), server)
    }

    async fn retrieval_fixture() -> (SqlitePool, Arc<ResearchService>) {
        let database = Database::in_memory().await.unwrap();
        let pool = database.pool().clone();
        for case_id in ["case-a", "case-b"] {
            sqlx::query(
                "INSERT INTO research_cases (id, title, created_at_ms, updated_at_ms) VALUES (?, ?, 1, 1)",
            )
            .bind(case_id)
            .bind(case_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (source_id, case_id) in [
            ("source-a", "case-a"),
            ("source-a2", "case-a"),
            ("source-b", "case-a"),
            ("source-cross", "case-b"),
        ] {
            sqlx::query(
                "INSERT INTO research_sources (id, research_case_id, kind, label, created_at_ms) VALUES (?, ?, 'reference_pdf', ?, 1)",
            )
            .bind(source_id)
            .bind(case_id)
            .bind(source_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (snapshot_id, source_id, artifact_id, content_hash) in [
            ("snapshot-ea", "source-a", "artifact-ea", "hash-ea"),
            ("snapshot-ea2", "source-a2", "artifact-ea2", "hash-ea2"),
            ("snapshot-eb", "source-b", "artifact-eb", "hash-eb"),
            (
                "snapshot-cross",
                "source-cross",
                "artifact-cross",
                "hash-cross",
            ),
        ] {
            sqlx::query(
                "INSERT INTO research_artifacts (id, hash_algorithm, content_hash, size_bytes, media_type, original_filename, created_at_ms) VALUES (?, 'sha256', ?, 1, 'application/pdf', ?, 1)",
            )
            .bind(artifact_id)
            .bind(content_hash)
            .bind(format!("{artifact_id}.pdf"))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_source_snapshots (id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, origin_json, metadata_json) VALUES (?, ?, 'sha256', ?, 1, 'user_provided', ?, '{}')",
            )
            .bind(snapshot_id)
            .bind(source_id)
            .bind(content_hash)
            .bind(
                serde_json::to_string(&json!({
                    "kind": "external_import",
                    "provider": "fixture",
                    "external_reference": artifact_id
                }))
                .unwrap(),
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        for (extraction_id, snapshot_id, artifact_id, text) in [
            (
                "extraction-ea",
                "snapshot-ea",
                "artifact-ea",
                "evidence from EA",
            ),
            (
                "extraction-ea2",
                "snapshot-ea2",
                "artifact-ea2",
                "evidence from EA2",
            ),
            (
                "extraction-eb",
                "snapshot-eb",
                "artifact-eb",
                "evidence from EB",
            ),
            (
                "extraction-cross",
                "snapshot-cross",
                "artifact-cross",
                "evidence from cross-case",
            ),
        ] {
            sqlx::query(
                "INSERT INTO research_pdf_extractions (id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, hash_algorithm, extraction_hash, extracted_at_ms, status) VALUES (?, ?, ?, 'fixture', '1', 1, 'sha256', ?, 1, 'ready')",
            )
            .bind(extraction_id)
            .bind(snapshot_id)
            .bind(artifact_id)
            .bind(format!("hash-{extraction_id}"))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_pdf_pages (extraction_id, page, text, hash_algorithm, text_hash) VALUES (?, 1, ?, 'sha256', ?)",
            )
            .bind(extraction_id)
            .bind(text)
            .bind(sha256_hex(text.as_bytes()))
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO research_dify_case_indexes (id, research_case_id, dataset_id, status, created_at_ms, updated_at_ms) VALUES ('index-a', 'case-a', 'dataset-a', 'ready', 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (index_id, extraction_id, snapshot_id, source_id, segment_id, text) in [
            (
                "index-ea",
                "extraction-ea",
                "snapshot-ea",
                "source-a",
                "segment-ea",
                "evidence from EA",
            ),
            (
                "index-ea2",
                "extraction-ea2",
                "snapshot-ea2",
                "source-a2",
                "segment-ea2",
                "evidence from EA2",
            ),
            (
                "index-eb",
                "extraction-eb",
                "snapshot-eb",
                "source-b",
                "segment-eb",
                "evidence from EB",
            ),
        ] {
            sqlx::query(
                "INSERT INTO research_dify_extraction_indexes (id, case_index_id, research_case_id, extraction_id, source_snapshot_id, document_id, metadata_qualified, chunker_version, status, created_at_ms, updated_at_ms) VALUES (?, 'index-a', 'case-a', ?, ?, ?, 1, ?, 'ready', 1, 1)",
            )
            .bind(index_id)
            .bind(extraction_id)
            .bind(snapshot_id)
            .bind(format!("document-{extraction_id}"))
            .bind(CHUNKER_VERSION)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_retrieval_chunks (id, extraction_index_id, research_case_id, research_source_id, source_snapshot_id, extraction_id, page, start_offset, end_offset, text, hash_algorithm, text_hash) VALUES (?, ?, 'case-a', ?, ?, ?, 1, 0, ?, ?, 'sha256', ?)",
            )
            .bind(format!("chunk-{extraction_id}"))
            .bind(index_id)
            .bind(source_id)
            .bind(snapshot_id)
            .bind(extraction_id)
            .bind(text.chars().count() as i64)
            .bind(text)
            .bind(sha256_hex(text.as_bytes()))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_dify_segment_mappings (dataset_id, document_id, segment_id, retrieval_chunk_id, created_at_ms) VALUES ('dataset-a', ?, ?, ?, 1)",
            )
            .bind(format!("document-{extraction_id}"))
            .bind(segment_id)
            .bind(format!("chunk-{extraction_id}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        (
            pool.clone(),
            Arc::new(ResearchService::new(
                SqliteResearchRepository::new(pool),
                Arc::new(BroadcastEventBus::new(8)),
            )),
        )
    }

    async fn add_live_fixture(pool: &SqlitePool) -> (String, Vec<String>) {
        let case_id = new_id();
        sqlx::query(
            "INSERT INTO research_cases (id, title, created_at_ms, updated_at_ms) VALUES (?, 'Dify qualification', 1, 1)",
        )
        .bind(&case_id)
        .execute(pool)
        .await
        .unwrap();
        let mut extraction_ids = Vec::with_capacity(2);
        for suffix in ["ea", "eb"] {
            let source_id = new_id();
            let snapshot_id = new_id();
            let artifact_id = new_id();
            let extraction_id = new_id();
            let content_hash = format!("hash-{artifact_id}");
            let text = format!("Dify qualification evidence {suffix}");
            sqlx::query(
                "INSERT INTO research_sources (id, research_case_id, kind, label, created_at_ms) VALUES (?, ?, 'reference_pdf', ?, 1)",
            )
            .bind(&source_id)
            .bind(&case_id)
            .bind(format!("qualification-{suffix}"))
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_artifacts (id, hash_algorithm, content_hash, size_bytes, media_type, original_filename, created_at_ms) VALUES (?, 'sha256', ?, 1, 'application/pdf', ?, 1)",
            )
            .bind(&artifact_id)
            .bind(&content_hash)
            .bind(format!("qualification-{suffix}.pdf"))
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_source_snapshots (id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, origin_json, metadata_json) VALUES (?, ?, 'sha256', ?, 1, 'user_provided', ?, '{}')",
            )
            .bind(&snapshot_id)
            .bind(&source_id)
            .bind(&content_hash)
            .bind(
                serde_json::to_string(&json!({
                    "kind": "uploaded_artifact",
                    "artifact_id": artifact_id,
                    "revision_id": null
                }))
                .unwrap(),
            )
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_pdf_extractions (id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, hash_algorithm, extraction_hash, extracted_at_ms, status) VALUES (?, ?, ?, 'fixture', '1', 1, 'sha256', ?, 1, 'ready')",
            )
            .bind(&extraction_id)
            .bind(&snapshot_id)
            .bind(&artifact_id)
            .bind(format!("hash-{extraction_id}"))
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO research_pdf_pages (extraction_id, page, text, hash_algorithm, text_hash) VALUES (?, 1, ?, 'sha256', ?)",
            )
            .bind(&extraction_id)
            .bind(&text)
            .bind(sha256_hex(text.as_bytes()))
            .execute(pool)
            .await
            .unwrap();
            extraction_ids.push(extraction_id);
        }
        (case_id, extraction_ids)
    }

    fn adapter(
        pool: SqlitePool,
        research: Arc<ResearchService>,
        base_url: String,
    ) -> DifyResearchService {
        DifyResearchService::new(
            pool,
            research,
            Arc::new(BroadcastEventBus::new(8)),
            Some(DifyConfig::new(base_url, "test-key")),
        )
        .unwrap()
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
        let hits = client.retrieve("dataset", "query", 1, None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].segment_id, "segment-1");
        assert_eq!(hits[0].score, 0.91);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn scoped_retrieve_uses_exact_extraction_metadata_filter() {
        let (pool, research) = retrieval_fixture().await;
        let (base_url, server) = mock_response_server(vec![(
            200,
            json!({
                "records": [{"score": 0.91, "segment": {"id": "segment-ea"}}]
            }),
        )])
        .await;
        let service = adapter(pool, research, base_url);
        let candidates = service
            .retrieve_from_extractions("case-a", &["extraction-ea".to_owned()], "query", 5)
            .await
            .unwrap();
        let requests = server.await.unwrap();
        let body: Value =
            serde_json::from_str(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].extraction_id, "extraction-ea");
        assert_eq!(
            body["retrieval_model"]["metadata_filtering_conditions"],
            json!({
                "logical_operator": "and",
                "conditions": [{
                    "name": DIFY_EXTRACTION_METADATA_FIELD,
                    "comparison_operator": "is",
                    "value": "extraction-ea"
                }]
            })
        );
    }

    #[tokio::test]
    async fn source_scope_resolves_only_one_explicitly_qualified_extraction() {
        let (pool, research) = retrieval_fixture().await;
        let (base_url, server) = mock_response_server(vec![(
            200,
            json!({
                "records": [{"score": 0.91, "segment": {"id": "segment-ea"}}]
            }),
        )])
        .await;
        let service = adapter(pool, research, base_url);
        let candidates = service
            .retrieve_with_scope(
                "case-a",
                &ResearchRetrievalScope::Sources {
                    source_ids: vec![
                        nineprofs_research::ResearchSourceId::parse("source-a").unwrap(),
                    ],
                },
                "query",
                5,
            )
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(candidates[0].extraction_id, "extraction-ea");
    }

    #[tokio::test]
    async fn hostile_remote_scope_hit_fails_closed_without_evidence() {
        let (pool, research) = retrieval_fixture().await;
        let (base_url, server) = mock_response_server(vec![(
            200,
            json!({
                "records": [{"score": 0.99, "segment": {"id": "segment-eb"}}]
            }),
        )])
        .await;
        let service = adapter(pool.clone(), research, base_url);
        let result = service
            .retrieve_from_extractions("case-a", &["extraction-ea".to_owned()], "query", 5)
            .await;
        server.await.unwrap();
        assert!(matches!(result, Err(DifyError::IndexDrift)));
        let evidence_count = sqlx::query("SELECT COUNT(*) AS count FROM research_evidence")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert_eq!(evidence_count, 0);
        let case_status = sqlx::query(
            "SELECT status FROM research_dify_case_indexes WHERE research_case_id = 'case-a'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<String, _>("status");
        assert_eq!(case_status, "degraded");
    }

    #[tokio::test]
    async fn cross_case_extraction_scope_is_rejected_before_remote_query() {
        let (pool, research) = retrieval_fixture().await;
        let service = adapter(pool, research, "http://127.0.0.1:1/v1".to_owned());
        let result = service
            .retrieve_from_extractions("case-a", &["extraction-cross".to_owned()], "query", 5)
            .await;
        assert!(matches!(
            result,
            Err(DifyError::Invalid(message)) if message.contains("does not belong")
        ));
    }

    #[tokio::test]
    async fn multi_extraction_scope_accepts_only_mapped_extractions() {
        let (pool, research) = retrieval_fixture().await;
        let (base_url, server) = mock_response_server(vec![(
            200,
            json!({
                "records": [
                    {"score": 0.91, "segment": {"id": "segment-ea"}},
                    {"score": 0.90, "segment": {"id": "segment-ea2"}}
                ]
            }),
        )])
        .await;
        let service = adapter(pool, research, base_url);
        let candidates = service
            .retrieve_with_scope(
                "case-a",
                &ResearchRetrievalScope::Extractions {
                    extraction_ids: vec![
                        nineprofs_research::ResearchPdfExtractionId::parse("extraction-ea")
                            .unwrap(),
                        nineprofs_research::ResearchPdfExtractionId::parse("extraction-ea2")
                            .unwrap(),
                    ],
                },
                "query",
                5,
            )
            .await
            .unwrap();
        let requests = server.await.unwrap();
        let body: Value =
            serde_json::from_str(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body["retrieval_model"]["metadata_filtering_conditions"],
            json!({
                "logical_operator": "and",
                "conditions": [{
                    "name": DIFY_EXTRACTION_METADATA_FIELD,
                    "comparison_operator": "in",
                    "value": ["extraction-ea", "extraction-ea2"]
                }]
            })
        );
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.extraction_id.as_str())
                .collect::<Vec<_>>(),
            ["extraction-ea", "extraction-ea2"]
        );
    }

    #[tokio::test]
    async fn legacy_index_remains_case_wide_but_refuses_scoped_retrieval() {
        let (pool, research) = retrieval_fixture().await;
        sqlx::query(
            "UPDATE research_dify_extraction_indexes SET metadata_qualified = 0 WHERE extraction_id = 'extraction-ea'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let (base_url, server) = mock_response_server(vec![(
            200,
            json!({
                "records": [{"score": 0.91, "segment": {"id": "segment-ea"}}]
            }),
        )])
        .await;
        let service = adapter(pool.clone(), research.clone(), base_url);
        assert_eq!(
            service.retrieve("case-a", "query", 5).await.unwrap().len(),
            1
        );
        server.await.unwrap();
        let service = adapter(pool, research, "http://127.0.0.1:1/v1".to_owned());
        assert!(matches!(
            service
                .retrieve_from_extractions("case-a", &["extraction-ea".to_owned()], "query", 5,)
                .await,
            Err(DifyError::IndexingFailed)
        ));
    }

    #[tokio::test]
    async fn metadata_provisioning_is_idempotent_and_uses_v1_16_1_shapes() {
        let fields = [
            (DIFY_EXTRACTION_METADATA_FIELD, "field-extraction"),
            (DIFY_SOURCE_METADATA_FIELD, "field-source"),
            (DIFY_SNAPSHOT_METADATA_FIELD, "field-snapshot"),
        ];
        let all_fields = json!({
            "doc_metadata": fields.iter().map(|(name, id)| json!({
                "id": id, "name": name, "type": "string"
            })).collect::<Vec<_>>()
        });
        let responses = vec![
            (200, json!({"doc_metadata": []})),
            (
                200,
                json!({"id": fields[0].1, "name": fields[0].0, "type": "string"}),
            ),
            (
                200,
                json!({"id": fields[1].1, "name": fields[1].0, "type": "string"}),
            ),
            (
                200,
                json!({"id": fields[2].1, "name": fields[2].0, "type": "string"}),
            ),
            (200, all_fields),
            (200, json!({"result": "success"})),
            (
                200,
                json!({
                    "id": "document-ea",
                    "doc_metadata": fields.iter().map(|(name, id)| json!({
                        "id": id,
                        "name": name,
                        "type": "string",
                        "value": match *name {
                            DIFY_EXTRACTION_METADATA_FIELD => "extraction-ea",
                            DIFY_SOURCE_METADATA_FIELD => "source-a",
                            _ => "snapshot-ea",
                        }
                    })).collect::<Vec<_>>()
                }),
            ),
        ];
        let (base_url, server) = mock_response_server(responses).await;
        let client = DifyClient::new(DifyConfig::new(base_url, "test-key")).unwrap();
        let created = client.ensure_metadata_fields("dataset").await.unwrap();
        let reused = client.ensure_metadata_fields("dataset").await.unwrap();
        assert_eq!(
            created
                .iter()
                .map(|field| field.id.as_str())
                .collect::<Vec<_>>(),
            ["field-extraction", "field-source", "field-snapshot"]
        );
        assert_eq!(
            reused
                .iter()
                .map(|field| field.id.as_str())
                .collect::<Vec<_>>(),
            ["field-extraction", "field-source", "field-snapshot"]
        );
        client
            .update_document_metadata(
                "dataset",
                "document-ea",
                &created,
                "extraction-ea",
                "source-a",
                "snapshot-ea",
            )
            .await
            .unwrap();
        let document = client
            .document_metadata("dataset", "document-ea")
            .await
            .unwrap();
        DifyClient::verify_document_metadata(
            &document,
            "document-ea",
            &created,
            "extraction-ea",
            "source-a",
            "snapshot-ea",
        )
        .unwrap();
        let requests = server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("POST /v1/datasets/dataset/metadata"))
                .count(),
            3
        );
        let create_body: Value =
            serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            create_body,
            json!({"name": DIFY_EXTRACTION_METADATA_FIELD, "type": "string"})
        );
        let update_body: Value =
            serde_json::from_str(requests[5].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            update_body,
            json!({
                "operation_data": [{
                    "document_id": "document-ea",
                    "metadata_list": fields.iter().map(|(name, id)| json!({
                        "id": id,
                        "name": name,
                        "value": match *name {
                            DIFY_EXTRACTION_METADATA_FIELD => "extraction-ea",
                            DIFY_SOURCE_METADATA_FIELD => "source-a",
                            _ => "snapshot-ea",
                        }
                    })).collect::<Vec<_>>(),
                    "partial_update": true
                }]
            })
        );
    }

    #[tokio::test]
    async fn service_persists_metadata_field_ids_without_duplicate_provisioning() {
        let (pool, research) = retrieval_fixture().await;
        let fields = [
            (DIFY_EXTRACTION_METADATA_FIELD, "field-extraction"),
            (DIFY_SOURCE_METADATA_FIELD, "field-source"),
            (DIFY_SNAPSHOT_METADATA_FIELD, "field-snapshot"),
        ];
        let all_fields = json!({
            "doc_metadata": fields.iter().map(|(name, id)| json!({
                "id": id, "name": name, "type": "string"
            })).collect::<Vec<_>>()
        });
        let responses = vec![
            (200, json!({"id": "dataset-a"})),
            (200, json!({"doc_metadata": []})),
            (
                200,
                json!({"id": fields[0].1, "name": fields[0].0, "type": "string"}),
            ),
            (
                200,
                json!({"id": fields[1].1, "name": fields[1].0, "type": "string"}),
            ),
            (
                200,
                json!({"id": fields[2].1, "name": fields[2].0, "type": "string"}),
            ),
            (200, json!({"id": "dataset-a"})),
            (200, all_fields),
        ];
        let (base_url, server) = mock_response_server(responses).await;
        let service = adapter(pool.clone(), research, base_url);
        service.ensure_case_index("case-a").await.unwrap();
        service.ensure_case_index("case-a").await.unwrap();
        let fields_count = sqlx::query(
            "SELECT COUNT(*) AS count FROM research_dify_metadata_fields WHERE dataset_id = 'dataset-a'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>("count");
        assert_eq!(fields_count, 3);
        let requests = server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("POST /v1/datasets/dataset-a/metadata"))
                .count(),
            3
        );
    }

    async fn empty_service(config: Option<DifyConfig>) -> DifyResearchService {
        let database = Database::in_memory().await.unwrap();
        let research = Arc::new(ResearchService::new(
            SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(8)),
        ));
        DifyResearchService::new(
            database.pool().clone(),
            research,
            Arc::new(BroadcastEventBus::new(8)),
            config,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn readiness_distinguishes_configuration_connectivity_and_authentication() {
        let service = empty_service(None).await;
        assert!(matches!(
            service.readiness().status,
            DifyReadinessStatus::NotConfigured
        ));
        assert!(!service.qualified_readiness().await.ready);

        let mut config = DifyConfig::new("http://127.0.0.1:1/v1", "test-key");
        config.timeout = Duration::from_millis(100);
        let service = empty_service(Some(config)).await;
        let readiness = service.qualified_readiness().await;
        assert!(matches!(readiness.status, DifyReadinessStatus::Unreachable));
        assert!(!readiness.reachable);

        let (base_url, server) =
            mock_response_server(vec![(401, json!({"code": "unauthorized"}))]).await;
        let service = empty_service(Some(DifyConfig::new(base_url, "bad-key"))).await;
        let readiness = service.qualified_readiness().await;
        assert!(matches!(
            readiness.status,
            DifyReadinessStatus::Unauthorized
        ));
        assert!(readiness.reachable);
        assert!(!readiness.authorized);
        server.await.unwrap();

        let (base_url, server) = mock_response_server(vec![(200, json!({"data": []}))]).await;
        let service = empty_service(Some(DifyConfig::new(base_url, "good-key"))).await;
        let readiness = service.qualified_readiness().await;
        assert!(matches!(readiness.status, DifyReadinessStatus::Ready));
        assert!(readiness.ready && readiness.reachable && readiness.authorized);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn real_dify_scoped_qualification_is_explicitly_opt_in() {
        let (base_url, api_key) = match (
            std::env::var("NINEPROFS_DIFY_TEST_BASE_URL"),
            std::env::var("NINEPROFS_DIFY_TEST_API_KEY"),
        ) {
            (Ok(base_url), Ok(api_key)) if !base_url.trim().is_empty() && !api_key.is_empty() => {
                (base_url, api_key)
            }
            _ => {
                return;
            }
        };
        let (pool, research) = retrieval_fixture().await;
        let (case_id, extraction_ids) = add_live_fixture(&pool).await;
        let mut config = DifyConfig::new(base_url, Arc::<str>::from(api_key));
        config.max_poll_attempts = 120;
        let service = DifyResearchService::new(
            pool,
            research,
            Arc::new(BroadcastEventBus::new(8)),
            Some(config),
        )
        .unwrap();
        let client = service.client.as_ref().unwrap().clone();
        let result: Result<(), DifyError> = async {
            let index = service.ensure_case_index(&case_id).await?;
            for extraction_id in &extraction_ids {
                service
                    .sync_extraction(&index.index_id, extraction_id)
                    .await?;
            }
            let case_wide = service
                .retrieve(&case_id, "qualification evidence", 5)
                .await?;
            assert!(
                case_wide
                    .iter()
                    .all(|candidate| { extraction_ids.contains(&candidate.extraction_id) })
            );
            let scoped = service
                .retrieve_from_extractions(
                    &case_id,
                    &[extraction_ids[0].clone()],
                    "qualification evidence",
                    5,
                )
                .await?;
            assert!(
                scoped
                    .iter()
                    .all(|candidate| candidate.extraction_id == extraction_ids[0])
            );
            Ok(())
        }
        .await;
        let index = sqlx::query(
            "SELECT dataset_id FROM research_dify_case_indexes WHERE research_case_id = ?",
        )
        .bind(&case_id)
        .fetch_optional(&service.pool)
        .await
        .unwrap();
        if let Some(index) = index {
            client
                .delete_dataset(&index.get::<String, _>("dataset_id"))
                .await
                .expect("only the temporary qualification dataset is cleaned up");
        }
        result.expect("opt-in Dify scoped qualification should pass");
    }
}
