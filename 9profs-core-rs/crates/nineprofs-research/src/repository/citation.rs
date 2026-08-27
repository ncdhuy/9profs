use sqlx::Row;

use crate::{
    CitationOccurrence, CitationOccurrenceId, CitationTarget, CitationTargetBinding,
    CitationTargetBindingId, CitationTargetId, ClaimCitationLink, ClaimCitationLinkId,
    ResearchCaseId, ResearchClaimId, ResearchError, ResearchPdfExtractionId, ResearchSourceId,
    ResearchSourceSnapshotId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

impl SqliteResearchRepository {
    pub(super) async fn list_citation_occurrences(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<CitationOccurrence>, ResearchError> {
        let rows = match research_case_id {
            Some(case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, origin_json, rendered_text, created_at_ms \
                     FROM research_citation_occurrences WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, origin_json, rendered_text, created_at_ms \
                     FROM research_citation_occurrences ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_citation_occurrence).collect()
    }

    pub(super) async fn get_citation_occurrence(
        &self,
        id: &CitationOccurrenceId,
    ) -> Result<Option<CitationOccurrence>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, origin_json, rendered_text, created_at_ms \
             FROM research_citation_occurrences WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_occurrence).transpose()
    }

    pub(super) async fn insert_citation_occurrence(
        &self,
        value: &CitationOccurrence,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_citation_occurrences \
             (id, research_case_id, origin_json, rendered_text, created_at_ms) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(json_text(&value.origin)?)
        .bind(&value.rendered_text)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn list_citation_targets(
        &self,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Vec<CitationTarget>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, citation_occurrence_id, ordinal, reference_key, cited_locator \
             FROM research_citation_targets WHERE citation_occurrence_id = ? ORDER BY ordinal ASC",
        )
        .bind(citation_occurrence_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_citation_target).collect()
    }

    pub(super) async fn get_citation_target(
        &self,
        id: &CitationTargetId,
    ) -> Result<Option<CitationTarget>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, citation_occurrence_id, ordinal, reference_key, cited_locator \
             FROM research_citation_targets WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_target).transpose()
    }

    pub(super) async fn insert_citation_target(
        &self,
        value: &CitationTarget,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_citation_targets \
             (id, citation_occurrence_id, ordinal, reference_key, cited_locator) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.citation_occurrence_id.as_str())
        .bind(value.ordinal)
        .bind(&value.reference_key)
        .bind(&value.cited_locator)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn list_citation_target_bindings(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Vec<CitationTargetBinding>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, research_case_id, citation_target_id, source_id, source_snapshot_id, \
             extraction_id, method, created_at_ms FROM research_citation_target_bindings \
             WHERE citation_target_id = ? ORDER BY created_at_ms ASC, id ASC",
        )
        .bind(citation_target_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_citation_target_binding).collect()
    }

    pub(super) async fn get_citation_target_binding(
        &self,
        id: &CitationTargetBindingId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, citation_target_id, source_id, source_snapshot_id, \
             extraction_id, method, created_at_ms FROM research_citation_target_bindings WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_target_binding).transpose()
    }

    pub(super) async fn latest_citation_target_binding(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, citation_target_id, source_id, source_snapshot_id, \
             extraction_id, method, created_at_ms FROM research_citation_target_bindings \
             WHERE citation_target_id = ? ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(citation_target_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_target_binding).transpose()
    }

    pub(super) async fn insert_citation_target_binding(
        &self,
        value: &CitationTargetBinding,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_citation_target_bindings \
             (id, research_case_id, citation_target_id, source_id, source_snapshot_id, extraction_id, method, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.citation_target_id.as_str())
        .bind(value.source_id.as_str())
        .bind(value.source_snapshot_id.as_ref().map(|id| id.as_str()))
        .bind(value.extraction_id.as_ref().map(|id| id.as_str()))
        .bind(enum_text(&value.method))
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn list_claim_citation_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        citation_occurrence_id: Option<&CitationOccurrenceId>,
    ) -> Result<Vec<ClaimCitationLink>, ResearchError> {
        let mut conditions = Vec::new();
        if research_case_id.is_some() {
            conditions.push("research_case_id = ?");
        }
        if claim_id.is_some() {
            conditions.push("claim_id = ?");
        }
        if citation_occurrence_id.is_some() {
            conditions.push("citation_occurrence_id = ?");
        }
        let suffix = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        let query = format!(
            "SELECT id, research_case_id, claim_id, citation_occurrence_id, created_at_ms \
             FROM research_claim_citations{suffix} ORDER BY created_at_ms ASC, id ASC"
        );
        let mut query = sqlx::query(&query);
        for value in [
            research_case_id.map(|value| value.as_str()),
            claim_id.map(|value| value.as_str()),
            citation_occurrence_id.map(|value| value.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            query = query.bind(value);
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_claim_citation_link).collect()
    }

    pub(super) async fn get_claim_citation_link(
        &self,
        id: &ClaimCitationLinkId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_id, citation_occurrence_id, created_at_ms \
             FROM research_claim_citations WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_claim_citation_link).transpose()
    }

    pub(super) async fn find_claim_citation_link(
        &self,
        claim_id: &ResearchClaimId,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_id, citation_occurrence_id, created_at_ms \
             FROM research_claim_citations WHERE claim_id = ? AND citation_occurrence_id = ?",
        )
        .bind(claim_id.as_str())
        .bind(citation_occurrence_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_claim_citation_link).transpose()
    }

    pub(super) async fn insert_claim_citation_link(
        &self,
        value: &ClaimCitationLink,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_claim_citations \
             (id, research_case_id, claim_id, citation_occurrence_id, created_at_ms) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.claim_id.as_str())
        .bind(value.citation_occurrence_id.as_str())
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn map_citation_occurrence(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CitationOccurrence, ResearchError> {
    Ok(CitationOccurrence {
        id: CitationOccurrenceId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        origin: json_column(row.get("origin_json"), "citation occurrence origin")?,
        rendered_text: row.get("rendered_text"),
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_citation_target(row: sqlx::sqlite::SqliteRow) -> Result<CitationTarget, ResearchError> {
    Ok(CitationTarget {
        id: CitationTargetId::parse(row.get::<String, _>("id"))?,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        reference_key: row.get("reference_key"),
        cited_locator: row.get("cited_locator"),
    })
}

fn map_citation_target_binding(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CitationTargetBinding, ResearchError> {
    Ok(CitationTargetBinding {
        id: CitationTargetBindingId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        citation_target_id: CitationTargetId::parse(row.get::<String, _>("citation_target_id"))?,
        source_id: ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        source_snapshot_id: row
            .get::<Option<String>, _>("source_snapshot_id")
            .map(ResearchSourceSnapshotId::parse)
            .transpose()?,
        extraction_id: row
            .get::<Option<String>, _>("extraction_id")
            .map(ResearchPdfExtractionId::parse)
            .transpose()?,
        method: json_column(
            format!("\"{}\"", row.get::<String, _>("method")),
            "citation binding method",
        )?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_claim_citation_link(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ClaimCitationLink, ResearchError> {
    Ok(ClaimCitationLink {
        id: ClaimCitationLinkId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        claim_id: ResearchClaimId::parse(row.get::<String, _>("claim_id"))?,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        created_at_ms: row.get("created_at_ms"),
    })
}
