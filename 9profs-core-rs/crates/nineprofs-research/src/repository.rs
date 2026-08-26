use async_trait::async_trait;
use nineprofs_common::now_ms;
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Row, SqlitePool};

use crate::{
    ClaimEvidenceLink, ClaimEvidenceLinkId, ContentHash, ResearchCase, ResearchCaseId,
    ResearchClaim, ResearchClaimId, ResearchError, ResearchEvidence, ResearchEvidenceId,
    ResearchSource, ResearchSourceId, ResearchSourceSnapshot, ResearchSourceSnapshotId,
};

#[async_trait]
pub trait ResearchRepository: Send + Sync {
    async fn list_cases(&self) -> Result<Vec<ResearchCase>, ResearchError>;
    async fn get_case(&self, id: &ResearchCaseId) -> Result<Option<ResearchCase>, ResearchError>;
    async fn insert_case(&self, value: &ResearchCase) -> Result<(), ResearchError>;

    async fn list_sources(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchSource>, ResearchError>;
    async fn get_source(
        &self,
        id: &ResearchSourceId,
    ) -> Result<Option<ResearchSource>, ResearchError>;
    async fn insert_source(&self, value: &ResearchSource) -> Result<(), ResearchError>;

    async fn list_snapshots(
        &self,
        source_id: Option<&ResearchSourceId>,
    ) -> Result<Vec<ResearchSourceSnapshot>, ResearchError>;
    async fn get_snapshot(
        &self,
        id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError>;
    async fn find_snapshot_by_hash(
        &self,
        source_id: &ResearchSourceId,
        content_hash: &ContentHash,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError>;
    async fn insert_snapshot(&self, value: &ResearchSourceSnapshot) -> Result<bool, ResearchError>;

    async fn list_evidence(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError>;
    async fn get_evidence(
        &self,
        id: &ResearchEvidenceId,
    ) -> Result<Option<ResearchEvidence>, ResearchError>;
    async fn insert_evidence(&self, value: &ResearchEvidence) -> Result<(), ResearchError>;

    async fn list_claims(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchClaim>, ResearchError>;
    async fn get_claim(&self, id: &ResearchClaimId)
    -> Result<Option<ResearchClaim>, ResearchError>;
    async fn insert_claim(&self, value: &ResearchClaim) -> Result<(), ResearchError>;

    async fn list_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        evidence_id: Option<&ResearchEvidenceId>,
    ) -> Result<Vec<ClaimEvidenceLink>, ResearchError>;
    async fn get_link(
        &self,
        id: &ClaimEvidenceLinkId,
    ) -> Result<Option<ClaimEvidenceLink>, ResearchError>;
    async fn insert_link(&self, value: &ClaimEvidenceLink) -> Result<(), ResearchError>;
}

#[derive(Clone, Debug)]
pub struct SqliteResearchRepository {
    pool: SqlitePool,
}

impl SqliteResearchRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResearchRepository for SqliteResearchRepository {
    async fn list_cases(&self) -> Result<Vec<ResearchCase>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, title, created_at_ms, updated_at_ms FROM research_cases ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_case).collect()
    }

    async fn get_case(&self, id: &ResearchCaseId) -> Result<Option<ResearchCase>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, title, created_at_ms, updated_at_ms FROM research_cases WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_case).transpose()
    }

