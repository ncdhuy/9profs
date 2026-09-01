use std::sync::Arc;

use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    ConsolidatedFinding, DOCUMENT_MAP_CONTRACT_VERSION, DocumentMap, DocumentMapLocator,
    ManuscriptReviewResult, ManuscriptReviewSummary, ResearchContext, ResearchService,
    SqliteResearchRepository,
};

fn empty_map() -> DocumentMap {
    DocumentMap {
        contract_version: DOCUMENT_MAP_CONTRACT_VERSION.to_owned(),
        document_id: "doc-orchestration".to_owned(),
        version: 11,
        sections: vec![],
        blocks: vec![],
        tables: vec![],
        figures: vec![],
        citations: vec![],
        references: vec![],
    }
}

#[tokio::test]
async fn orchestrator_composes_empty_pipeline_and_preserves_document_version() {
    let database = Database::in_memory().await.unwrap();
    let service = ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(8)),
    );

    let result = service
        .run_manuscript_review(&empty_map(), &ResearchContext::default(), 1)
        .await
        .unwrap();

    assert_eq!(result.document_id, "doc-orchestration");
    assert_eq!(result.document_version, 11);
    assert!(result.synthesized_findings.is_empty());
    assert_eq!(result.summary.task_count, 0);
    assert_eq!(result.summary.consolidated_finding_count, 0);
}

#[test]
fn result_serialization_preserves_synthesized_finding_provenance() {
    let locator = DocumentMapLocator {
        document_id: "doc-orchestration".to_owned(),
        version: 11,
        block_id: "b7".to_owned(),
        block_ordinal: 3,
        docx_index: Some(7),
        section_id: Some("section:methods".to_owned()),
    };
    let result = ManuscriptReviewResult {
        document_id: "doc-orchestration".to_owned(),
        document_version: 11,
        synthesized_findings: vec![ConsolidatedFinding {
            id: "consolidated-1".to_owned(),
            source_finding_ids: vec!["finding-a".to_owned(), "finding-b".to_owned()],
            statement: "The outcome is unclear.".to_owned(),
            explanation: "The manuscript uses two definitions.".to_owned(),
            manuscript_locators: vec![locator.clone()],
            evidence: vec![],
            authority_references: vec![],
            priority_rank: 1,
        }],
        summary: ManuscriptReviewSummary {
            consolidated_finding_count: 1,
            ..ManuscriptReviewSummary::default()
        },
    };

    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["documentId"], "doc-orchestration");
    assert_eq!(json["documentVersion"], 11);
    assert_eq!(json["synthesizedFindings"][0]["id"], "consolidated-1");
    assert_eq!(
        json["synthesizedFindings"][0]["sourceFindingIds"],
        serde_json::json!(["finding-a", "finding-b"])
    );
    assert_eq!(json["synthesizedFindings"][0]["manuscriptLocators"][0]["blockId"], "b7");
    assert_eq!(json["synthesizedFindings"][0]["priorityRank"], 1);
}
