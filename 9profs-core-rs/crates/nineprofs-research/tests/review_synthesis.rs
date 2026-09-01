use std::time::Duration;

use nineprofs_research::{
    AuthorityPackSource, ContentHash, DocumentMapLocator, Finding, FindingEvidence, HashAlgorithm,
    ReviewAuthorityReference, ReviewSynthesisError, ReviewSynthesisExecutor,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn locator(block_id: &str, ordinal: u32) -> DocumentMapLocator {
    DocumentMapLocator {
        document_id: "synthesis-test".to_owned(),
        version: 1,
        block_id: block_id.to_owned(),
        block_ordinal: ordinal,
        docx_index: Some(ordinal),
        section_id: Some("section:test".to_owned()),
    }
}

fn authority(pack_id: &str) -> ReviewAuthorityReference {
    ReviewAuthorityReference::AuthorityPack {
        pack_id: pack_id.to_owned(),
        version: "v1".to_owned(),
        source: AuthorityPackSource {
            manifest_path: format!("{pack_id}/pack.yaml"),
            manifest_hash: ContentHash {
                algorithm: HashAlgorithm::Sha256,
                value: format!("hash-{pack_id}"),
            },
        },
        content_paths: vec![format!("{pack_id}/review.md")],
    }
}

fn finding(id: &str, statement: &str, block_id: &str, authority_ids: &[&str]) -> Finding {
    let locator = locator(block_id, block_id[1..].parse().unwrap());
    Finding {
        id: id.to_owned(),
        task_id: format!("task:{id}"),
        task_kind: "methodology".to_owned(),
        manuscript_locators: vec![locator.clone()],
        statement: statement.to_owned(),
        explanation: "The supplied manuscript evidence supports this concern.".to_owned(),
        evidence: vec![FindingEvidence {
            locator,
            excerpt: format!("excerpt for {id}"),
        }],
        authority_references: authority_ids.iter().map(|id| authority(id)).collect(),
    }
}

fn openai_response(output: Value) -> Vec<u8> {
    let content = serde_json::to_string(&output).unwrap();
    serde_json::to_vec(&json!({
        "choices": [{"message": {"content": content}}]
    }))
    .unwrap()
}

async fn mock_server(response: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 4096];
            let bytes_read = stream.read(&mut chunk).await.unwrap();
            if bytes_read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..bytes_read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let content_length = String::from_utf8_lossy(&request[..header_end])
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(&response).await.unwrap();
    });
    (format!("http://{address}/v1"), server)
}

fn config(base_url: String) -> nineprofs_structured_model::StructuredModelConfig {
    nineprofs_structured_model::StructuredModelConfig {
        provider: "openai".to_owned(),
        model: "mock-model".to_owned(),
        base_url: Some(base_url),
        api_key_env: "NINEPROFS_REVIEW_SYNTHESIS_TEST_KEY".to_owned(),
        timeout: Duration::from_secs(5),
        max_response_bytes: 256 * 1024,
        max_output_tokens: 1_024,
    }
}

async fn synthesize(
    findings: &[Finding],
    output: Value,
) -> Result<nineprofs_research::ReviewSynthesis, ReviewSynthesisError> {
    let (base_url, server) = mock_server(openai_response(output)).await;
    unsafe {
        std::env::set_var("NINEPROFS_REVIEW_SYNTHESIS_TEST_KEY", "test-key");
    }
    let result = ReviewSynthesisExecutor::new(config(base_url))
        .synthesize(findings)
        .await;
    server.await.unwrap();
    result
}

fn group(ids: &[&str], statement: &str, priority_rank: u32) -> Value {
    json!({
        "sourceFindingIds": ids,
        "statement": statement,
        "explanation": "The group members describe one bounded review concern.",
        "priorityRank": priority_rank,
    })
}

#[tokio::test]
async fn duplicate_findings_can_consolidate() {
    let findings = vec![
        finding(
            "finding-a",
            "The period may be inconsistent.",
            "b1",
            &["research.core"],
        ),
        finding(
            "finding-b",
            "The data period may differ.",
            "b2",
            &["research.core"],
        ),
    ];
    let synthesis = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a", "finding-b"], "The study period may be inconsistent.", 1)]}),
    )
    .await
    .unwrap();

    assert_eq!(synthesis.findings.len(), 1);
    assert_eq!(
        synthesis.findings[0].source_finding_ids,
        vec!["finding-a", "finding-b"]
    );
}

#[tokio::test]
async fn distinct_findings_remain_separate_and_ordered() {
    let findings = vec![
        finding("finding-a", "A method issue.", "b1", &["research.core"]),
        finding("finding-b", "A result issue.", "b2", &["research.core"]),
    ];
    let synthesis = synthesize(
        &findings,
        json!({
            "groups": [
                group(&["finding-a"], "A method issue.", 2),
                group(&["finding-b"], "A result issue.", 1)
            ]
        }),
    )
    .await
    .unwrap();

    assert_eq!(synthesis.findings.len(), 2);
    assert_eq!(synthesis.findings[0].source_finding_ids, vec!["finding-b"]);
    assert_eq!(synthesis.findings[1].source_finding_ids, vec!["finding-a"]);
}

#[tokio::test]
async fn consolidated_result_retains_all_source_finding_ids() {
    let findings = vec![
        finding("finding-a", "Issue A.", "b1", &["research.core"]),
        finding("finding-b", "Issue B.", "b2", &["research.core"]),
        finding("finding-c", "Issue C.", "b3", &["research.core"]),
    ];
    let synthesis = synthesize(
        &findings,
        json!({
            "groups": [
                group(&["finding-a", "finding-b"], "Issues A and B overlap.", 1),
                group(&["finding-c"], "Issue C.", 2)
            ]
        }),
    )
    .await
    .unwrap();

    let ids = synthesis
        .findings
        .iter()
        .flat_map(|finding| finding.source_finding_ids.iter().cloned())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["finding-a", "finding-b", "finding-c"]);
}

