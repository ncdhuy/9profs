CREATE TABLE IF NOT EXISTS research_cases (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS research_sources (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_sources_case_idx
    ON research_sources(research_case_id);

CREATE TABLE IF NOT EXISTS research_source_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    hash_algorithm TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    captured_at_ms INTEGER NOT NULL,
    capture_method TEXT NOT NULL,
    origin_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    UNIQUE (source_id, hash_algorithm, content_hash),
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_source_snapshots_source_idx
    ON research_source_snapshots(source_id);

CREATE TABLE IF NOT EXISTS research_evidence (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    verbatim_excerpt TEXT NOT NULL,
    normalized_text TEXT,
    locator_json TEXT NOT NULL,
    hash_algorithm TEXT NOT NULL,
    excerpt_hash TEXT NOT NULL,
    captured_at_ms INTEGER NOT NULL,
    capture_method TEXT NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS research_evidence_case_idx
    ON research_evidence(research_case_id);

CREATE INDEX IF NOT EXISTS research_evidence_snapshot_idx
    ON research_evidence(source_snapshot_id);

CREATE TABLE IF NOT EXISTS research_claims (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    text TEXT NOT NULL,
    origin_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_claims_case_idx
    ON research_claims(research_case_id);

CREATE TABLE IF NOT EXISTS research_claim_evidence (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    rationale TEXT,
    assessment_method TEXT NOT NULL,
    assessment_metadata_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE CASCADE,
    FOREIGN KEY (claim_id) REFERENCES research_claims(id) ON DELETE CASCADE,
    FOREIGN KEY (evidence_id) REFERENCES research_evidence(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS research_claim_evidence_case_idx
    ON research_claim_evidence(research_case_id);

CREATE INDEX IF NOT EXISTS research_claim_evidence_claim_idx
    ON research_claim_evidence(claim_id);

CREATE INDEX IF NOT EXISTS research_claim_evidence_evidence_idx
    ON research_claim_evidence(evidence_id);
