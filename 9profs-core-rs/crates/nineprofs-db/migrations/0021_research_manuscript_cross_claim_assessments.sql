CREATE TABLE research_manuscript_cross_claim_assessment_runs (
    assessment_run_id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    candidate_run_id TEXT NOT NULL,
    claim_inventory_run_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    model_id TEXT,
    assessor_implementation_version TEXT NOT NULL,
    assessment_contract_version TEXT NOT NULL,
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 0),
    assessed_count INTEGER NOT NULL CHECK (assessed_count >= 0),
    failed_item_count INTEGER NOT NULL CHECK (failed_item_count >= 0),
    conflict_count INTEGER NOT NULL CHECK (conflict_count >= 0),
    compatible_count INTEGER NOT NULL CHECK (compatible_count >= 0),
    qualification_count INTEGER NOT NULL CHECK (qualification_count >= 0),
    equivalent_count INTEGER NOT NULL CHECK (equivalent_count >= 0),
    not_comparable_count INTEGER NOT NULL CHECK (not_comparable_count >= 0),
    insufficient_context_count INTEGER NOT NULL CHECK (insufficient_context_count >= 0),
    failed_assessment_count INTEGER NOT NULL CHECK (failed_assessment_count >= 0),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (candidate_run_id) REFERENCES research_manuscript_cross_claim_candidate_runs(candidate_run_id) ON DELETE CASCADE,
    FOREIGN KEY (claim_inventory_run_id) REFERENCES research_manuscript_claim_inventory_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_cross_claim_assessment_runs_case
    ON research_manuscript_cross_claim_assessment_runs(research_case_id, created_at_ms DESC, assessment_run_id DESC);

CREATE UNIQUE INDEX uq_research_manuscript_cross_claim_assessment_completed
    ON research_manuscript_cross_claim_assessment_runs(
        candidate_run_id,
        provider_id,
        COALESCE(model_id, ''),
        assessor_implementation_version,
        assessment_contract_version
    )
    WHERE status = 'completed' AND failed_item_count = 0;

CREATE TABLE research_manuscript_cross_claim_assessment_items (
    assessment_item_id TEXT PRIMARY KEY NOT NULL,
    assessment_run_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    left_inventory_item_id TEXT NOT NULL,
    right_inventory_item_id TEXT NOT NULL,
    left_ordinal INTEGER NOT NULL CHECK (left_ordinal >= 0),
    right_ordinal INTEGER NOT NULL CHECK (right_ordinal >= 0),
    assessment_status TEXT NOT NULL CHECK (assessment_status IN ('assessed', 'assessment_failed')),
    relation TEXT CHECK (relation IN (
        'conflict', 'compatible', 'qualification_or_refinement',
        'equivalent_or_restatement', 'not_meaningfully_comparable', 'insufficient_context'
    )),
    dimensions_json TEXT NOT NULL,
    rationale TEXT,
    failure_code TEXT,
    attention TEXT NOT NULL CHECK (attention IN (
        'no_internal_consistency_attention_detected', 'review_suggested',
        'context_review_needed', 'assessment_unavailable'
    )),
    attention_reasons_json TEXT NOT NULL,
    UNIQUE (assessment_run_id, candidate_id),
    FOREIGN KEY (assessment_run_id) REFERENCES research_manuscript_cross_claim_assessment_runs(assessment_run_id) ON DELETE CASCADE,
    FOREIGN KEY (candidate_id) REFERENCES research_manuscript_cross_claim_candidates(candidate_id) ON DELETE CASCADE,
    FOREIGN KEY (left_inventory_item_id) REFERENCES research_manuscript_claim_inventory_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (right_inventory_item_id) REFERENCES research_manuscript_claim_inventory_items(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_cross_claim_assessment_items_run
    ON research_manuscript_cross_claim_assessment_items(assessment_run_id, left_ordinal, right_ordinal, candidate_id);

CREATE INDEX idx_research_manuscript_cross_claim_assessment_items_candidate
    ON research_manuscript_cross_claim_assessment_items(candidate_id);
