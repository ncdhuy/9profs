CREATE TABLE research_manuscript_citation_review_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    citation_sync_run_id TEXT,
    reference_catalog_run_id TEXT,
    reference_resolution_run_id TEXT,
    claim_extraction_run_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    failure_stage TEXT,
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_sync_run_id) REFERENCES research_manuscript_citation_sync_runs(id) ON DELETE RESTRICT,
    FOREIGN KEY (reference_catalog_run_id) REFERENCES research_manuscript_reference_catalog_runs(id) ON DELETE RESTRICT,
    FOREIGN KEY (reference_resolution_run_id) REFERENCES research_manuscript_reference_resolution_runs(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_extraction_run_id) REFERENCES research_manuscript_claim_extraction_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_citation_review_runs_case
    ON research_manuscript_citation_review_runs (research_case_id, created_at_ms DESC, id DESC);
CREATE INDEX idx_research_citation_review_runs_document
    ON research_manuscript_citation_review_runs (document_id, document_version, created_at_ms DESC);

CREATE TABLE research_manuscript_citation_review_items (
    id TEXT PRIMARY KEY NOT NULL,
    review_run_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    claim_id TEXT NOT NULL,
    claim_citation_link_id TEXT NOT NULL,
    citation_occurrence_id TEXT NOT NULL,
    citation_target_id TEXT NOT NULL,
    reference_entry_id TEXT,
    resolution_entry_id TEXT,
    resolution_outcome TEXT,
    binding_id TEXT,
    binding_method TEXT,
    source_id TEXT,
    source_snapshot_id TEXT,
    extraction_id TEXT,
    document_block_id TEXT NOT NULL,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL CHECK (start < end),
    rendered_text TEXT NOT NULL,
    reference_key TEXT NOT NULL,
    cited_locator TEXT,
    claim_text TEXT NOT NULL,
    source_excerpt TEXT,
    status TEXT NOT NULL CHECK (status IN (
        'unresolved_reference', 'ambiguous_reference',
        'reference_requires_confirmation', 'source_matched_not_verification_ready',
        'binding_conflict', 'ready_for_verification', 'verification_running',
        'verification_completed', 'verification_failed', 'resolution_failed'
    )),
    failure_code TEXT,
    verification_run_id TEXT,
    UNIQUE (review_run_id, ordinal),
    UNIQUE (review_run_id, claim_citation_link_id, citation_target_id),
    FOREIGN KEY (review_run_id) REFERENCES research_manuscript_citation_review_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (claim_id) REFERENCES research_claims(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_citation_link_id) REFERENCES research_claim_citations(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_target_id) REFERENCES research_citation_targets(id) ON DELETE RESTRICT,
    FOREIGN KEY (reference_entry_id) REFERENCES research_manuscript_reference_entries(id) ON DELETE RESTRICT,
    FOREIGN KEY (resolution_entry_id) REFERENCES research_manuscript_reference_resolution_entries(id) ON DELETE RESTRICT,
    FOREIGN KEY (binding_id) REFERENCES research_citation_target_bindings(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT,
    FOREIGN KEY (verification_run_id) REFERENCES research_citation_verification_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_citation_review_items_run_ordinal
    ON research_manuscript_citation_review_items (review_run_id, ordinal);
CREATE INDEX idx_research_citation_review_items_target
    ON research_manuscript_citation_review_items (citation_target_id, review_run_id);