#[tokio::test]
async fn unknown_finding_id_is_rejected() {
    let findings = vec![finding("finding-a", "Issue A.", "b1", &["research.core"])];
    let error = synthesize(
        &findings,
        json!({"groups": [group(&["unknown"], "Unknown issue.", 1)]}),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("unknown source finding ID"));
}

#[tokio::test]
async fn invented_locator_cannot_enter_synthesized_provenance() {
    let findings = vec![finding("finding-a", "Issue A.", "b1", &["research.core"])];
    let mut output = group(&["finding-a"], "Issue A.", 1);
    output["manuscriptLocators"] = json!([{"blockId": "invented"}]);
    let error = synthesize(&findings, json!({"groups": [output]}))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[tokio::test]
async fn invented_authority_cannot_enter_synthesized_provenance() {
    let findings = vec![finding("finding-a", "Issue A.", "b1", &["research.core"])];
    let mut output = group(&["finding-a"], "Issue A.", 1);
    output["authorityReferences"] = json!([{"kind": "authority_pack", "pack_id": "invented"}]);
    let error = synthesize(&findings, json!({"groups": [output]}))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[tokio::test]
async fn multiple_source_authorities_are_preserved_when_findings_merge() {
    let findings = vec![
        finding("finding-a", "Issue A.", "b1", &["research.core"]),
        finding("finding-b", "Issue B.", "b2", &["domain.med"]),
    ];
    let synthesis = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a", "finding-b"], "One issue.", 1)]}),
    )
    .await
    .unwrap();

    let authorities = &synthesis.findings[0].authority_references;
    assert_eq!(authorities.len(), 2);
    assert!(authorities.contains(&authority("research.core")));
    assert!(authorities.contains(&authority("domain.med")));
}

#[tokio::test]
async fn multiple_source_locators_are_preserved_when_findings_merge() {
    let findings = vec![
        finding("finding-a", "Issue A.", "b1", &["research.core"]),
        finding("finding-b", "Issue B.", "b2", &["research.core"]),
    ];
    let synthesis = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a", "finding-b"], "One issue.", 1)]}),
    )
    .await
    .unwrap();

    assert_eq!(
        synthesis.findings[0]
            .manuscript_locators
            .iter()
            .map(|locator| locator.block_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b1", "b2"]
    );
}

#[tokio::test]
async fn empty_synthesized_statement_is_rejected() {
    let findings = vec![finding("finding-a", "Issue A.", "b1", &["research.core"])];
    let error = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a"], "   ", 1)]}),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("synthesized statement is empty"));
}

#[tokio::test]
async fn model_cannot_silently_omit_a_finding() {
    let findings = vec![
        finding("finding-a", "Issue A.", "b1", &["research.core"]),
        finding("finding-b", "Issue B.", "b2", &["research.core"]),
    ];
    let error = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a"], "Issue A.", 1)]}),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("source findings omitted"));
}

#[tokio::test]
async fn model_cannot_assign_one_finding_to_multiple_groups() {
    let findings = vec![
        finding("finding-a", "Issue A.", "b1", &["research.core"]),
        finding("finding-b", "Issue B.", "b2", &["research.core"]),
    ];
    let error = synthesize(
        &findings,
        json!({
            "groups": [
                group(&["finding-a"], "Issue A.", 1),
                group(&["finding-a", "finding-b"], "Issues A and B.", 2)
            ]
        }),
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("source finding assigned more than once")
    );
}

#[tokio::test]
async fn uncertain_source_wording_requires_uncertain_synthesized_wording() {
    let findings = vec![finding(
        "finding-a",
        "The study period may be inconsistent.",
        "b1",
        &["research.core"],
    )];
    let error = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a"], "The study period is inconsistent.", 1)]}),
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("strengthens uncertain source wording")
    );
}

#[tokio::test]
async fn evidence_bound_paraphrase_is_accepted_for_uncertain_source() {
    let findings = vec![finding(
        "finding-a",
        "The study period may be inconsistent.",
        "b1",
        &["research.core"],
    )];
    let synthesis = synthesize(
        &findings,
        json!({
            "groups": [group(
                &["finding-a"],
                "The reported evidence does not fully establish the study period.",
                1
            )]
        }),
    )
    .await
    .unwrap();

    assert_eq!(synthesis.findings.len(), 1);
}

#[tokio::test]
async fn uncertain_source_provenance_remains_bounded() {
    let findings = vec![finding(
        "finding-a",
        "The study period may be inconsistent.",
        "b1",
        &["research.core"],
    )];
    let synthesis = synthesize(
        &findings,
        json!({"groups": [group(&["finding-a"], "The study period may be inconsistent.", 1)]}),
    )
    .await
    .unwrap();

    assert_eq!(synthesis.findings[0].source_finding_ids, vec!["finding-a"]);
    assert_eq!(
        synthesis.findings[0].manuscript_locators,
        findings[0].manuscript_locators
    );
    assert_eq!(
        synthesis.findings[0].authority_references,
        findings[0].authority_references
    );
}

#[tokio::test]
async fn zero_findings_produces_empty_synthesis_without_model_call() {
    let config = nineprofs_structured_model::StructuredModelConfig {
        provider: String::new(),
        model: String::new(),
        base_url: None,
        api_key_env: "missing".to_owned(),
        timeout: Duration::from_secs(1),
        max_response_bytes: 1024,
        max_output_tokens: 1,
    };

    let synthesis = ReviewSynthesisExecutor::new(config)
        .synthesize(&[])
        .await
        .unwrap();
    assert!(synthesis.findings.is_empty());
}
