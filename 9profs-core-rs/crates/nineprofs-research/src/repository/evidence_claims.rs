use sqlx::Row;

use crate::{
    ClaimEvidenceLink, ClaimEvidenceLinkId, ContentHash, ResearchCaseId, ResearchClaim,
    ResearchClaimId, ResearchError, ResearchEvidence, ResearchEvidenceId, ResearchPdfExtractionId,
    ResearchSourceSnapshotId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

impl SqliteResearchRepository {
    pub(super) async fn list_evidence(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError> {
        let (query, binds) = match (research_case_id, source_snapshot_id) {
            (Some(case_id), Some(snapshot_id)) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence WHERE research_case_id = ? AND source_snapshot_id = ? ORDER BY id ASC",
                Some((case_id.as_str().to_owned(), snapshot_id.as_str().to_owned())),
            ),
            (Some(case_id), None) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence WHERE research_case_id = ? ORDER BY id ASC",
                Some((case_id.as_str().to_owned(), String::new())),
            ),
            (None, Some(snapshot_id)) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence WHERE source_snapshot_id = ? ORDER BY id ASC",
                Some((snapshot_id.as_str().to_owned(), String::new())),
            ),
            (None, None) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
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

    pub(super) async fn get_evidence(
        &self,
        id: &ResearchEvidenceId,
    ) -> Result<Option<ResearchEvidence>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
             locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
             FROM research_evidence WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_evidence).transpose()
    }

    pub(super) async fn insert_evidence(
        &self,
        value: &ResearchEvidence,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_evidence \
             (id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, locator_json, \
              hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(value.pdf_extraction_id.as_ref().map(|id| id.as_str()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn list_claims(
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

    pub(super) async fn get_claim(
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

    pub(super) async fn insert_claim(&self, value: &ResearchClaim) -> Result<(), ResearchError> {
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

    pub(super) async fn list_links(
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

    pub(super) async fn get_link(
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

    pub(super) async fn insert_link(&self, value: &ClaimEvidenceLink) -> Result<(), ResearchError> {
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
        pdf_extraction_id: row
            .get::<Option<String>, _>("pdf_extraction_id")
            .map(ResearchPdfExtractionId::parse)
            .transpose()?,
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
