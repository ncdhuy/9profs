CREATE TABLE IF NOT EXISTS research_citation_verification_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    claim_citation_link_id TEXT NOT NULL,
    citation_target_binding_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    citation_occurrence_id TEXT NOT NULL,
    citation_target_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    extraction_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE,
    FOREIGN KEY (claim_citation_link_id) REFERENCES research_claim_citations(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_target_binding_id) REFERENCES research_citation_target_bindings(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_id) REFERENCES research_claims(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_target_id) REFERENCES research_citation_targets(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_citation_verification_runs_claim_idx
    ON research_citation_verification_runs(claim_id, created_at_ms ASC, id ASC);

CREATE INDEX IF NOT EXISTS research_citation_verification_runs_binding_idx
    ON research_citation_verification_runs(citation_target_binding_id, created_at_ms ASC, id ASC);

CREATE TABLE IF NOT EXISTS research_citation_verification_candidates (
    verification_run_id TEXT NOT NULL,
    retrieval_chunk_id TEXT NOT NULL,
    research_source_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    extraction_id TEXT NOT NULL,
    page INTEGER NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    excerpt_hash TEXT NOT NULL,
    rank INTEGER NOT NULL,
    retrieval_score REAL NOT NULL,
    PRIMARY KEY (verification_run_id, retrieval_chunk_id),
    FOREIGN KEY (verification_run_id) REFERENCES research_citation_verification_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (retrieval_chunk_id) REFERENCES research_retrieval_chunks(id) ON DELETE RESTRICT,
    FOREIGN KEY (research_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_citation_verification_candidates_run_rank_idx
    ON research_citation_verification_candidates(verification_run_id, rank ASC, retrieval_chunk_id ASC);

CREATE TABLE IF NOT EXISTS research_citation_verification_results (
    verification_run_id TEXT PRIMARY KEY NOT NULL,
    overall_relation TEXT NOT NULL CHECK (overall_relation IN ('supports', 'contradicts', 'contextualizes', 'insufficient')),
    rationale TEXT NOT NULL,
    assessor_provider TEXT NOT NULL,
    assessor_version TEXT NOT NULL,
    assessor_model_id TEXT,
    assessment_contract_version TEXT NOT NULL,
    completed_at_ms INTEGER NOT NULL,
    FOREIGN KEY (verification_run_id) REFERENCES research_citation_verification_runs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS research_citation_verification_evidence (
    verification_run_id TEXT NOT NULL,
    retrieval_chunk_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    claim_evidence_link_id TEXT NOT NULL,
    relation TEXT NOT NULL CHECK (relation IN ('supports', 'contradicts', 'contextualizes', 'insufficient')),
    PRIMARY KEY (verification_run_id, retrieval_chunk_id),
    FOREIGN KEY (verification_run_id, retrieval_chunk_id)
        REFERENCES research_citation_verification_candidates(verification_run_id, retrieval_chunk_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (evidence_id) REFERENCES research_evidence(id) ON DELETE RESTRICT,
    FOREIGN KEY (claim_evidence_link_id) REFERENCES research_claim_evidence(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_citation_verification_evidence_evidence_idx
    ON research_citation_verification_evidence(evidence_id);
