ALTER TABLE research_manuscript_research_review_runs
    ADD COLUMN execution_identity_hash_algorithm TEXT;

ALTER TABLE research_manuscript_research_review_runs
    ADD COLUMN execution_identity_hash TEXT;

DROP INDEX IF EXISTS uq_research_manuscript_research_review_completed_input;
DROP INDEX IF EXISTS uq_research_manuscript_research_review_completed_children;

CREATE UNIQUE INDEX uq_research_manuscript_research_review_completed_execution_identity
    ON research_manuscript_research_review_runs(
        input_hash_algorithm,
        input_hash,
        execution_identity_hash_algorithm,
        execution_identity_hash,
        review_contract_version
    )
    WHERE status = 'completed'
      AND execution_identity_hash_algorithm IS NOT NULL
      AND execution_identity_hash IS NOT NULL;
