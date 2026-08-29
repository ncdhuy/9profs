CREATE TABLE research_manuscript_citation_expectation_runs (
    expectation_run_id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    claim_coverage_run_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    assessor_version TEXT NOT NULL,
    model_id TEXT,
    expectation_contract_version TEXT NOT NULL,
    coverage_contract_version TEXT NOT NULL,
    coverage_scope TEXT NOT NULL,
    coverage_limitations_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    failed_item_count INTEGER NOT NULL CHECK (failed_item_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_coverage_run_id)
        REFERENCES research_manuscript_claim_coverage_runs(coverage_run_id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_citation_expectation_runs_case
    ON research_manuscript_citation_expectation_runs
       (research_case_id, created_at_ms DESC, expectation_run_id DESC);

CREATE UNIQUE INDEX uq_research_manuscript_citation_expectation_completed
    ON research_manuscript_citation_expectation_runs (
        claim_coverage_run_id,
        provider_id,
        assessor_version,
        COALESCE(model_id, ''),
        expectation_contract_version
    )
    WHERE status = 'completed' AND failed_item_count = 0;

CREATE TABLE research_manuscript_citation_expectation_items (
    expectation_item_id TEXT PRIMARY KEY NOT NULL,
    expectation_run_id TEXT NOT NULL,
    coverage_item_id TEXT NOT NULL,
    inventory_item_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    claim_text TEXT NOT NULL,
    source_excerpt TEXT NOT NULL,
    review_kind TEXT NOT NULL CHECK (review_kind IN (
        'external_evidence', 'manuscript_internal', 'interpretive',
        'non_evidentiary', 'uncertain'
    )),
    block_kind TEXT NOT NULL CHECK (block_kind IN ('paragraph', 'heading', 'list_item')),
    assessment_status TEXT NOT NULL CHECK (assessment_status IN ('assessed', 'assessment_failed')),
    expectation TEXT CHECK (expectation IS NULL OR expectation IN (
        'external_evidence_expected', 'external_evidence_context_dependent',
        'manuscript_internal_support', 'no_external_citation_expected', 'uncertain'
    )),
    attention TEXT NOT NULL CHECK (attention IN (
        'no_coverage_attention_detected', 'review_suggested',
        'expectation_review_needed', 'assessment_unavailable'
    )),
    attention_reasons_json TEXT NOT NULL,
    rationale TEXT,
    failure_code TEXT,
    UNIQUE (expectation_run_id, coverage_item_id),
    UNIQUE (expectation_run_id, ordinal),
    FOREIGN KEY (expectation_run_id)
        REFERENCES research_manuscript_citation_expectation_runs(expectation_run_id)
        ON DELETE CASCADE,
    FOREIGN KEY (coverage_item_id)
        REFERENCES research_manuscript_claim_coverage_items(coverage_item_id) ON DELETE RESTRICT,
    FOREIGN KEY (inventory_item_id)
        REFERENCES research_manuscript_claim_inventory_items(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_citation_expectation_items_run
    ON research_manuscript_citation_expectation_items (expectation_run_id, ordinal);
