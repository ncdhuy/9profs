use sqlx::Row;

use crate::{
    RegulationRequirementCandidate, RegulationRequirementCandidateId, ResearchError,
    ResearchPdfExtractionId, ResearchSourceId, ResearchSourceSnapshotId,
};

use super::{
    SqliteResearchRepository,
    common::{json_column, json_text},
};

const CANDIDATE_COLUMNS: &str = "id, source_id, source_snapshot_id, pdf_extraction_id, \
     source_locator_json, authority_locator_suggestion_json, ocr_excerpt, \
     normalized_requirement, applicability_suggestion_json, extraction_json, \
     risk_flags_json, review_notes, created_at_ms";

impl SqliteResearchRepository {
    pub(super) async fn list_regulation_requirement_candidates(
        &self,
        source_id: Option<&ResearchSourceId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
        pdf_extraction_id: Option<&ResearchPdfExtractionId>,
    ) -> Result<Vec<RegulationRequirementCandidate>, ResearchError> {
        let mut query_text =
            format!("SELECT {CANDIDATE_COLUMNS} FROM research_regulation_requirement_candidates");
        let mut filters = Vec::new();
        if source_id.is_some() {
            filters.push("source_id = ?");
        }
        if source_snapshot_id.is_some() {
            filters.push("source_snapshot_id = ?");
        }
        if pdf_extraction_id.is_some() {
            filters.push("pdf_extraction_id = ?");
        }
        if !filters.is_empty() {
            query_text.push_str(" WHERE ");
            query_text.push_str(&filters.join(" AND "));
        }
        query_text.push_str(" ORDER BY id ASC");

        let mut query = sqlx::query(&query_text);
        if let Some(source_id) = source_id {
            query = query.bind(source_id.as_str());
        }
        if let Some(source_snapshot_id) = source_snapshot_id {
            query = query.bind(source_snapshot_id.as_str());
        }
        if let Some(pdf_extraction_id) = pdf_extraction_id {
            query = query.bind(pdf_extraction_id.as_str());
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_candidate).collect()
    }

    pub(super) async fn get_regulation_requirement_candidate(
        &self,
        id: &RegulationRequirementCandidateId,
    ) -> Result<Option<RegulationRequirementCandidate>, ResearchError> {
        let row = sqlx::query(&format!(
            "SELECT {CANDIDATE_COLUMNS} FROM research_regulation_requirement_candidates WHERE id = ?"
        ))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_candidate).transpose()
    }

    pub(super) async fn insert_regulation_requirement_candidate(
        &self,
        value: &RegulationRequirementCandidate,
    ) -> Result<(), ResearchError> {
        value.validate()?;
        sqlx::query(
            "INSERT INTO research_regulation_requirement_candidates \
             (id, source_id, source_snapshot_id, pdf_extraction_id, source_locator_json, \
              authority_locator_suggestion_json, ocr_excerpt, normalized_requirement, \
              applicability_suggestion_json, extraction_json, risk_flags_json, review_notes, \
              created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.source_id.as_str())
        .bind(value.source_snapshot_id.as_str())
        .bind(value.pdf_extraction_id.as_str())
        .bind(json_text(&value.source_locator)?)
        .bind(
            value
                .authority_locator_suggestion
                .as_ref()
                .map(json_text)
                .transpose()?,
        )
        .bind(&value.ocr_excerpt)
        .bind(&value.normalized_requirement)
        .bind(json_text(&value.applicability_suggestion)?)
        .bind(json_text(&value.extraction)?)
        .bind(json_text(&value.risk_flags)?)
        .bind(&value.review_notes)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn map_candidate(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RegulationRequirementCandidate, ResearchError> {
    let value = RegulationRequirementCandidate {
        id: RegulationRequirementCandidateId::parse(row.get::<String, _>("id"))?,
        source_id: ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        source_snapshot_id: ResearchSourceSnapshotId::parse(
            row.get::<String, _>("source_snapshot_id"),
        )?,
        pdf_extraction_id: ResearchPdfExtractionId::parse(
            row.get::<String, _>("pdf_extraction_id"),
        )?,
        source_locator: json_column(
            row.get("source_locator_json"),
            "regulation requirement candidate source locator",
        )?,
        authority_locator_suggestion: row
            .try_get::<Option<String>, _>("authority_locator_suggestion_json")?
            .map(|value| {
                json_column(
                    value,
                    "regulation requirement candidate authority locator suggestion",
                )
            })
            .transpose()?,
        ocr_excerpt: row.get("ocr_excerpt"),
        normalized_requirement: row.get("normalized_requirement"),
        applicability_suggestion: json_column(
            row.get("applicability_suggestion_json"),
            "regulation requirement candidate applicability suggestion",
        )?,
        extraction: json_column(
            row.get("extraction_json"),
            "regulation requirement candidate extraction",
        )?,
        risk_flags: json_column(
            row.get("risk_flags_json"),
            "regulation requirement candidate risk flags",
        )?,
        review_notes: row.try_get("review_notes")?,
        created_at_ms: row.get("created_at_ms"),
    };
    value.validate()?;
    Ok(value)
}
