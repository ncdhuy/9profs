CREATE TABLE IF NOT EXISTS research_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    hash_algorithm TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    size_bytes INTEGER NOT NULL,
    media_type TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS research_pdf_extractions (
    id TEXT PRIMARY KEY NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    extractor TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    page_count INTEGER NOT NULL,
    hash_algorithm TEXT NOT NULL,
    extraction_hash TEXT NOT NULL,
    extracted_at_ms INTEGER NOT NULL,
    status TEXT NOT NULL,
    UNIQUE (source_snapshot_id, extractor, extractor_version, extraction_hash),
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (artifact_id) REFERENCES research_artifacts(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_pdf_extractions_snapshot_idx
    ON research_pdf_extractions(source_snapshot_id, extracted_at_ms DESC);

CREATE TABLE IF NOT EXISTS research_pdf_pages (
    extraction_id TEXT NOT NULL,
    page INTEGER NOT NULL,
    text TEXT NOT NULL,
    hash_algorithm TEXT NOT NULL,
    text_hash TEXT NOT NULL,
    PRIMARY KEY (extraction_id, page),
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

ALTER TABLE research_evidence
    ADD COLUMN pdf_extraction_id TEXT REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS research_evidence_pdf_extraction_idx
    ON research_evidence(pdf_extraction_id);
