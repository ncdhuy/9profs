ALTER TABLE research_sources ADD COLUMN identity_json TEXT;

CREATE TABLE research_manuscript_reference_resolution_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    catalog_run_id TEXT NOT NULL,
    catalog_hash_algorithm TEXT NOT NULL,
    catalog_hash TEXT NOT NULL,
    source_state_hash_algorithm TEXT NOT NULL,
    source_state_hash TEXT NOT NULL,
    resolver_policy_version TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    resolved_entry_count INTEGER NOT NULL CHECK (resolved_entry_count >= 0),
    candidate_entry_count INTEGER NOT NULL CHECK (candidate_entry_count >= 0),
    unresolved_entry_count INTEGER NOT NULL CHECK (unresolved_entry_count >= 0),
    conflict_entry_count INTEGER NOT NULL CHECK (conflict_entry_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    failure_code TEXT,
    UNIQUE (
        catalog_run_id,
        catalog_hash_algorithm,
        catalog_hash,
        source_state_hash_algorithm,
        source_state_hash,
        resolver_policy_version
    ),
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (catalog_run_id)
        REFERENCES research_manuscript_reference_catalog_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_reference_resolution_runs_case
    ON research_manuscript_reference_resolution_runs
       (research_case_id, created_at_ms DESC, id DESC);

CREATE TABLE research_manuscript_reference_resolution_entries (
    id TEXT PRIMARY KEY NOT NULL,
    resolution_run_id TEXT NOT NULL,
    reference_entry_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    match_kind TEXT,
    chosen_source_id TEXT,
    chosen_source_snapshot_id TEXT,
    chosen_extraction_id TEXT,
    automatic_binding_permitted INTEGER NOT NULL CHECK (automatic_binding_permitted IN (0, 1)),
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 0),
    UNIQUE (resolution_run_id, reference_entry_id),
    FOREIGN KEY (resolution_run_id)
        REFERENCES research_manuscript_reference_resolution_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (reference_entry_id)
        REFERENCES research_manuscript_reference_entries(id) ON DELETE RESTRICT,
    FOREIGN KEY (chosen_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (chosen_source_snapshot_id)
        REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (chosen_extraction_id)
        REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_reference_resolution_entries_run
    ON research_manuscript_reference_resolution_entries (resolution_run_id, id);

CREATE TABLE research_manuscript_reference_resolution_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    resolution_entry_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_id TEXT NOT NULL,
    source_snapshot_id TEXT,
    extraction_id TEXT,
    match_kind TEXT NOT NULL,
    automatic_binding_permitted INTEGER NOT NULL CHECK (automatic_binding_permitted IN (0, 1)),
    UNIQUE (resolution_entry_id, ordinal),
    FOREIGN KEY (resolution_entry_id)
        REFERENCES research_manuscript_reference_resolution_entries(id) ON DELETE CASCADE,
    FOREIGN KEY (source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id)
        REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id)
        REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_reference_resolution_candidates_entry
    ON research_manuscript_reference_resolution_candidates (resolution_entry_id, ordinal);

