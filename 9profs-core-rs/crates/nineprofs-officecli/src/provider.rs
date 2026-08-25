use std::sync::Arc;

use async_trait::async_trait;
use nineprofs_tools::{
    ToolDefinition, ToolError, ToolHandler, ToolId, ToolInvocation, ToolPolicy, ToolProvider,
    ToolRegistration, ToolResult, ToolSource,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    artifact::DocumentResolver,
    operations::{
        DocumentReference, GetRequest, IssuesRequest, OfficeCliOperation, QueryRequest,
        ScreenshotRequest, ValidateRequest, ViewRequest,
    },
    runner::OfficeCliRunner,
};

pub const OFFICE_INSPECT_TEXT: &str = "office.inspect_text";
pub const OFFICE_INSPECT_ANNOTATED: &str = "office.inspect_annotated";
pub const OFFICE_INSPECT_OUTLINE: &str = "office.inspect_outline";
pub const OFFICE_INSPECT_STATS: &str = "office.inspect_stats";
pub const OFFICE_FIND_ISSUES: &str = "office.find_issues";
pub const OFFICE_GET: &str = "office.get";
pub const OFFICE_QUERY: &str = "office.query";
pub const OFFICE_VALIDATE: &str = "office.validate";
pub const OFFICE_RENDER: &str = "office.render";

#[derive(Clone)]
pub struct OfficeCliToolProvider {
    runner: Arc<OfficeCliRunner>,
    resolver: Arc<dyn DocumentResolver>,
}

impl OfficeCliToolProvider {
    pub fn new(runner: Arc<OfficeCliRunner>, resolver: Arc<dyn DocumentResolver>) -> Self {
        Self { runner, resolver }
    }

    pub fn runner(&self) -> Arc<OfficeCliRunner> {
        Arc::clone(&self.runner)
    }
}

#[async_trait]
impl ToolProvider for OfficeCliToolProvider {
    async fn list_tools(&self) -> Result<Vec<ToolRegistration>, ToolError> {
        if !self.runner.is_available() {
            return Ok(Vec::new());
        }
        let tools = [
            (OFFICE_INSPECT_TEXT, "Inspect document text", schema_view()),
            (
                OFFICE_INSPECT_ANNOTATED,
                "Inspect document text with formatting annotations",
                schema_view(),
            ),
            (
                OFFICE_INSPECT_OUTLINE,
                "Inspect document outline",
                schema_view(),
            ),
            (
                OFFICE_INSPECT_STATS,
                "Inspect document statistics",
                schema_view(),
            ),
            (
                OFFICE_FIND_ISSUES,
                "Inspect document content and structure issues",
                schema_issues(),
            ),
            (OFFICE_GET, "Read a selected document node", schema_get()),
            (OFFICE_QUERY, "Query document structure", schema_query()),
            (
                OFFICE_VALIDATE,
                "Validate Office OpenXML package structure",
                schema_document(),
            ),
            (
                OFFICE_RENDER,
                "Render a document page to a controlled artifact",
                schema_screenshot(),
            ),
        ];
        Ok(tools
            .into_iter()
            .map(|(id, description, input_schema)| ToolRegistration {
                definition: ToolDefinition {
                    id: ToolId::new(id),
                    name: id.to_owned(),
                    description: description.to_owned(),
                    input_schema,
                    source: ToolSource::OfficeCli,
                    policy: ToolPolicy::read_only(),
                    enabled: true,
                },
                handler: Arc::new(OfficeCliToolHandler {
                    id: ToolId::new(id),
                    runner: Arc::clone(&self.runner),
                    resolver: Arc::clone(&self.resolver),
                }),
            })
            .collect())
    }
}

struct OfficeCliToolHandler {
    id: ToolId,
    runner: Arc<OfficeCliRunner>,
    resolver: Arc<dyn DocumentResolver>,
}