    async fn insert_case(&self, value: &ResearchCase) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_cases (id, title, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(&value.title)
        .bind(value.created_at_ms)
        .bind(value.updated_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_sources(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchSource>, ResearchError> {
        let rows = match research_case_id {
            Some(research_case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, kind, label, created_at_ms \
                     FROM research_sources WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(research_case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, kind, label, created_at_ms \
                     FROM research_sources ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_source).collect()
    }

    async fn get_source(
        &self,
        id: &ResearchSourceId,
    ) -> Result<Option<ResearchSource>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, kind, label, created_at_ms \
             FROM research_sources WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_source).transpose()
    }

    async fn insert_source(&self, value: &ResearchSource) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_sources (id, research_case_id, kind, label, created_at_ms) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(enum_text(&value.kind))
        .bind(&value.label)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_snapshots(
        &self,
        source_id: Option<&ResearchSourceId>,
    ) -> Result<Vec<ResearchSourceSnapshot>, ResearchError> {
        let query = match source_id {
            Some(_) => snapshot_select("WHERE source_id = ?"),
            None => snapshot_select(""),
        };
        let mut query = sqlx::query(&query);
        if let Some(source_id) = source_id {
            query = query.bind(source_id.as_str());
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_snapshot).collect()
    }

    async fn get_snapshot(
        &self,
        id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError> {
        let query = snapshot_select("WHERE id = ?");
        let row = sqlx::query(&query)
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_snapshot).transpose()
    }

    async fn find_snapshot_by_hash(
        &self,
        source_id: &ResearchSourceId,
        content_hash: &ContentHash,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError> {
        let query =
            snapshot_select("WHERE source_id = ? AND hash_algorithm = ? AND content_hash = ?");
        let row = sqlx::query(&query)
            .bind(source_id.as_str())
            .bind(enum_text(&content_hash.algorithm))
            .bind(&content_hash.value)
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_snapshot).transpose()
    }

    async fn insert_snapshot(&self, value: &ResearchSourceSnapshot) -> Result<bool, ResearchError> {
        let result = sqlx::query(
            "INSERT INTO research_source_snapshots \
             (id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, origin_json, metadata_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(source_id, hash_algorithm, content_hash) DO NOTHING",
        )
        .bind(value.id.as_str())
        .bind(value.source_id.as_str())
        .bind(enum_text(&value.content_hash.algorithm))
        .bind(&value.content_hash.value)
        .bind(value.captured_at_ms)
        .bind(enum_text(&value.capture_method))
        .bind(json_text(&value.origin)?)
        .bind(json_text(&value.metadata)?)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn list_evidence(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError> {
        let (query, binds) = match (research_case_id, source_snapshot_id) {
            (Some(case_id), Some(snapshot_id)) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                 locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method \
                 FROM research_evidence WHERE research_case_id = ? AND source_snapshot_id = ? ORDER BY id ASC",
                Some((case_id.as_str().to_owned(), snapshot_id.as_str().to_owned())),
            ),
            (Some(case_id), None) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                 locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method \
                 FROM research_evidence WHERE research_case_id = ? ORDER BY id ASC",
                Some((case_id.as_str().to_owned(), String::new())),
            ),
            (None, Some(snapshot_id)) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                 locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method \
                 FROM research_evidence WHERE source_snapshot_id = ? ORDER BY id ASC",
                Some((snapshot_id.as_str().to_owned(), String::new())),
            ),
            (None, None) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                 locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method \
                 FROM research_evidence ORDER BY id ASC",
                None,
            ),
        };
        let mut query = sqlx::query(query);
        if let Some((first, second)) = binds {
            query = query.bind(first);
            if research_case_id.is_some() && source_snapshot_id.is_some() {
                query = query.bind(second);
            }
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_evidence).collect()
    }

    async fn get_evidence(
        &self,
        id: &ResearchEvidenceId,
    ) -> Result<Option<ResearchEvidence>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
             locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method \
             FROM research_evidence WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_evidence).transpose()
    }

    async fn insert_evidence(&self, value: &ResearchEvidence) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_evidence \
             (id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, locator_json, \
              hash_algorithm, excerpt_hash, captured_at_ms, capture_method) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.source_snapshot_id.as_str())
        .bind(&value.verbatim_excerpt)
        .bind(&value.normalized_text)
        .bind(json_text(&value.locator)?)
        .bind(enum_text(&value.excerpt_hash.algorithm))
        .bind(&value.excerpt_hash.value)
        .bind(value.captured_at_ms)
        .bind(enum_text(&value.capture_method))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_claims(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchClaim>, ResearchError> {
        let rows = match research_case_id {
            Some(case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, text, origin_json, created_at_ms \
                 FROM research_claims WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, text, origin_json, created_at_ms \
                 FROM research_claims ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_claim).collect()
    }

    async fn get_claim(
        &self,
        id: &ResearchClaimId,
    ) -> Result<Option<ResearchClaim>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, text, origin_json, created_at_ms \
             FROM research_claims WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_claim).transpose()
    }

    async fn insert_claim(&self, value: &ResearchClaim) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_claims (id, research_case_id, text, origin_json, created_at_ms) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(&value.text)
        .bind(json_text(&value.origin)?)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        evidence_id: Option<&ResearchEvidenceId>,
    ) -> Result<Vec<ClaimEvidenceLink>, ResearchError> {
        let mut conditions = Vec::new();
        if research_case_id.is_some() {
            conditions.push("research_case_id = ?");
        }
        if claim_id.is_some() {
            conditions.push("claim_id = ?");
        }
        if evidence_id.is_some() {
            conditions.push("evidence_id = ?");
        }
        let suffix = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        let query = format!(
            "SELECT id, research_case_id, claim_id, evidence_id, relation, rationale, \
             assessment_method, assessment_metadata_json, created_at_ms \
             FROM research_claim_evidence{suffix} ORDER BY id ASC"
        );
        let mut query = sqlx::query(&query);
        for value in [
            research_case_id.map(|value| value.as_str()),
            claim_id.map(|value| value.as_str()),
            evidence_id.map(|value| value.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            query = query.bind(value);
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_link).collect()
    }

    async fn get_link(
        &self,
        id: &ClaimEvidenceLinkId,
    ) -> Result<Option<ClaimEvidenceLink>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_id, evidence_id, relation, rationale, \
             assessment_method, assessment_metadata_json, created_at_ms \
             FROM research_claim_evidence WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_link).transpose()
    }

    async fn insert_link(&self, value: &ClaimEvidenceLink) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_claim_evidence \
             (id, research_case_id, claim_id, evidence_id, relation, rationale, assessment_method, \
              assessment_metadata_json, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.claim_id.as_str())
        .bind(value.evidence_id.as_str())
        .bind(enum_text(&value.relation))
        .bind(&value.rationale)
        .bind(enum_text(&value.assessment_method))
        .bind(json_text(&value.assessment_metadata)?)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn snapshot_select(where_clause: &str) -> String {
    format!(
        "SELECT id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, \
         origin_json, metadata_json FROM research_source_snapshots {where_clause} ORDER BY id ASC"
    )
}

