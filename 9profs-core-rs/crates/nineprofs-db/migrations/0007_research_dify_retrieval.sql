CREATE TABLE IF NOT EXISTS research_dify_case_indexes (
    id TEXT PRIMARY KEY NOT NULL,
    research_case_id TEXT NOT NULL UNIQUE,
    dataset_id TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS research_dify_extraction_indexes (
    id TEXT PRIMARY KEY NOT NULL,
    case_index_id TEXT NOT NULL,
    research_case_id TEXT NOT NULL,
    extraction_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    document_id TEXT,
    chunker_version TEXT NOT NULL,
    status TEXT NOT NULL,
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (case_index_id) REFERENCES research_dify_case_indexes(id) ON DELETE RESTRICT,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    UNIQUE (case_index_id, extraction_id, chunker_version)
);

CREATE INDEX IF NOT EXISTS research_dify_extraction_indexes_case_idx
    ON research_dify_extraction_indexes(research_case_id, updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS research_retrieval_chunks (
    id TEXT PRIMARY KEY NOT NULL,
    extraction_index_id TEXT NOT NULL,
    research_case_id TEXT NOT NULL,
    research_source_id TEXT NOT NULL,
    source_snapshot_id TEXT NOT NULL,
    extraction_id TEXT NOT NULL,
    page INTEGER NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    text TEXT NOT NULL,
    hash_algorithm TEXT NOT NULL,
    text_hash TEXT NOT NULL,
    FOREIGN KEY (extraction_index_id) REFERENCES research_dify_extraction_indexes(id) ON DELETE RESTRICT,
    FOREIGN KEY (research_case_id) REFERENCES research_cases(id) ON DELETE RESTRICT,
    FOREIGN KEY (research_source_id) REFERENCES research_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_snapshot_id) REFERENCES research_source_snapshots(id) ON DELETE RESTRICT,
    FOREIGN KEY (extraction_id) REFERENCES research_pdf_extractions(id) ON DELETE RESTRICT,
    UNIQUE (extraction_index_id, page, start_offset, end_offset)
);

CREATE INDEX IF NOT EXISTS research_retrieval_chunks_index_idx
    ON research_retrieval_chunks(extraction_index_id, page, start_offset);

CREATE TABLE IF NOT EXISTS research_dify_segment_mappings (
    dataset_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    segment_id TEXT NOT NULL,
    retrieval_chunk_id TEXT NOT NULL UNIQUE,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (dataset_id, segment_id),
    FOREIGN KEY (retrieval_chunk_id) REFERENCES research_retrieval_chunks(id) ON DELETE RESTRICT
);