#[async_trait]
impl ToolHandler for OfficeCliToolHandler {
    async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
        let operation = operation_from_arguments(self.id.as_str(), invocation.arguments)?;
        let result = self
            .runner
            .execute_readonly(operation, self.resolver.as_ref(), None)
            .await
            .map_err(|error| ToolError::Handler(error.to_string()))?;
        serde_json::to_value(result)
            .map(ToolResult::new)
            .map_err(|_| ToolError::Handler("OfficeCLI result could not be serialized".to_owned()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentArgs {
    artifact_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewArgs {
    artifact_id: String,
    start: Option<u32>,
    end: Option<u32>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IssuesArgs {
    artifact_id: String,
    issue_type: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetArgs {
    artifact_id: String,
    selector: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    artifact_id: String,
    selector: String,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    artifact_id: String,
    page: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
}

fn operation_from_arguments(id: &str, arguments: Value) -> Result<OfficeCliOperation, ToolError> {
    let parse = |result: Result<OfficeCliOperation, serde_json::Error>| {
        result.map_err(|_| ToolError::Handler("invalid OfficeCLI tool arguments".to_owned()))
    };
    match id {
        OFFICE_INSPECT_TEXT => parse(serde_json::from_value::<ViewArgs>(arguments).map(|args| {
            OfficeCliOperation::ViewText(ViewRequest {
                document: document(args.artifact_id),
                start: args.start,
                end: args.end,
                limit: args.limit,
            })
        })),
        OFFICE_INSPECT_ANNOTATED => {
            parse(serde_json::from_value::<ViewArgs>(arguments).map(|args| {
                OfficeCliOperation::ViewAnnotated(ViewRequest {
                    document: document(args.artifact_id),
                    start: args.start,
                    end: args.end,
                    limit: args.limit,
                })
            }))
        }
        OFFICE_INSPECT_OUTLINE => parse(serde_json::from_value::<DocumentArgs>(arguments).map(
            |args| {
                OfficeCliOperation::ViewOutline(ViewRequest {
                    document: document(args.artifact_id),
                    start: None,
                    end: None,
                    limit: None,
                })
            },
        )),
        OFFICE_INSPECT_STATS => parse(serde_json::from_value::<DocumentArgs>(arguments).map(
            |args| {
                OfficeCliOperation::ViewStats(ViewRequest {
                    document: document(args.artifact_id),
                    start: None,
                    end: None,
                    limit: None,
                })
            },
        )),
        OFFICE_FIND_ISSUES => parse(serde_json::from_value::<IssuesArgs>(arguments).map(|args| {
            OfficeCliOperation::ViewIssues(IssuesRequest {
                document: document(args.artifact_id),
                issue_type: args.issue_type,
                limit: args.limit,
            })
        })),
        OFFICE_GET => parse(serde_json::from_value::<GetArgs>(arguments).map(|args| {
            OfficeCliOperation::Get(GetRequest {
                document: document(args.artifact_id),
                selector: args.selector,
            })
        })),
        OFFICE_QUERY => parse(serde_json::from_value::<QueryArgs>(arguments).map(|args| {
            OfficeCliOperation::Query(QueryRequest {
                document: document(args.artifact_id),
                selector: args.selector,
                limit: args.limit,
            })
        })),
        OFFICE_VALIDATE => parse(
            serde_json::from_value::<DocumentArgs>(arguments).map(|args| {
                OfficeCliOperation::Validate(ValidateRequest {
                    document: document(args.artifact_id),
                })
            }),
        ),
        OFFICE_RENDER => parse(
            serde_json::from_value::<ScreenshotArgs>(arguments).map(|args| {
                OfficeCliOperation::Screenshot(ScreenshotRequest {
                    document: document(args.artifact_id),
                    page: args.page,
                    width: args.width,
                    height: args.height,
                })
            }),
        ),
        _ => Err(ToolError::UnknownTool(ToolId::new(id))),
    }
}

fn document(artifact_id: String) -> DocumentReference {
    DocumentReference { artifact_id }
}

fn schema_document() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id"],"properties":{"artifact_id":{"type":"string","minLength":1}}})
}

fn schema_view() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id"],"properties":{"artifact_id":{"type":"string","minLength":1},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1}}})
}

fn schema_issues() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id"],"properties":{"artifact_id":{"type":"string","minLength":1},"issue_type":{"type":"string"},"limit":{"type":"integer","minimum":1}}})
}

fn schema_get() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id","selector"],"properties":{"artifact_id":{"type":"string","minLength":1},"selector":{"type":"string","minLength":1}}})
}

fn schema_query() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id","selector"],"properties":{"artifact_id":{"type":"string","minLength":1},"selector":{"type":"string","minLength":1},"limit":{"type":"integer","minimum":1}}})
}

fn schema_screenshot() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id"],"properties":{"artifact_id":{"type":"string","minLength":1},"page":{"type":"integer","minimum":1},"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096}}})
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::{
        artifact::{ArtifactKind, ResolvedDocument},
        config::{OfficeCliConfig, OfficeCliStatus},
        process::{ProcessBackend, ProcessError, ProcessOutput},
        runner::test_runner,
    };

    struct Backend;

    #[async_trait]
    impl ProcessBackend for Backend {
        async fn run(
            &self,
            _args: &[OsString],
            _environment: &[(String, OsString)],
            _max_output_bytes: usize,
        ) -> Result<ProcessOutput, ProcessError> {
            Ok(ProcessOutput {
                stdout: "{}".to_owned(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
    }

    struct Resolver;

    impl DocumentResolver for Resolver {
        fn resolve(
            &self,
            reference: &DocumentReference,
        ) -> Result<ResolvedDocument, crate::ArtifactError> {
            Ok(ResolvedDocument {
                id: reference.artifact_id.clone(),
                path: "approved.docx".into(),
                kind: ArtifactKind::Detached,
            })
        }
    }

    fn provider() -> OfficeCliToolProvider {
        let mut config = OfficeCliConfig::default();
        config.timeout = Duration::from_secs(1);
        let runner = test_runner(config, Arc::new(Backend), OfficeCliStatus::available());
        OfficeCliToolProvider::new(Arc::new(runner), Arc::new(Resolver))
    }

    #[tokio::test]
    async fn registers_read_only_tools_and_default_toolset_denies_them() {
        let provider = provider();
        let registrations = provider.list_tools().await.unwrap();
        assert_eq!(registrations.len(), 9);
        assert!(registrations.iter().all(|registration| {
            registration.definition.source == ToolSource::OfficeCli
                && registration.definition.policy.effects
                    == std::collections::BTreeSet::from([nineprofs_tools::ToolEffect::Read])
        }));

        let registry = nineprofs_tools::ToolRegistry::new();
        registry.register_provider(&provider).await.unwrap();
        let id = ToolId::new(OFFICE_INSPECT_TEXT);
        let denied = registry
            .execute(
                ToolInvocation::new(id.clone(), json!({"artifact_id":"doc"})),
                &nineprofs_tools::ToolSet::default(),
            )
            .await;
        assert!(matches!(
            denied,
            Err(ToolError::ToolNotAuthorized(tool_id)) if tool_id == id
        ));

        let allowed = registry
            .execute(
                ToolInvocation::new(id, json!({"artifact_id":"doc"})),
                &nineprofs_tools::ToolSet::from_ids([OFFICE_INSPECT_TEXT]),
            )
            .await
            .unwrap();
        assert_eq!(allowed.output["operation"], "view_text");
        assert_eq!(allowed.output["document_id"], "doc");
    }

    #[tokio::test]
    async fn mutation_arguments_are_rejected_by_typed_schema() {
        let provider = provider();
        let registrations = provider.list_tools().await.unwrap();
        let registry = nineprofs_tools::ToolRegistry::new();
        registry.register_provider(&provider).await.unwrap();
        let error = registry
            .execute(
                ToolInvocation::new(
                    ToolId::new(OFFICE_INSPECT_TEXT),
                    json!({"artifact_id":"doc","command":"set","path":"C:\\secret.docx"}),
                ),
                &nineprofs_tools::ToolSet::from_ids([OFFICE_INSPECT_TEXT]),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, ToolError::Handler(message) if message.contains("invalid OfficeCLI tool arguments"))
        );
        assert!(registrations.iter().all(|registration| {
            registration.definition.input_schema["properties"]
                .get("command")
                .is_none()
        }));
    }

    #[tokio::test]
    async fn unavailable_provider_contributes_no_tools() {
        let runner = Arc::new(OfficeCliRunner::initialize(OfficeCliConfig::default()).await);
        let provider = OfficeCliToolProvider::new(runner, Arc::new(Resolver));
        assert!(provider.list_tools().await.unwrap().is_empty());
    }
}