fn enum_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("research enum serialization cannot fail")
        .trim_matches('"')
        .to_owned()
}

fn json_text<T: Serialize>(value: &T) -> Result<String, ResearchError> {
    Ok(serde_json::to_string(value)?)
}

fn json_column<T: DeserializeOwned>(value: String, field: &str) -> Result<T, ResearchError> {
    serde_json::from_str(&value)
        .map_err(|error| ResearchError::Invalid(format!("invalid persisted {field}: {error}")))
}

fn map_case(row: sqlx::sqlite::SqliteRow) -> Result<ResearchCase, ResearchError> {
    Ok(ResearchCase {
        id: ResearchCaseId::parse(row.get::<String, _>("id"))?,
        title: row.get("title"),
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}

fn map_source(row: sqlx::sqlite::SqliteRow) -> Result<ResearchSource, ResearchError> {
    Ok(ResearchSource {
        id: ResearchSourceId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        kind: json_column(
            format!("\"{}\"", row.get::<String, _>("kind")),
            "source kind",
        )?,
        label: row.get("label"),
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_snapshot(row: sqlx::sqlite::SqliteRow) -> Result<ResearchSourceSnapshot, ResearchError> {
    Ok(ResearchSourceSnapshot {
        id: ResearchSourceSnapshotId::parse(row.get::<String, _>("id"))?,
        source_id: ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        content_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "snapshot hash algorithm",
            )?,
            value: row.get("content_hash"),
        },
        captured_at_ms: row.get("captured_at_ms"),
        capture_method: json_column(
            format!("\"{}\"", row.get::<String, _>("capture_method")),
            "snapshot capture method",
        )?,
        origin: json_column(row.get("origin_json"), "snapshot origin")?,
        metadata: json_column(row.get("metadata_json"), "snapshot metadata")?,
    })
}

fn map_evidence(row: sqlx::sqlite::SqliteRow) -> Result<ResearchEvidence, ResearchError> {
    Ok(ResearchEvidence {
        id: ResearchEvidenceId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        source_snapshot_id: ResearchSourceSnapshotId::parse(
            row.get::<String, _>("source_snapshot_id"),
        )?,
        verbatim_excerpt: row.get("verbatim_excerpt"),
        normalized_text: row.get("normalized_text"),
        locator: json_column(row.get("locator_json"), "evidence locator")?,
        excerpt_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "evidence hash algorithm",
            )?,
            value: row.get("excerpt_hash"),
        },
        captured_at_ms: row.get("captured_at_ms"),
        capture_method: json_column(
            format!("\"{}\"", row.get::<String, _>("capture_method")),
            "evidence capture method",
        )?,
    })
}

fn map_claim(row: sqlx::sqlite::SqliteRow) -> Result<ResearchClaim, ResearchError> {
    Ok(ResearchClaim {
        id: ResearchClaimId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        text: row.get("text"),
        origin: json_column(row.get("origin_json"), "claim origin")?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_link(row: sqlx::sqlite::SqliteRow) -> Result<ClaimEvidenceLink, ResearchError> {
    Ok(ClaimEvidenceLink {
        id: ClaimEvidenceLinkId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        claim_id: ResearchClaimId::parse(row.get::<String, _>("claim_id"))?,
        evidence_id: ResearchEvidenceId::parse(row.get::<String, _>("evidence_id"))?,
        relation: json_column(
            format!("\"{}\"", row.get::<String, _>("relation")),
            "claim-evidence relation",
        )?,
        rationale: row.get("rationale"),
        assessment_method: json_column(
            format!("\"{}\"", row.get::<String, _>("assessment_method")),
            "assessment method",
        )?,
        assessment_metadata: json_column(
            row.get("assessment_metadata_json"),
            "assessment metadata",
        )?,
        created_at_ms: row.get("created_at_ms"),
    })
}

#[allow(dead_code)]
fn _now_for_repository_tests() -> i64 {
    now_ms()
}
