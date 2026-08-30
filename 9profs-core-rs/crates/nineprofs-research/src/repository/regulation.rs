use sqlx::Row;

use crate::{
    ContentHash, RegulationRequirement, RegulationRequirementId, RegulationReviewStatus,
    ResearchError, ResearchPdfExtractionId, ResearchSourceId, ResearchSourceSnapshotId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

const REQUIREMENT_COLUMNS: &str = "id, source_id, source_snapshot_id, pdf_extraction_id, text, source_excerpt, \
     source_excerpt_hash_algorithm, source_excerpt_hash, source_locator_json, \
     authority_locator_json, applicability_json, effective_from, effective_until, \
     extraction_method, extraction_contract_version, review_status, active, \
     created_at_ms, updated_at_ms";

impl SqliteResearchRepository {
    pub(super) async fn list_regulation_requirements(
        &self,
        source_id: Option<&ResearchSourceId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<RegulationRequirement>, ResearchError> {
        let mut query_text =
            format!("SELECT {REQUIREMENT_COLUMNS} FROM research_regulation_requirements");
        let mut filters = Vec::new();
        if source_id.is_some() {
            filters.push("source_id = ?");
        }
        if source_snapshot_id.is_some() {
            filters.push("source_snapshot_id = ?");
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
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_requirement).collect()
    }

    pub(super) async fn get_regulation_requirement(
        &self,
        id: &RegulationRequirementId,
    ) -> Result<Option<RegulationRequirement>, ResearchError> {
        let row = sqlx::query(&format!(
            "SELECT {REQUIREMENT_COLUMNS} FROM research_regulation_requirements WHERE id = ?"
        ))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_requirement).transpose()
    }

    pub(super) async fn insert_regulation_requirement(
        &self,
        value: &RegulationRequirement,
    ) -> Result<(), ResearchError> {
        value.validate()?;
        sqlx::query(
            "INSERT INTO research_regulation_requirements \
             (id, source_id, source_snapshot_id, pdf_extraction_id, text, source_excerpt, \
              source_excerpt_hash_algorithm, source_excerpt_hash, source_locator_json, \
              authority_locator_json, applicability_json, effective_from, effective_until, \
              extraction_method, extraction_contract_version, review_status, active, \
              created_at_ms, updated_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.source_id.as_str())
        .bind(value.source_snapshot_id.as_str())
        .bind(value.pdf_extraction_id.as_ref().map(ToString::to_string))
        .bind(&value.text)
        .bind(&value.source_excerpt)
        .bind(enum_text(&value.source_excerpt_hash.algorithm))
        .bind(&value.source_excerpt_hash.value)
        .bind(json_text(&value.source_locator)?)
        .bind(
            value
                .authority_locator
                .as_ref()
                .map(json_text)
                .transpose()?,
        )
        .bind(json_text(&value.applicability)?)
        .bind(value.effective_from)
        .bind(value.effective_until)
        .bind(&value.extraction_method)
        .bind(&value.extraction_contract_version)
        .bind(enum_text(&value.review_status))
        .bind(value.active as i64)
        .bind(value.created_at_ms)
        .bind(value.updated_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn update_regulation_requirement_review_status(
        &self,
        id: &RegulationRequirementId,
        status: &RegulationReviewStatus,
        updated_at_ms: i64,
    ) -> Result<bool, ResearchError> {
        let status_text = enum_text(status);
        let result = sqlx::query(
            "UPDATE research_regulation_requirements \
             SET review_status = ?, updated_at_ms = ? \
             WHERE id = ? AND (active = 0 OR ? = 'approved')",
        )
        .bind(&status_text)
        .bind(updated_at_ms)
        .bind(id.as_str())
        .bind(&status_text)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub(super) async fn set_regulation_requirement_active(
        &self,
        id: &RegulationRequirementId,
        active: bool,
        updated_at_ms: i64,
    ) -> Result<bool, ResearchError> {
        let result = sqlx::query(
            "UPDATE research_regulation_requirements \
             SET active = ?, updated_at_ms = ? \
             WHERE id = ? AND (? = 0 OR review_status = 'approved')",
        )
        .bind(active as i64)
        .bind(updated_at_ms)
        .bind(id.as_str())
        .bind(active as i64)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

fn map_requirement(row: sqlx::sqlite::SqliteRow) -> Result<RegulationRequirement, ResearchError> {
    let active = match row.get::<i64, _>("active") {
        0 => false,
        1 => true,
        value => {
            return Err(ResearchError::Invalid(format!(
                "invalid persisted regulation requirement active value: {value}"
            )));
        }
    };
    let value = RegulationRequirement {
        id: RegulationRequirementId::parse(row.get::<String, _>("id"))?,
        source_id: ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        source_snapshot_id: ResearchSourceSnapshotId::parse(
            row.get::<String, _>("source_snapshot_id"),
        )?,
        pdf_extraction_id: row
            .try_get::<Option<String>, _>("pdf_extraction_id")?
            .map(ResearchPdfExtractionId::parse)
            .transpose()?,
        text: row.get("text"),
        source_excerpt: row.get("source_excerpt"),
        source_excerpt_hash: ContentHash {
            algorithm: json_column(
                format!(
                    "\"{}\"",
                    row.get::<String, _>("source_excerpt_hash_algorithm")
                ),
                "regulation source excerpt hash algorithm",
            )?,
            value: row.get("source_excerpt_hash"),
        },
        source_locator: json_column(row.get("source_locator_json"), "regulation source locator")?,
        authority_locator: row
            .try_get::<Option<String>, _>("authority_locator_json")?
            .map(|value| json_column(value, "regulation authority locator"))
            .transpose()?,
        applicability: json_column(row.get("applicability_json"), "regulation applicability")?,
        effective_from: row.get("effective_from"),
        effective_until: row.get("effective_until"),
        extraction_method: row.get("extraction_method"),
        extraction_contract_version: row.try_get("extraction_contract_version")?,
        review_status: json_column(
            format!("\"{}\"", row.get::<String, _>("review_status")),
            "regulation review status",
        )?,
        active,
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    };
    value.validate()?;
    Ok(value)
}
