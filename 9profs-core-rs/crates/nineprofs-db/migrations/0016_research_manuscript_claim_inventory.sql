CREATE TABLE research_manuscript_claim_inventory_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    document_context_hash_algorithm TEXT NOT NULL,
    document_context_hash TEXT NOT NULL,
    extractor_provider TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    extractor_model_id TEXT,
    extraction_contract_version TEXT NOT NULL,
    coverage_contract_version TEXT NOT NULL,
    coverage_scope TEXT NOT NULL,
    coverage_limitations_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    covered_block_count INTEGER NOT NULL CHECK (covered_block_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    failure_code TEXT,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_claim_inventory_runs_lookup
    ON research_manuscript_claim_inventory_runs
       (research_case_id, manuscript_source_id, document_id, document_version, created_at_ms DESC, id DESC);

CREATE INDEX idx_research_manuscript_claim_inventory_runs_identity
    ON research_manuscript_claim_inventory_runs
       (research_case_id, manuscript_source_id, document_id, document_version,
        document_context_hash, extractor_provider, extractor_version,
        extractor_model_id, extraction_contract_version, status);

CREATE TABLE research_manuscript_claim_inventory_items (
    id TEXT PRIMARY KEY NOT NULL,
    inventory_run_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    document_block_id TEXT NOT NULL,
    block_ordinal INTEGER NOT NULL CHECK (block_ordinal >= 0),
    block_kind TEXT NOT NULL CHECK (block_kind IN ('paragraph', 'heading', 'list_item')),
    source_start INTEGER NOT NULL,
    source_end INTEGER NOT NULL CHECK (source_start < source_end),
    source_excerpt TEXT NOT NULL,
    source_excerpt_hash_algorithm TEXT NOT NULL,
    source_excerpt_hash TEXT NOT NULL,
    claim_text TEXT NOT NULL,
    review_kind TEXT NOT NULL CHECK (review_kind IN (
        'external_evidence', 'manuscript_internal', 'interpretive', 'non_evidentiary', 'uncertain'
    )),
    overlapping_citation_count INTEGER NOT NULL CHECK (overlapping_citation_count >= 0),
    UNIQUE (inventory_run_id, ordinal),
    UNIQUE (inventory_run_id, document_block_id, source_start, source_end, source_excerpt_hash, claim_text),
    FOREIGN KEY (inventory_run_id) REFERENCES research_manuscript_claim_inventory_runs(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_claim_inventory_items_run
    ON research_manuscript_claim_inventory_items (inventory_run_id, ordinal);

CREATE TABLE research_manuscript_claim_inventory_coverage (
    id TEXT PRIMARY KEY NOT NULL,
    inventory_run_id TEXT NOT NULL,
    document_block_id TEXT NOT NULL,
    block_ordinal INTEGER NOT NULL CHECK (block_ordinal >= 0),
    block_kind TEXT NOT NULL CHECK (block_kind IN ('paragraph', 'heading', 'list_item')),
    status TEXT NOT NULL CHECK (status IN ('processed', 'no_claims', 'excluded')),
    reason TEXT,
    UNIQUE (inventory_run_id, document_block_id),
    UNIQUE (inventory_run_id, block_ordinal),
    FOREIGN KEY (inventory_run_id) REFERENCES research_manuscript_claim_inventory_runs(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_claim_inventory_coverage_run
    ON research_manuscript_claim_inventory_coverage (inventory_run_id, block_ordinal);
