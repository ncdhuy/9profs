ALTER TABLE research_dify_extraction_indexes
    ADD COLUMN metadata_qualified INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS research_dify_metadata_fields (
    dataset_id TEXT NOT NULL,
    field_name TEXT NOT NULL,
    field_id TEXT NOT NULL,
    field_type TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (dataset_id, field_name),
    UNIQUE (dataset_id, field_id),
    FOREIGN KEY (dataset_id) REFERENCES research_dify_case_indexes(dataset_id) ON DELETE RESTRICT
);

