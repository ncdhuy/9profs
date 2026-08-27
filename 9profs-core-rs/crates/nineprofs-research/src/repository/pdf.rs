use sqlx::Row;

use crate::{
    ContentHash, ResearchError, ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage,
    ResearchSourceSnapshotId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column},
};

impl SqliteResearchRepository {
    pub(super) async fn get_pdf_extraction(
        &self,
        id: &ResearchPdfExtractionId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        let row = sqlx::query(&pdf_extraction_select(
            "WHERE id = ?",
            "extracted_at_ms ASC, id ASC",
        ))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_extraction).transpose()
    }

    pub(super) async fn latest_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        let row = sqlx::query(&pdf_extraction_select(
            "WHERE source_snapshot_id = ?",
            "extracted_at_ms DESC, id DESC",
        ))
        .bind(source_snapshot_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_extraction).transpose()
    }

    pub(super) async fn list_pdf_extractions(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Vec<ResearchPdfExtraction>, ResearchError> {
        let rows = sqlx::query(&pdf_extraction_select(
            "WHERE source_snapshot_id = ?",
            "extracted_at_ms ASC, id ASC",
        ))
        .bind(source_snapshot_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_pdf_extraction).collect()
    }

    pub(super) async fn find_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
        extractor: &str,
        extractor_version: &str,
        extraction_hash: &ContentHash,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        let row = sqlx::query(&pdf_extraction_select(
            "WHERE source_snapshot_id = ? AND extractor = ? AND extractor_version = ? \
             AND hash_algorithm = ? AND extraction_hash = ?",
            "extracted_at_ms ASC, id ASC",
        ))
        .bind(source_snapshot_id.as_str())
        .bind(extractor)
        .bind(extractor_version)
        .bind(enum_text(&extraction_hash.algorithm))
        .bind(&extraction_hash.value)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_extraction).transpose()
    }

    pub(super) async fn insert_pdf_extraction(
        &self,
        value: &ResearchPdfExtraction,
    ) -> Result<bool, ResearchError> {
        let result = sqlx::query(
            "INSERT INTO research_pdf_extractions \
             (id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, \
              hash_algorithm, extraction_hash, extracted_at_ms, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(source_snapshot_id, extractor, extractor_version, extraction_hash) DO NOTHING",
        )
        .bind(value.id.as_str())
        .bind(value.source_snapshot_id.as_str())
        .bind(&value.artifact_id)
        .bind(&value.extractor)
        .bind(&value.extractor_version)
        .bind(value.page_count as i64)
        .bind(enum_text(&value.extraction_hash.algorithm))
        .bind(&value.extraction_hash.value)
        .bind(value.extracted_at_ms)
        .bind(enum_text(&value.status))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub(super) async fn insert_pdf_extraction_with_pages(
        &self,
        extraction: &ResearchPdfExtraction,
        pages: &[ResearchPdfPage],
    ) -> Result<bool, ResearchError> {
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO research_pdf_extractions \
             (id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, \
              hash_algorithm, extraction_hash, extracted_at_ms, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(source_snapshot_id, extractor, extractor_version, extraction_hash) DO NOTHING",
        )
        .bind(extraction.id.as_str())
        .bind(extraction.source_snapshot_id.as_str())
        .bind(&extraction.artifact_id)
        .bind(&extraction.extractor)
        .bind(&extraction.extractor_version)
        .bind(extraction.page_count as i64)
        .bind(enum_text(&extraction.extraction_hash.algorithm))
        .bind(&extraction.extraction_hash.value)
        .bind(extraction.extracted_at_ms)
        .bind(enum_text(&extraction.status))
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 1 {
            for page in pages {
                sqlx::query(
                    "INSERT INTO research_pdf_pages \
                     (extraction_id, page, text, hash_algorithm, text_hash) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(page.extraction_id.as_str())
                .bind(page.page as i64)
                .bind(&page.text)
                .bind(enum_text(&page.text_hash.algorithm))
                .bind(&page.text_hash.value)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    pub(super) async fn list_pdf_pages(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        start_page: u32,
        limit: u32,
    ) -> Result<Vec<ResearchPdfPage>, ResearchError> {
        let rows = sqlx::query(
            "SELECT extraction_id, page, text, hash_algorithm, text_hash \
             FROM research_pdf_pages WHERE extraction_id = ? AND page >= ? \
             ORDER BY page ASC LIMIT ?",
        )
        .bind(extraction_id.as_str())
        .bind(start_page as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_pdf_page).collect()
    }

    pub(super) async fn get_pdf_page(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        page: u32,
    ) -> Result<Option<ResearchPdfPage>, ResearchError> {
        let row = sqlx::query(
            "SELECT extraction_id, page, text, hash_algorithm, text_hash \
             FROM research_pdf_pages WHERE extraction_id = ? AND page = ?",
        )
        .bind(extraction_id.as_str())
        .bind(page as i64)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_page).transpose()
    }

    pub(super) async fn insert_pdf_page(
        &self,
        value: &ResearchPdfPage,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_pdf_pages \
             (extraction_id, page, text, hash_algorithm, text_hash) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.extraction_id.as_str())
        .bind(value.page as i64)
        .bind(&value.text)
        .bind(enum_text(&value.text_hash.algorithm))
        .bind(&value.text_hash.value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn pdf_extraction_select(where_clause: &str, order_by: &str) -> String {
    format!(
        "SELECT id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, \
         hash_algorithm, extraction_hash, extracted_at_ms, status \
         FROM research_pdf_extractions {where_clause} ORDER BY {order_by}"
    )
}

fn map_pdf_extraction(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ResearchPdfExtraction, ResearchError> {
    Ok(ResearchPdfExtraction {
        id: ResearchPdfExtractionId::parse(row.get::<String, _>("id"))?,
        source_snapshot_id: ResearchSourceSnapshotId::parse(
            row.get::<String, _>("source_snapshot_id"),
        )?,
        artifact_id: row.get("artifact_id"),
        extractor: row.get("extractor"),
        extractor_version: row.get("extractor_version"),
        page_count: row.get::<i64, _>("page_count") as u32,
        extraction_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "PDF extraction hash algorithm",
            )?,
            value: row.get("extraction_hash"),
        },
        extracted_at_ms: row.get("extracted_at_ms"),
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "PDF extraction status",
        )?,
    })
}

fn map_pdf_page(row: sqlx::sqlite::SqliteRow) -> Result<ResearchPdfPage, ResearchError> {
    Ok(ResearchPdfPage {
        extraction_id: ResearchPdfExtractionId::parse(row.get::<String, _>("extraction_id"))?,
        page: row.get::<i64, _>("page") as u32,
        text: row.get("text"),
        text_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "PDF page hash algorithm",
            )?,
            value: row.get("text_hash"),
        },
    })
}
