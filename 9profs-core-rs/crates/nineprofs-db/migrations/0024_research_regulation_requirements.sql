CREATE TABLE IF NOT EXISTS research_regulation_requirements (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    pdf_extraction_id TEXT,
    text TEXT NOT NULL,
    source_excerpt TEXT NOT NULL,
    source_excerpt_hash_algorithm TEXT NOT NULL,
    source_excerpt_hash TEXT NOT NULL,
    source_locator_json TEXT NOT NULL,
    authority_locator_json TEXT,
    applicability_json TEXT NOT NULL,
    effective_from INTEGER,
    effective_until INTEGER,
    extraction_method TEXT NOT NULL,
    extraction_contract_version TEXT,
    review_status TEXT NOT NULL CHECK (review_status IN ('needs_review', 'approved', 'rejected')),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK (effective_from IS NULL OR effective_until IS NULL OR effective_from <= effective_until),
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE CASCADE,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (pdf_extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_regulation_requirements_source_idx
    ON research_regulation_requirements(source_id, id);

CREATE INDEX IF NOT EXISTS research_regulation_requirements_snapshot_idx
    ON research_regulation_requirements(source_snapshot_id, id);
