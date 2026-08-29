CREATE TABLE research_manuscript_research_review_runs (
    review_run_id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    input_hash_algorithm TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    citation_review_run_id TEXT,
    claim_inventory_run_id TEXT,
    claim_coverage_run_id TEXT,
    citation_expectation_run_id TEXT,
    cross_claim_candidate_run_id TEXT,
    cross_claim_assessment_run_id TEXT,
    review_contract_version TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    failure_stage TEXT,
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_review_run_id) REFERENCES research_manuscript_citation_review_runs(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_inventory_run_id) REFERENCES research_manuscript_claim_inventory_runs(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_coverage_run_id) REFERENCES research_manuscript_claim_coverage_runs(coverage_run_id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_expectation_run_id) REFERENCES research_manuscript_citation_expectation_runs(expectation_run_id) ON DELETE RESTRICT,
    FOREIGN KEY (cross_claim_candidate_run_id) REFERENCES research_manuscript_cross_claim_candidate_runs(candidate_run_id) ON DELETE RESTRICT,
    FOREIGN KEY (cross_claim_assessment_run_id) REFERENCES research_manuscript_cross_claim_assessment_runs(assessment_run_id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_research_review_case
    ON research_manuscript_research_review_runs(research_case_id, created_at_ms DESC, review_run_id DESC);

CREATE UNIQUE INDEX uq_research_manuscript_research_review_completed_input
    ON research_manuscript_research_review_runs(input_hash_algorithm, input_hash, review_contract_version)
    WHERE status = 'completed';

CREATE UNIQUE INDEX uq_research_manuscript_research_review_completed_children
    ON research_manuscript_research_review_runs(
        citation_review_run_id,
        claim_inventory_run_id,
        claim_coverage_run_id,
        citation_expectation_run_id,
        cross_claim_candidate_run_id,
        cross_claim_assessment_run_id,
        review_contract_version
    )
    WHERE status = 'completed';
