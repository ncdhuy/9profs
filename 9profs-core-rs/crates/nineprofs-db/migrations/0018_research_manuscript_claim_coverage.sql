CREATE TABLE research_manuscript_claim_coverage_runs (
    coverage_run_id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    claim_inventory_run_id TEXT NOT NULL,
    citation_review_run_id TEXT NOT NULL,
    analysis_contract_version TEXT NOT NULL,
    coverage_contract_version TEXT NOT NULL,
    coverage_scope TEXT NOT NULL,
    coverage_limitations_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_inventory_run_id) REFERENCES research_manuscript_claim_inventory_runs(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_review_run_id) REFERENCES research_manuscript_citation_review_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_claim_coverage_runs_case
    ON research_manuscript_claim_coverage_runs (research_case_id, created_at_ms DESC, coverage_run_id DESC);

CREATE TABLE research_manuscript_claim_coverage_items (
    coverage_item_id TEXT PRIMARY KEY NOT NULL,
    coverage_run_id TEXT NOT NULL,
    inventory_item_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    bridge_status TEXT NOT NULL CHECK (bridge_status IN (
        'exact_claim_bridge', 'no_citation_scoped_claim_match',
        'same_span_different_claim', 'multiple_exact_candidates', 'invalid_cross_history'
    )),
    structural_citation_state TEXT NOT NULL CHECK (structural_citation_state IN (
        'exact_citation_linked', 'citation_observed_in_claim_range',
        'citation_observed_in_block', 'no_citation_observed_in_block',
        'ambiguous_claim_bridge'
    )),
    matched_claim_extraction_item_id TEXT,
    matched_research_claim_id TEXT,
    inventory_overlapping_citation_count INTEGER NOT NULL CHECK (inventory_overlapping_citation_count >= 0),
    same_block_citation_count INTEGER NOT NULL CHECK (same_block_citation_count >= 0),
    claim_range_citation_count INTEGER NOT NULL CHECK (claim_range_citation_count >= 0),
    exact_claim_citation_link_count INTEGER NOT NULL CHECK (exact_claim_citation_link_count >= 0),
    target_count INTEGER NOT NULL CHECK (target_count >= 0),
    support_count INTEGER NOT NULL CHECK (support_count >= 0),
    contradiction_count INTEGER NOT NULL CHECK (contradiction_count >= 0),
    contextualize_count INTEGER NOT NULL CHECK (contextualize_count >= 0),
    insufficient_count INTEGER NOT NULL CHECK (insufficient_count >= 0),
    unverified_count INTEGER NOT NULL CHECK (unverified_count >= 0),
    blocked_count INTEGER NOT NULL CHECK (blocked_count >= 0),
    UNIQUE (coverage_run_id, inventory_item_id),
    UNIQUE (coverage_run_id, ordinal),
    FOREIGN KEY (coverage_run_id) REFERENCES research_manuscript_claim_coverage_runs(coverage_run_id) ON DELETE CASCADE,
    FOREIGN KEY (inventory_item_id) REFERENCES research_manuscript_claim_inventory_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (matched_claim_extraction_item_id) REFERENCES research_manuscript_claim_extraction_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (matched_research_claim_id) REFERENCES research_claims(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_claim_coverage_items_run
    ON research_manuscript_claim_coverage_items (coverage_run_id, ordinal);

CREATE TABLE research_manuscript_claim_coverage_targets (
    coverage_target_id TEXT PRIMARY KEY NOT NULL,
    coverage_item_id TEXT NOT NULL,
    claim_citation_link_id TEXT NOT NULL,
    citation_occurrence_id TEXT NOT NULL,
    citation_target_id TEXT NOT NULL,
    citation_review_item_id TEXT NOT NULL,
    binding_id TEXT,
    source_id TEXT,
    source_snapshot_id TEXT,
    extraction_id TEXT,
    verification_run_id TEXT,
    review_status TEXT NOT NULL CHECK (review_status IN (
        'unresolved_reference', 'ambiguous_reference',
        'reference_requires_confirmation', 'source_matched_not_verification_ready',
        'binding_conflict', 'ready_for_verification', 'verification_running',
        'verification_completed', 'verification_failed', 'resolution_failed'
    )),
    failure_code TEXT,
    verification_status TEXT CHECK (verification_status IN ('running', 'completed', 'failed')),
    verification_failure_code TEXT,
    relation TEXT CHECK (relation IN ('supports', 'contradicts', 'contextualizes', 'insufficient')),
    rationale TEXT,
    evidence_count INTEGER NOT NULL CHECK (evidence_count >= 0),
    UNIQUE (coverage_item_id, claim_citation_link_id, citation_target_id),
    FOREIGN KEY (coverage_item_id) REFERENCES research_manuscript_claim_coverage_items(coverage_item_id) ON DELETE CASCADE,
    FOREIGN KEY (claim_citation_link_id) REFERENCES research_claim_citations(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_target_id) REFERENCES research_citation_targets(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_review_item_id) REFERENCES research_manuscript_citation_review_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (binding_id) REFERENCES research_citation_target_bindings(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT,
    FOREIGN KEY (verification_run_id) REFERENCES research_citation_verification_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_claim_coverage_targets_item
    ON research_manuscript_claim_coverage_targets (coverage_item_id, citation_target_id);

CREATE UNIQUE INDEX idx_research_manuscript_claim_coverage_runs_completed_identity
    ON research_manuscript_claim_coverage_runs (
        claim_inventory_run_id, citation_review_run_id, analysis_contract_version
    )
    WHERE status = 'completed';
