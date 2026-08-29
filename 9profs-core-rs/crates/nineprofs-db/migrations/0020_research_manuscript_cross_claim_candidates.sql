CREATE TABLE research_manuscript_cross_claim_candidate_runs (
    candidate_run_id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    claim_inventory_run_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    model_id TEXT,
    discovery_implementation_version TEXT NOT NULL,
    discovery_contract_version TEXT NOT NULL,
    claim_count INTEGER NOT NULL CHECK (claim_count >= 0),
    batch_count INTEGER NOT NULL CHECK (batch_count >= 0),
    expected_window_count INTEGER NOT NULL CHECK (expected_window_count >= 0),
    processed_window_count INTEGER NOT NULL CHECK (processed_window_count >= 0),
    candidate_pair_count INTEGER NOT NULL CHECK (candidate_pair_count >= 0),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_inventory_run_id)
        REFERENCES research_manuscript_claim_inventory_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_cross_claim_candidate_runs_case
    ON research_manuscript_cross_claim_candidate_runs
       (research_case_id, created_at_ms DESC, candidate_run_id DESC);

CREATE UNIQUE INDEX uq_research_manuscript_cross_claim_candidate_completed
    ON research_manuscript_cross_claim_candidate_runs (
        claim_inventory_run_id,
        provider_id,
        COALESCE(model_id, ''),
        discovery_implementation_version,
        discovery_contract_version
    )
    WHERE status = 'completed';

CREATE TABLE research_manuscript_cross_claim_comparison_windows (
    window_id TEXT NOT NULL,
    candidate_run_id TEXT NOT NULL,
    left_batch_ordinal INTEGER NOT NULL CHECK (left_batch_ordinal >= 0),
    right_batch_ordinal INTEGER NOT NULL CHECK (right_batch_ordinal >= left_batch_ordinal),
    same_batch INTEGER NOT NULL CHECK (same_batch IN (0, 1)),
    status TEXT NOT NULL CHECK (status IN ('pending', 'processed', 'failed')),
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 0),
    failure_code TEXT,
    PRIMARY KEY (candidate_run_id, window_id),
    UNIQUE (candidate_run_id, left_batch_ordinal, right_batch_ordinal),
    FOREIGN KEY (candidate_run_id)
        REFERENCES research_manuscript_cross_claim_candidate_runs(candidate_run_id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_cross_claim_windows_run
    ON research_manuscript_cross_claim_comparison_windows
       (candidate_run_id, left_batch_ordinal, right_batch_ordinal);

CREATE TABLE research_manuscript_cross_claim_candidates (
    candidate_id TEXT PRIMARY KEY NOT NULL,
    candidate_run_id TEXT NOT NULL,
    comparison_window_id TEXT NOT NULL,
    left_inventory_item_id TEXT NOT NULL,
    right_inventory_item_id TEXT NOT NULL,
    left_ordinal INTEGER NOT NULL CHECK (left_ordinal >= 0),
    right_ordinal INTEGER NOT NULL CHECK (right_ordinal > left_ordinal),
    candidate_kinds_json TEXT NOT NULL,
    rationale TEXT NOT NULL,
    UNIQUE (candidate_run_id, left_inventory_item_id, right_inventory_item_id),
    FOREIGN KEY (candidate_run_id)
        REFERENCES research_manuscript_cross_claim_candidate_runs(candidate_run_id) ON DELETE CASCADE,
    FOREIGN KEY (candidate_run_id, comparison_window_id)
        REFERENCES research_manuscript_cross_claim_comparison_windows(candidate_run_id, window_id)
        ON DELETE CASCADE,
    FOREIGN KEY (left_inventory_item_id)
        REFERENCES research_manuscript_claim_inventory_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (right_inventory_item_id)
        REFERENCES research_manuscript_claim_inventory_items(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_cross_claim_candidates_run
    ON research_manuscript_cross_claim_candidates
       (candidate_run_id, left_ordinal, right_ordinal);
