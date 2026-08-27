CREATE TABLE research_manuscript_citation_sync_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    inventory_hash_algorithm TEXT NOT NULL,
    inventory_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    occurrence_count INTEGER NOT NULL CHECK (occurrence_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    failure_code TEXT,
    UNIQUE (research_case_id, manuscript_source_id, document_id, document_version),
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_manuscript_citation_sync_runs_case
    ON research_manuscript_citation_sync_runs (research_case_id, created_at_ms DESC, id DESC);

CREATE INDEX idx_research_manuscript_citation_sync_runs_source
    ON research_manuscript_citation_sync_runs
       (manuscript_source_id, document_version DESC, created_at_ms DESC, id DESC);

CREATE TABLE research_manuscript_citation_sync_occurrences (
    id TEXT PRIMARY KEY NOT NULL,
    sync_run_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    citation_occurrence_id TEXT NOT NULL,
    document_block_id TEXT NOT NULL,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL CHECK (start < end),
    format TEXT NOT NULL CHECK (format IN ('word_native', 'zotero')),
    UNIQUE (sync_run_id, ordinal),
    FOREIGN KEY (sync_run_id) REFERENCES research_manuscript_citation_sync_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (citation_occurrence_id) REFERENCES research_citation_occurrences(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_citation_sync_occurrences_run
    ON research_manuscript_citation_sync_occurrences (sync_run_id, ordinal);

CREATE TABLE research_manuscript_citation_sync_targets (
    id TEXT PRIMARY KEY NOT NULL,
    sync_occurrence_id TEXT NOT NULL,
    document_target_ordinal INTEGER NOT NULL CHECK (document_target_ordinal >= 0),
    citation_target_id TEXT NOT NULL,
    UNIQUE (sync_occurrence_id, document_target_ordinal),
    FOREIGN KEY (sync_occurrence_id)
        REFERENCES research_manuscript_citation_sync_occurrences(id) ON DELETE CASCADE,
    FOREIGN KEY (citation_target_id) REFERENCES research_citation_targets(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_manuscript_citation_sync_targets_occurrence
    ON research_manuscript_citation_sync_targets (sync_occurrence_id, document_target_ordinal);
