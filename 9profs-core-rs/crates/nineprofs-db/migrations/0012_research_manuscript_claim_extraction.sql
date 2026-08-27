CREATE TABLE research_manuscript_claim_extraction_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    citation_sync_run_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    context_hash_algorithm TEXT NOT NULL,
    context_hash TEXT NOT NULL,
    extractor_provider TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    extractor_model_id TEXT,
    extraction_contract_version TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    claim_count INTEGER NOT NULL CHECK (claim_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    failure_code TEXT,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_sync_run_id) REFERENCES research_manuscript_citation_sync_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_claim_extraction_runs_sync
    ON research_manuscript_claim_extraction_runs
       (citation_sync_run_id, created_at_ms DESC, id DESC);

CREATE INDEX idx_research_manuscript_claim_extraction_runs_identity
    ON research_manuscript_claim_extraction_runs
       (citation_sync_run_id, context_hash, extractor_provider, extractor_version,
        extractor_model_id, extraction_contract_version, status);

CREATE TABLE research_manuscript_claim_extraction_items (
    id TEXT PRIMARY KEY NOT NULL,
    extraction_run_id TEXT NOT NULL,
    research_claim_id TEXT NOT NULL,
    document_block_id TEXT NOT NULL,
    source_start INTEGER NOT NULL,
    source_end INTEGER NOT NULL CHECK (source_start < source_end),
    source_excerpt TEXT NOT NULL,
    source_excerpt_hash_algorithm TEXT NOT NULL,
    source_excerpt_hash TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    UNIQUE (extraction_run_id, ordinal),
    UNIQUE (extraction_run_id, document_block_id, source_start, source_end, source_excerpt_hash),
    FOREIGN KEY (extraction_run_id) REFERENCES research_manuscript_claim_extraction_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (research_claim_id) REFERENCES research_claims(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_claim_extraction_items_run
    ON research_manuscript_claim_extraction_items (extraction_run_id, ordinal);

CREATE INDEX idx_research_manuscript_claim_extraction_items_claim
    ON research_manuscript_claim_extraction_items (research_claim_id);

CREATE TABLE research_manuscript_claim_extraction_citations (
    id TEXT PRIMARY KEY NOT NULL,
    extraction_run_id TEXT NOT NULL,
    extraction_item_id TEXT,
    claim_citation_link_id TEXT,
    citation_occurrence_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('associated_with_claim', 'no_verifiable_claim')),
    reason TEXT,
    UNIQUE (extraction_run_id, extraction_item_id, claim_citation_link_id, citation_occurrence_id),
    CHECK ((status = 'associated_with_claim' AND extraction_item_id IS NOT NULL AND claim_citation_link_id IS NOT NULL)
        OR (status = 'no_verifiable_claim' AND extraction_item_id IS NULL AND claim_citation_link_id IS NULL)),
    FOREIGN KEY (extraction_run_id) REFERENCES research_manuscript_claim_extraction_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (extraction_item_id) REFERENCES research_manuscript_claim_extraction_items(id) ON DELETE CASCADE,
    FOREIGN KEY (claim_citation_link_id) REFERENCES research_claim_citations(id) ON DELETE CASCADE,
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_claim_extraction_citations_run
    ON research_manuscript_claim_extraction_citations (extraction_run_id, citation_occurrence_id);

CREATE INDEX idx_research_manuscript_claim_extraction_citations_citation
    ON research_manuscript_claim_extraction_citations (citation_occurrence_id);

CREATE INDEX idx_research_manuscript_claim_extraction_citations_item
    ON research_manuscript_claim_extraction_citations (extraction_item_id);

CREATE INDEX idx_research_manuscript_claim_extraction_citations_link
    ON research_manuscript_claim_extraction_citations (claim_citation_link_id);

CREATE UNIQUE INDEX idx_research_manuscript_claim_extraction_citations_unassociated
    ON research_manuscript_claim_extraction_citations (extraction_run_id, citation_occurrence_id)
    WHERE extraction_item_id IS NULL AND claim_citation_link_id IS NULL;
