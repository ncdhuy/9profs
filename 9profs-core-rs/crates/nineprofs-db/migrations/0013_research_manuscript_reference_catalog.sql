CREATE TABLE research_manuscript_reference_catalog_runs (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL,
    manuscript_source_id TEXT NOT NULL,
    citation_sync_run_id TEXT NOT NULL UNIQUE,
    document_id TEXT NOT NULL,
    document_version INTEGER NOT NULL CHECK (document_version >= 0),
    catalog_hash_algorithm TEXT NOT NULL,
    catalog_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    target_mapping_count INTEGER NOT NULL CHECK (target_mapping_count >= 0),
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    failure_code TEXT,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (manuscript_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_sync_run_id)
        REFERENCES research_manuscript_citation_sync_runs(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_reference_catalog_runs_case
    ON research_manuscript_reference_catalog_runs
       (research_case_id, created_at_ms DESC, id DESC);

CREATE INDEX idx_research_reference_catalog_runs_source
    ON research_manuscript_reference_catalog_runs
       (manuscript_source_id, document_version DESC, created_at_ms DESC, id DESC);

CREATE TABLE research_manuscript_reference_entries (
    id TEXT PRIMARY KEY NOT NULL,
    catalog_run_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    format TEXT NOT NULL CHECK (format IN ('word_native', 'zotero')),
    reference_key TEXT NOT NULL,
    descriptor_hash_algorithm TEXT NOT NULL,
    descriptor_hash TEXT NOT NULL,
    word_tag TEXT,
    word_title TEXT,
    word_author TEXT,
    word_year TEXT,
    zotero_item_id TEXT,
    zotero_uris_json TEXT NOT NULL,
    target_count INTEGER NOT NULL CHECK (target_count >= 0),
    UNIQUE (catalog_run_id, ordinal),
    UNIQUE (catalog_run_id, format, reference_key),
    FOREIGN KEY (catalog_run_id)
        REFERENCES research_manuscript_reference_catalog_runs(id) ON DELETE CASCADE
);

CREATE INDEX idx_research_reference_entries_run
    ON research_manuscript_reference_entries (catalog_run_id, ordinal);

CREATE INDEX idx_research_reference_entries_identity
    ON research_manuscript_reference_entries (format, reference_key);

CREATE TABLE research_manuscript_reference_target_mappings (
    id TEXT PRIMARY KEY NOT NULL,
    catalog_run_id TEXT NOT NULL,
    reference_entry_id TEXT NOT NULL,
    citation_occurrence_id TEXT NOT NULL,
    citation_target_id TEXT NOT NULL,
    document_target_ordinal INTEGER NOT NULL CHECK (document_target_ordinal >= 0),
    UNIQUE (catalog_run_id, citation_target_id),
    UNIQUE (catalog_run_id, citation_occurrence_id, document_target_ordinal),
    FOREIGN KEY (catalog_run_id)
        REFERENCES research_manuscript_reference_catalog_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (reference_entry_id)
        REFERENCES research_manuscript_reference_entries(id) ON DELETE CASCADE,
    FOREIGN KEY (citation_occurrence_id)
        REFERENCES research_citation_occurrences(id) ON DELETE RESTRICT,
    FOREIGN KEY (citation_target_id)
        REFERENCES research_citation_targets(id) ON DELETE RESTRICT
);

CREATE INDEX idx_research_reference_mappings_run
    ON research_manuscript_reference_target_mappings
       (catalog_run_id, citation_occurrence_id, document_target_ordinal);

CREATE INDEX idx_research_reference_mappings_entry
    ON research_manuscript_reference_target_mappings
       (reference_entry_id, document_target_ordinal, id);

CREATE INDEX idx_research_reference_mappings_target
    ON research_manuscript_reference_target_mappings (citation_target_id);
