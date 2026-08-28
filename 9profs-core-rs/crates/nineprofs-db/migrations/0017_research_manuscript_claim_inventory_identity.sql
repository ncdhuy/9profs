CREATE UNIQUE INDEX idx_research_manuscript_claim_inventory_runs_completed_identity
    ON research_manuscript_claim_inventory_runs (
        research_case_id,
        manuscript_source_id,
        document_id,
        document_version,
        document_context_hash_algorithm,
        document_context_hash,
        extractor_provider,
        extractor_version,
        COALESCE(extractor_model_id, ''),
        extraction_contract_version,
        coverage_contract_version
    )
    WHERE status = 'completed';
