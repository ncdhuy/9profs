use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt;
use nineprofs_agent::{AgentBackendDescriptor, AgentRunContext, AgentTaskId, RunId, TaskState};
use nineprofs_api_types::*;
use nineprofs_assistant::{Assistant, AssistantError, CreateAssistant, UpdateAssistant};
use nineprofs_document_tools::{
    DocumentProposalView, ProposalAvailability, ProposalFreshness, ProposalStoreError,
    ProposalWorkflowError,
};
use nineprofs_documents::ActiveDocumentDescriptor;
use nineprofs_mcp::{
    CreateMcpServer, McpError, McpServerSnapshot, McpTransportConfig, McpTransportSummary,
    UpdateMcpServer,
};
use nineprofs_officecli::OfficeCliStatus;
use nineprofs_research::*;
use nineprofs_research_dify::*;
use nineprofs_research_verification::{
    CitationVerificationError, CitationVerificationRun, CreateCitationVerification,
};
use nineprofs_runtime::{AgentExecutionServiceError, CoreRuntime};
use nineprofs_skills::{Skill, SkillSource};
use tower::ServiceExt;

use super::agents::{agent_run_context_dto, task_dto};
use super::assistants::assistant_dto;
use super::documents::active_document_dto;
use super::mcp::{mcp_server_dto, mcp_transport_config};
use super::proposals::{TRUSTED_DECISION_HEADER, document_proposal_dto};
use super::research::cases::research_case_dto;
use super::research::citations::{
    citation_occurrence_dto, citation_target_dto, claim_citation_link_dto,
};
use super::research::claims::{claim_evidence_link_dto, research_claim_dto};
use super::research::common::{header_text, research_content_hash_dto, safe_upload_label};
use super::research::evidence::{evidence_locator, evidence_locator_dto, research_evidence_dto};
use super::research::manuscript::{
    manuscript_citation_sync_format_dto, manuscript_citation_sync_occurrence_dto,
    manuscript_citation_sync_run_dto, manuscript_citation_sync_status_dto,
    manuscript_citation_sync_target_dto, manuscript_claim_extraction_coverage_dto,
    manuscript_claim_extraction_item_dto, manuscript_claim_extraction_run_dto,
    manuscript_claim_extraction_status_dto, manuscript_reference_catalog_citation,
    manuscript_reference_catalog_run_dto, manuscript_reference_catalog_status_dto,
    manuscript_reference_catalog_target, manuscript_reference_entry_dto,
    manuscript_reference_target_mapping_dto,
};
use super::research::pdf::{
    pdf_extraction_status, pdf_extraction_status_dto, research_artifact_dto,
    research_pdf_extraction_dto, research_pdf_page_dto, research_pdf_page_list_dto,
};
use super::research::retrieval::{
    dify_index_status_dto, research_extraction_retrieval_index_dto,
    research_retrieval_candidate_dto, research_retrieval_index_dto,
    research_retrieval_index_state_dto, research_retrieval_readiness_dto, research_retrieval_scope,
};
use super::research::sources::{
    capture_method, capture_method_dto, research_snapshot_dto, research_source_dto, source_kind,
    source_kind_dto, source_origin, source_origin_dto,
};
use super::research::verification::{
    citation_verification_candidate_dto, citation_verification_evidence_dto,
    citation_verification_result_dto, citation_verification_run_dto,
    citation_verification_status_dto,
};
use super::{ApiError, AppState, build_router};
#[cfg(test)]
mod agent_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn agent_list_get_and_unknown_backend_api_work() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);

        let response = router
            .clone()
            .oneshot(Request::get("/api/agents").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["success"], true);
        assert!(
            payload["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|agent| { agent["id"] == "codex" && agent["availability"] == "unknown" })
        );

        let response = router
            .clone()
            .oneshot(
                Request::get("/api/agents/codex")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"]["id"], "codex");

        let response = router
            .oneshot(
                Request::get("/api/agents/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["code"], "not_found");
        assert_eq!(payload["error"], "agent backend not found: does-not-exist");
    }

    #[tokio::test]
    async fn document_agent_profile_is_global_and_secret_free() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);
        let response = router
            .oneshot(
                Request::get("/api/document-agent-profile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"]["defaultAssistantId"], "document-foundation");
        assert_eq!(payload["data"]["backendId"], "nineprofs-default");
        assert!(
            payload["data"]["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|capability| capability == "document.list_active")
        );
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains("api_key"));
        assert!(!serialized.contains("credential"));
        assert!(!serialized.contains("session_secret"));
    }

    #[tokio::test]
    async fn docs_conversation_api_creates_bound_safe_metadata() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        runtime
            .agent_registry()
            .set_availability(
                "nineprofs-default",
                nineprofs_agent::AvailabilityState::Available,
                None,
            )
            .await
            .unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        runtime
            .document_bridge()
            .register(
                nineprofs_documents::DocumentRegistration {
                    protocol_version: nineprofs_documents::DOCUMENT_BRIDGE_PROTOCOL_VERSION
                        .to_owned(),
                    document_id: "doc-conversation".to_owned(),
                    document_type: nineprofs_documents::DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 1,
                    capabilities: vec![
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let router = build_router(runtime);
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-agent-conversations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "assistant_id": "document-foundation",
                            "document_id": "doc-conversation"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"]["assistantId"], "document-foundation");
        assert_eq!(payload["data"]["documentId"], "doc-conversation");
        assert_eq!(payload["data"]["state"], "idle");
        let conversation_id = payload["data"]["conversationId"].as_str().unwrap();
        assert!(conversation_id.starts_with("docs-"));
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains("backend"));
        assert!(!serialized.contains("credential"));

        let response = router
            .oneshot(
                Request::get(format!(
                    "/api/document-agent-conversations/{conversation_id}"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn active_docs_agent_run_rejects_unavailable_and_unsupported_documents() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        runtime
            .document_bridge()
            .register(
                nineprofs_documents::DocumentRegistration {
                    protocol_version: nineprofs_documents::DOCUMENT_BRIDGE_PROTOCOL_VERSION
                        .to_owned(),
                    document_id: "pdf-a".to_owned(),
                    document_type: "pdf".to_owned(),
                    version: 1,
                    capabilities: vec![
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let router = build_router(runtime);

        for (document_id, code) in [
            ("missing", "active_document_unavailable"),
            ("pdf-a", "active_document_unsupported"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::post("/api/document-agent-runs")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "assistant_id": "execution-assistant",
                                "document_id": document_id,
                                "input": "inspect document"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["code"], code);
        }
    }

    #[tokio::test]
    async fn active_docs_agent_run_requires_configured_session_secret() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("run-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(runtime);
        let body = serde_json::json!({
            "assistant_id": "execution-assistant",
            "document_id": "missing",
            "input": "inspect document"
        })
        .to_string();

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-agent-runs")
                    .header("content-type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .oneshot(
                Request::post("/api/document-agent-runs")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "run-test-secret")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[cfg(test)]
mod mcp_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn mcp_crud_redacts_secrets_and_validates_transport() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);
        let request = Request::post("/api/mcp/servers")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "id": "api-fixture",
                    "name": "API fixture",
                    "enabled": false,
                    "transport": {
                        "type": "stdio",
                        "command": "fixture",
                        "env": {"TOKEN": "never-return-this"}
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("never-return-this"));
        assert!(text.contains("TOKEN"));

        let response = router
            .clone()
            .oneshot(
                Request::put("/api/mcp/servers/api-fixture")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"description":"updated"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/mcp/servers/api-fixture/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .clone()
            .oneshot(
                Request::delete("/api/mcp/servers/api-fixture")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .oneshot(
                Request::post("/api/mcp/servers")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"invalid","transport":{"type":"sse","url":"file:///tmp"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[cfg(test)]
mod officecli_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn officecli_status_is_safe_and_core_starts_without_sidecar() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);
        let response = router
            .oneshot(
                Request::get("/api/officecli/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["success"], true);
        assert!(payload["data"]["supported_version"] == "1.0.144");
        assert!(payload["data"].get("binary_path").is_none());
        assert!(payload["data"].get("profile_root").is_none());
    }
}

#[cfg(test)]
mod document_proposal_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use nineprofs_documents::{
        DOCUMENT_BRIDGE_CAPABILITY_COMMIT, DOCUMENT_BRIDGE_CAPABILITY_INSPECT,
        DOCUMENT_BRIDGE_PROTOCOL_VERSION, DOCX_DOCUMENT_TYPE, DocumentRegistration,
    };
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn document_metadata_and_proposal_apis_are_read_only_and_safe() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let (sender, _receiver) = mpsc::channel(8);
        runtime
            .document_bridge()
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "api-doc".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 5,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let router = build_router(runtime);

        let response = router
            .clone()
            .oneshot(Request::get("/api/documents").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("api-doc"));
        assert!(!text.contains("sessionId"));

        let response = router
            .clone()
            .oneshot(
                Request::get("/api/document-proposals?documentId=api-doc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"], serde_json::json!([]));

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-proposals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let response = router
            .oneshot(
                Request::get("/api/document-proposals/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn trusted_decision_endpoints_require_session_secret_without_echoing_it() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("approval-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(runtime);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-proposals/missing/reject")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-proposals/missing/reject")
                    .header(TRUSTED_DECISION_HEADER, "wrong-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .oneshot(
                Request::post("/api/document-proposals/missing/reject")
                    .header(TRUSTED_DECISION_HEADER, "approval-test-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(
            !String::from_utf8(body.to_vec())
                .unwrap()
                .contains("approval-test-secret")
        );
    }
}

#[cfg(test)]
mod research_pdf_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn reference_pdf_api_streams_authenticates_and_derives_exact_evidence() {
        let root = std::env::temp_dir().join(format!(
            "9profs-core-reference-pdf-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.data_dir = root.clone();
        config.database_path = root.join("core.db");
        config.session_secret = Some(Arc::from("api-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let case = runtime
            .research_service()
            .create_case(CreateResearchCase {
                title: "Reference PDF API".to_owned(),
            })
            .await
            .unwrap();
        let router = build_router(runtime);
        let pdf = b"%PDF-1.7\nfixture".to_vec();
        let upload_path = format!("/api/research/cases/{}/reference-pdfs", case.id.as_str());

        let response = router
            .clone()
            .oneshot(
                Request::post(&upload_path)
                    .header("content-type", "application/pdf")
                    .body(Body::from(pdf.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post(&upload_path)
                    .header("content-type", "application/pdf")
                    .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                    .header(
                        "x-nineprofs-original-filename",
                        r"C:\\imports\\reference.pdf",
                    )
                    .header("x-nineprofs-source-label", "Reference PDF")
                    .body(Body::from(pdf))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(body["data"]["artifact"]["artifactId"].is_string());
        assert!(body["data"]["snapshot"]["contentHash"]["value"].is_string());
        assert!(body.get("path").is_none());
        assert!(!body.to_string().contains("C:\\\\imports"));
        let snapshot_id = body["data"]["snapshot"]["snapshotId"].as_str().unwrap();

        let page_text = "Điều trị giảm tử vong 😀 20%.";
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                .body(Body::from(
                    serde_json::json!({
                        "extractor": "pdfjs",
                        "extractorVersion": "api-test",
                        "pageCount": 1,
                        "status": "ready",
                        "pages": [{ "page": 1, "text": page_text }]
                    })
                    .to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let extraction_id = body["data"]["extractionId"].as_str().unwrap();

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/pdf-extractions/{extraction_id}/pages/1"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(page["data"]["text"], page_text);

        let start = page_text[..page_text.find("giảm tử vong").unwrap()]
            .chars()
            .count() as u64;
        let end = start + "giảm tử vong".chars().count() as u64;
        let response = router
            .oneshot(
                Request::post("/api/research/pdf-evidence")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                    .body(Body::from(
                        serde_json::json!({
                            "researchCaseId": case.id.as_str(),
                            "sourceSnapshotId": snapshot_id,
                            "extractionId": extraction_id,
                            "page": 1,
                            "start": start,
                            "end": end
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let evidence: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(evidence["data"]["verbatimExcerpt"], "giảm tử vong");
        assert_eq!(evidence["data"]["locator"]["kind"], "pdf_text_range");
        assert_eq!(evidence["data"]["pdfExtractionId"], extraction_id);
        assert_eq!(evidence["data"]["sourceSnapshotId"], snapshot_id);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn pdf_page_api_paginates_one_extraction_without_gaps() {
        let root = std::env::temp_dir().join(format!(
            "9profs-core-pdf-pagination-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.data_dir = root.clone();
        config.database_path = root.join("core.db");
        config.session_secret = Some(Arc::from("api-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let case = runtime
            .research_service()
            .create_case(CreateResearchCase {
                title: "PDF pagination API".to_owned(),
            })
            .await
            .unwrap();
        let router = build_router(runtime);
        let upload_path = format!("/api/research/cases/{}/reference-pdfs", case.id.as_str());
        let response = router
            .clone()
            .oneshot(
                Request::post(&upload_path)
                    .header("content-type", "application/pdf")
                    .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                    .body(Body::from(b"%PDF-1.7\npagination fixture".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let upload: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let snapshot_id = upload["data"]["snapshot"]["snapshotId"]
            .as_str()
            .unwrap()
            .to_owned();

        let pages: Vec<_> = (1..=120)
            .map(|page| serde_json::json!({ "page": page, "text": format!("page {page}") }))
            .collect();
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                .body(Body::from(
                    serde_json::json!({
                        "extractor": "pdfjs",
                        "extractorVersion": "pagination-test",
                        "pageCount": 120,
                        "status": "ready",
                        "pages": pages
                    })
                    .to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let extraction: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let extraction_id = extraction["data"]["extractionId"]
            .as_str()
            .unwrap()
            .to_owned();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                .body(Body::from(
                    serde_json::json!({
                        "extractor": "pdfjs",
                        "extractorVersion": "pagination-test-newer",
                        "pageCount": 1,
                        "status": "ready",
                        "pages": [{ "page": 1, "text": "newer revision" }]
                    })
                    .to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let newer_extraction: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let newer_extraction_id = newer_extraction["data"]["extractionId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(newer_extraction_id, extraction_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!("/api/research/pdf-extractions/{extraction_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let exact: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(exact["data"]["extractionId"], extraction_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/source-snapshots/{snapshot_id}/pdf-extractions"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let listed: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(listed["data"].as_array().unwrap().len(), 2);
        assert_eq!(listed["data"][0]["extractionId"], extraction_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let latest: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(latest["data"]["extractionId"], newer_extraction_id);

        let mut all_pages = Vec::new();
        for (start_page, expected_end, has_more, next_start_page) in [
            (1, 50, true, Some(51)),
            (51, 100, true, Some(101)),
            (101, 120, false, None),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::get(format!(
                        "/api/research/pdf-extractions/{extraction_id}/pages?startPage={start_page}&limit=50"
                    ))
                    .body(Body::empty())
                    .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let payload: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            assert_eq!(payload["data"]["startPage"], start_page);
            assert_eq!(payload["data"]["limit"], 50);
            assert_eq!(payload["data"]["hasMore"], has_more);
            if let Some(next_start_page) = next_start_page {
                assert_eq!(payload["data"]["nextStartPage"], next_start_page);
            } else {
                assert!(payload["data"]["nextStartPage"].is_null());
            }
            let response_pages = payload["data"]["data"].as_array().unwrap();
            assert_eq!(response_pages.first().unwrap()["page"], start_page);
            assert_eq!(response_pages.last().unwrap()["page"], expected_end);
            all_pages.extend(
                response_pages
                    .iter()
                    .map(|page| page["page"].as_u64().unwrap() as u32),
            );
        }
        assert_eq!(all_pages, (1..=120).collect::<Vec<_>>());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod research_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn research_writes_use_launch_secret_and_reads_are_safe() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("research-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(Arc::clone(&runtime));

        let request = Request::post("/api/research/cases")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"title":"Secure review"}"#))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let request = Request::post("/api/research/cases")
            .header("content-type", "application/json")
            .header(TRUSTED_DECISION_HEADER, "research-secret")
            .body(Body::from(r#"{"title":"Secure review"}"#))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let case = json_body(response).await;
        let case_id = case["data"]["caseId"].as_str().unwrap().to_owned();

        let response = router
            .oneshot(
                Request::get("/api/research/cases")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert_eq!(payload["data"][0]["caseId"], case_id);
        assert_eq!(payload["data"][0]["title"], "Secure review");
    }

    #[tokio::test]
    async fn citation_routes_require_launch_secret_and_round_trip_targets() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("citation-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(Arc::clone(&runtime));

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/citation-verifications")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"claimCitationLinkId":"claim-link","citationTargetBindingId":"binding"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/cases")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "citation-secret")
                    .body(Body::from(r#"{"title":"Citation API review"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let case_id = json_body(response).await["data"]["caseId"]
            .as_str()
            .unwrap()
            .to_owned();

        let occurrence_body = serde_json::json!({
            "researchCaseId": case_id,
            "origin": {"kind": "imported", "source": "citation-api-test"},
            "renderedText": "[12, 13]"
        });
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/citation-occurrences")
                    .header("content-type", "application/json")
                    .body(Body::from(occurrence_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/citation-occurrences")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "citation-secret")
                    .body(Body::from(occurrence_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let occurrence_status = response.status();
        let occurrence_payload = json_body(response).await;
        assert_eq!(
            occurrence_status,
            StatusCode::OK,
            "unexpected occurrence HTTP response: {occurrence_payload}"
        );
        assert!(
            occurrence_payload["data"]["occurrenceId"].is_string(),
            "unexpected occurrence response: {occurrence_payload}"
        );
        let occurrence_id = occurrence_payload["data"]["occurrenceId"]
            .as_str()
            .unwrap()
            .to_owned();

        let target_body = serde_json::json!({
            "citationOccurrenceId": occurrence_id,
            "ordinal": 0,
            "referenceKey": "12",
            "citedLocator": "p. 42"
        });
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/citation-occurrences/{occurrence_id}/targets"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "citation-secret")
                .body(Body::from(target_body.to_string()))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let target_payload = json_body(response).await;
        assert_eq!(target_payload["data"]["referenceKey"], "12");
        assert_eq!(target_payload["data"]["resolution"], "unresolved");

        let response = router
            .oneshot(
                Request::get(format!(
                    "/api/research/citation-occurrences/{occurrence_id}/targets"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let targets = json_body(response).await;
        assert_eq!(targets["data"].as_array().unwrap().len(), 1);
        assert_eq!(targets["data"][0]["referenceKey"], "12");
    }

    #[tokio::test]
    async fn manuscript_citation_sync_routes_are_trusted_idempotent_and_readable() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("sync-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(Arc::clone(&runtime));

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/cases")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(r#"{"title":"Sync API review"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let case_id = json_body(response).await["data"]["caseId"]
            .as_str()
            .unwrap()
            .to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/sources")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(
                        serde_json::json!({
                            "researchCaseId": case_id,
                            "kind": "manuscript",
                            "label": "Draft"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let source_id = json_body(response).await["data"]["sourceId"]
            .as_str()
            .unwrap()
            .to_owned();

        let sync_path =
            format!("/api/research/cases/{case_id}/manuscripts/{source_id}/citations/sync");
        let sync_body = serde_json::json!({
            "documentId": "doc-1",
            "documentVersion": 3,
            "citations": [{
                "format": "zotero",
                "renderedText": "[12]",
                "blockId": "b7",
                "start": 13,
                "end": 17,
                "targets": [{"ordinal": 1, "referenceKey": "12", "citedLocator": "table:0"}]
            }]
        });
        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .body(Body::from(sync_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(sync_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let first = json_body(response).await["data"].clone();
        assert_eq!(first["status"], "completed");
        assert_eq!(first["occurrenceCount"], 1);
        let run_id = first["syncRunId"].as_str().unwrap().to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(sync_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["data"]["syncRunId"], run_id);

        let mut changed_body = sync_body.clone();
        changed_body["citations"][0]["renderedText"] = serde_json::json!("[13]");
        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(changed_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            json_body(response).await["code"],
            "manuscript_citation_sync_conflict"
        );

        let latest_path = format!("{sync_path}/latest");
        let response = router
            .clone()
            .oneshot(Request::get(latest_path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["data"]["syncRunId"], run_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-citation-sync-runs/{run_id}/occurrences"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let sync_occurrence = json_body(response).await["data"][0].clone();
        assert_eq!(sync_occurrence["start"], 13);
        assert_eq!(sync_occurrence["end"], 17);
        assert_eq!(sync_occurrence["format"], "zotero");
        let sync_occurrence_id = sync_occurrence["syncOccurrenceId"]
            .as_str()
            .unwrap()
            .to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-citation-sync-occurrences/{sync_occurrence_id}/targets"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let target = json_body(response).await["data"][0].clone();
        assert_eq!(target["documentTargetOrdinal"], 1);
        let citation_target_id = target["citationTargetId"].as_str().unwrap();

        let catalog_path =
            format!("/api/research/manuscript-citation-syncs/{run_id}/reference-catalog");
        let catalog_body = serde_json::json!({
            "documentId": "doc-1",
            "documentVersion": 3,
            "citations": [{
                "citationOccurrenceId": sync_occurrence["citationOccurrenceId"],
                "blockId": "b7",
                "start": 13,
                "end": 17,
                "format": "zotero",
                "targets": [{
                    "citationTargetId": citation_target_id,
                    "ordinal": 1,
                    "referenceKey": "12",
                    "zotero": {"itemId": "12", "uris": []}
                }]
            }]
        });

        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .body(Body::from(catalog_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let mut override_body = catalog_body.clone();
        override_body["researchCaseId"] = serde_json::json!(case_id);
        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(override_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(catalog_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let catalog = json_body(response).await["data"].clone();
        assert_eq!(catalog["status"], "completed");
        assert_eq!(catalog["entryCount"], 1);
        assert_eq!(catalog["targetMappingCount"], 1);
        let catalog_run_id = catalog["catalogRunId"].as_str().unwrap().to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(catalog_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["catalogRunId"],
            catalog_run_id
        );

        let response = router
            .clone()
            .oneshot(Request::get(&catalog_path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["catalogRunId"],
            catalog_run_id
        );

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/cases/{case_id}/manuscripts/{source_id}/reference-catalog/latest"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["catalogRunId"],
            catalog_run_id
        );

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-reference-catalog-runs/{catalog_run_id}/entries"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let entry = json_body(response).await["data"][0].clone();
        assert_eq!(entry["format"], "zotero");
        assert_eq!(entry["referenceKey"], "12");
        let entry_id = entry["entryId"].as_str().unwrap();

        let response = router
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-reference-entries/{entry_id}/mappings"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"][0]["citationTargetId"],
            citation_target_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tower::ServiceExt;

    async fn test_runtime() -> Arc<CoreRuntime> {
        Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        )
    }

    async fn test_router() -> Router {
        let runtime = test_runtime().await;
        build_router(runtime)
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn health_endpoint_returns_stable_payload() {
        let response = test_router()
            .await
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["status"], "ok");
        assert_eq!(json["data"]["service"], "9profs-core");
    }

    #[tokio::test]
    async fn websocket_endpoint_accepts_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, test_router().await).await.unwrap();
        });

        let (mut socket, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
        socket.close(None).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn assistant_list_get_and_custom_crud_endpoints_work() {
        let router = test_router().await;
        let response = router
            .clone()
            .oneshot(Request::get("/api/assistants").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert_eq!(payload["success"], true);
        assert!(
            payload["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["source"] == "builtin")
        );

        let create = Request::post("/api/assistants")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "id": "api-custom",
                    "name": "API Custom",
                    "description": "Created through API",
                    "rules": "Custom API rules",
                    "skill_ids": ["writing-foundation", "document-foundation"],
                    "backend_agent_id": "codex"
                })
                .to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert_eq!(payload["data"]["source"], "custom");
        assert_eq!(
            payload["data"]["skill_ids"],
            serde_json::json!(["writing-foundation", "document-foundation"])
        );
        assert_eq!(payload["data"]["backend_agent_id"], "codex");

        let update = Request::put("/api/assistants/api-custom")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "rules": "Updated API rules" }).to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(update).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["rules"],
            "Updated API rules"
        );

        let response = router
            .clone()
            .oneshot(
                Request::delete("/api/assistants/api-custom")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = router
            .oneshot(
                Request::get("/api/assistants/api-custom")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn skill_list_get_invalid_id_and_scan_event_work() {
        let runtime = test_runtime().await;
        let mut events = runtime.event_bus().subscribe();
        let router = build_router(Arc::clone(&runtime));

        let response = router
            .clone()
            .oneshot(Request::get("/api/skills").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert!(
            payload["data"]["skills"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"] == "document-foundation")
        );

        let response = router
            .clone()
            .oneshot(
                Request::get("/api/skills/document-foundation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            json_body(response).await["data"]["content"]
                .as_str()
                .unwrap()
                .contains("Document Foundation")
        );

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/assistants")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "id": "../invalid",
                            "name": "Invalid",
                            "description": "Invalid"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = router
            .oneshot(
                Request::post("/api/skills/scan")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(events.recv().await.unwrap().name, "skill.catalogChanged");
    }

    #[tokio::test]
    async fn agent_run_routes_validate_and_report_invalid_ids() {
        let router = test_router().await;

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/agent-runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "assistant_id": "missing-assistant",
                            "input": "hello"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/agent-runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "assistant_id": "missing-assistant",
                            "input": "   "
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        for path in [
            "/api/agent-runs/not-a-run",
            "/api/agent-runs/not-a-run/tasks",
            "/api/agent-tasks/not-a-task/cancel",
        ] {
            let response = router
                .clone()
                .oneshot(if path.ends_with("cancel") {
                    Request::post(path).body(Body::empty()).unwrap()
                } else {
                    Request::get(path).body(Body::empty()).unwrap()
                })
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
}
#[tokio::test]
async fn route_composition_preserves_representative_paths_and_methods() {
    let runtime = Arc::new(
        CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
            .await
            .unwrap(),
    );
    let router = build_router(runtime);

    for (method, path, expected) in [
        ("GET", "/api/health", StatusCode::OK),
        ("GET", "/api/documents", StatusCode::OK),
        ("GET", "/api/document-proposals", StatusCode::OK),
        ("GET", "/api/agents", StatusCode::OK),
        ("GET", "/api/assistants", StatusCode::OK),
        ("GET", "/api/skills", StatusCode::OK),
        ("GET", "/api/mcp/servers", StatusCode::OK),
        ("GET", "/api/research/cases", StatusCode::OK),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{method} {path}");
    }

    for path in [
        "/api/documents",
        "/api/research/citation-verifications",
        "/api/research/cases/not-a-uuid/retrieval-index",
        "/api/research/manuscript-citation-sync-runs/not-a-uuid",
        "/ws",
        "/ws/documents",
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::NOT_FOUND, "POST {path}");
    }
}
