CREATE TABLE IF NOT EXISTS research_regulation_requirement_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    pdf_extraction_id TEXT NOT NULL,
    source_locator_json TEXT NOT NULL,
    authority_locator_suggestion_json TEXT,
    ocr_excerpt TEXT NOT NULL,
    normalized_requirement TEXT NOT NULL,
    applicability_suggestion_json TEXT NOT NULL,
    extraction_json TEXT NOT NULL,
    risk_flags_json TEXT NOT NULL,
    review_notes TEXT,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE CASCADE,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (pdf_extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_regulation_requirement_candidates_source_idx
    ON research_regulation_requirement_candidates(source_id, id);

CREATE INDEX IF NOT EXISTS research_regulation_requirement_candidates_snapshot_idx
    ON research_regulation_requirement_candidates(source_snapshot_id, id);

CREATE INDEX IF NOT EXISTS research_regulation_requirement_candidates_extraction_idx
    ON research_regulation_requirement_candidates(pdf_extraction_id, id);
