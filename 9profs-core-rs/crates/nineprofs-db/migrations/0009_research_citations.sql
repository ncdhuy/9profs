CREATE TABLE IF NOT EXISTS research_citation_occurrences (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    origin_json TEXT NOT NULL,
    rendered_text TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_citation_occurrences_case_idx
    ON research_citation_occurrences(research_case_id, created_at_ms ASC, id ASC);

CREATE TABLE IF NOT EXISTS research_citation_targets (
    id TEXT PRIMARY KEY NOT NULL,
    citation_occurrence_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    reference_key TEXT NOT NULL,
    cited_locator TEXT,
    UNIQUE (citation_occurrence_id, ordinal),
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_citation_targets_occurrence_idx
    ON research_citation_targets(citation_occurrence_id, ordinal ASC);

CREATE TABLE IF NOT EXISTS research_citation_target_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    citation_target_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_snapshot_id TEXT,
    extraction_id TEXT,
    method TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_target_id) REFERENCES research_citation_targets(id) ON DELETE CASCADE,
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_citation_target_bindings_target_idx
    ON research_citation_target_bindings(citation_target_id, created_at_ms ASC, id ASC);

CREATE TABLE IF NOT EXISTS research_claim_citations (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    citation_occurrence_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    UNIQUE (claim_id, citation_occurrence_id),
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE,
    FOREIGN KEY (claim_id) REFERENCES research_claims(id) ON DELETE CASCADE,
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_claim_citations_case_idx
    ON research_claim_citations(research_case_id);

CREATE INDEX IF NOT EXISTS research_claim_citations_claim_idx
    ON research_claim_citations(claim_id);

CREATE INDEX IF NOT EXISTS research_claim_citations_occurrence_idx
    ON research_claim_citations(citation_occurrence_id);
