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
    mutation::{CreateDocumentRequest, DetachedMutationRequest, DetachedMutationService},
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
pub const OFFICE_CREATE: &str = "office.create";
pub const OFFICE_MUTATE_DETACHED: &str = "office.mutate_detached";

#[derive(Clone)]
pub struct OfficeCliToolProvider {
    runner: Arc<OfficeCliRunner>,
    resolver: Arc<dyn DocumentResolver>,
    mutation_service: Arc<DetachedMutationService>,
}

impl OfficeCliToolProvider {
    pub fn new(runner: Arc<OfficeCliRunner>, resolver: Arc<dyn DocumentResolver>) -> Self {
        let mutation_service = Arc::new(DetachedMutationService::new(
            Arc::clone(&runner),
            Arc::clone(&resolver),
        ));
        Self {
            runner,
            resolver,
            mutation_service,
        }
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
            (
                OFFICE_INSPECT_TEXT,
                "Inspect document text",
                schema_view(),
                false,
            ),
            (
                OFFICE_INSPECT_ANNOTATED,
                "Inspect document text with formatting annotations",
                schema_view(),
                false,
            ),
            (
                OFFICE_INSPECT_OUTLINE,
                "Inspect document outline",
                schema_view(),
                false,
            ),
            (
                OFFICE_INSPECT_STATS,
                "Inspect document statistics",
                schema_view(),
                false,
            ),
            (
                OFFICE_FIND_ISSUES,
                "Inspect document content and structure issues",
                schema_issues(),
                false,
            ),
            (
                OFFICE_GET,
                "Read a selected document node",
                schema_get(),
                false,
            ),
            (
                OFFICE_QUERY,
                "Query document structure",
                schema_query(),
                false,
            ),
            (
                OFFICE_VALIDATE,
                "Validate Office OpenXML package structure",
                schema_document(),
                false,
            ),
            (
                OFFICE_RENDER,
                "Render document pages, sheets, or slides to controlled PNG artifacts",
                schema_screenshot(),
                false,
            ),
            (
                OFFICE_CREATE,
                "Create a new detached Office document",
                schema_create(),
                true,
            ),
            (
                OFFICE_MUTATE_DETACHED,
                "Apply typed mutations to a detached Office artifact copy",
                schema_mutate_detached(),
                true,
            ),
        ];
        Ok(tools
            .into_iter()
            .map(
                |(id, description, input_schema, writable)| ToolRegistration {
                    definition: ToolDefinition {
                        id: ToolId::new(id),
                        name: id.to_owned(),
                        description: description.to_owned(),
                        input_schema,
                        source: ToolSource::OfficeCli,
                        policy: if writable {
                            ToolPolicy::write_requires_confirmation()
                        } else {
                            ToolPolicy::read_only()
                        },
                        enabled: true,
                    },
                    handler: Arc::new(OfficeCliToolHandler {
                        id: ToolId::new(id),
                        runner: Arc::clone(&self.runner),
                        resolver: Arc::clone(&self.resolver),
                        mutation_service: Arc::clone(&self.mutation_service),
                    }),
                },
            )
            .collect())
    }
}

struct OfficeCliToolHandler {
    id: ToolId,
    runner: Arc<OfficeCliRunner>,
    resolver: Arc<dyn DocumentResolver>,
    mutation_service: Arc<DetachedMutationService>,
}

#[async_trait]
impl ToolHandler for OfficeCliToolHandler {
    async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
        if self.id.as_str() == OFFICE_CREATE {
            let args = serde_json::from_value::<CreateArgs>(invocation.arguments)
                .map_err(|_| ToolError::Handler("invalid OfficeCLI create arguments".to_owned()))?;
            let result = self
                .mutation_service
                .create(
                    CreateDocumentRequest {
                        document_type: args.document_type,
                        logical_name: args.logical_name,
                        operations: args.operations,
                    },
                    None,
                )
                .await
                .map_err(|error| ToolError::Handler(error.to_string()))?;
            return serialize_result(result);
        }
        if self.id.as_str() == OFFICE_MUTATE_DETACHED {
            if !typed_mutation_json_is_safe(&invocation.arguments) {
                return Err(ToolError::Handler(
                    "invalid OfficeCLI tool arguments".to_owned(),
                ));
            }
            let args = serde_json::from_value::<MutateDetachedArgs>(invocation.arguments).map_err(
                |_| ToolError::Handler("invalid OfficeCLI detached mutation arguments".to_owned()),
            )?;
            let result = self
                .mutation_service
                .mutate_detached(
                    DetachedMutationRequest {
                        document: document(args.artifact_id),
                        operations: args.operations,
                        base_revision_id: args.base_revision_id,
                    },
                    None,
                )
                .await
                .map_err(|error| ToolError::Handler(error.to_string()))?;
            return serialize_result(result);
        }
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

fn serialize_result<T: serde::Serialize>(result: T) -> Result<ToolResult, ToolError> {
    serde_json::to_value(result)
        .map(ToolResult::new)
        .map_err(|_| ToolError::Handler("OfficeCLI result could not be serialized".to_owned()))
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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    artifact_id: String,
    page: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    document_type: crate::OfficeDocumentType,
    logical_name: Option<String>,
    #[serde(default)]
    operations: Vec<crate::OfficeMutation>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MutateDetachedArgs {
    artifact_id: String,
    operations: Vec<crate::OfficeMutation>,
    #[serde(default)]
    base_revision_id: Option<String>,
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
    json!({"type":"object","additionalProperties":false,"required":["artifact_id","selector"],"properties":{"artifact_id":{"type":"string","minLength":1},"selector":{"type":"string","minLength":1}}})
}

fn schema_screenshot() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["artifact_id"],"properties":{"artifact_id":{"type":"string","minLength":1},"page":{"type":"integer","minimum":1},"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096}}})
}

fn schema_create() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["document_type"],
        "properties":{
            "document_type":{"type":"string","enum":["docx","xlsx","pptx"]},
            "logical_name":{"type":"string","minLength":1,"maxLength":256},
            "operations":{"type":"array","maxItems":64,"items":mutation_schema()}
        }
    })
}

fn schema_mutate_detached() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["artifact_id","operations"],
        "properties":{
            "artifact_id":{"type":"string","minLength":1},
            "base_revision_id":{"type":"string","minLength":1},
            "operations":{"type":"array","minItems":1,"maxItems":64,"items":mutation_schema()}
        }
    })
}

fn typed_mutation_json_is_safe(arguments: &Value) -> bool {
    let Some(object) = arguments.as_object() else {
        return false;
    };
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "artifact_id" | "base_revision_id" | "operations"
        )
    }) {
        return false;
    }
    let Some(operations) = object.get("operations").and_then(Value::as_array) else {
        return false;
    };
    operations.iter().all(|operation| {
        let Some(operation_object) = operation.as_object() else {
            return false;
        };
        let Some(op) = operation_object.get("op").and_then(Value::as_str) else {
            return false;
        };
        let allowed = match op {
            "set" => &["op", "selector", "properties"][..],
            "add" => &["op", "parent", "element_type", "properties"][..],
            "remove" => &["op", "selector"][..],
            "move" | "copy" => &["op", "selector", "target", "index"][..],
            "swap" => &["op", "first", "second"][..],
            _ => return false,
        };
        operation_object
            .keys()
            .all(|key| allowed.contains(&key.as_str()))
    })
}

fn mutation_schema() -> Value {
    json!({
        "oneOf":[
            {"type":"object","additionalProperties":false,"required":["op","selector","properties"],"properties":{"op":{"const":"set"},"selector":{"type":"string","pattern":"^/"},"properties":{"type":"object","additionalProperties":{"type":"string"}}}},
            {"type":"object","additionalProperties":false,"required":["op","parent","element_type","properties"],"properties":{"op":{"const":"add"},"parent":{"type":"string","pattern":"^/"},"element_type":{"type":"string","minLength":1},"properties":{"type":"object","additionalProperties":{"type":"string"}}}},
            {"type":"object","additionalProperties":false,"required":["op","selector"],"properties":{"op":{"const":"remove"},"selector":{"type":"string","pattern":"^/"}}},
            {"type":"object","additionalProperties":false,"required":["op","selector","target"],"properties":{"op":{"const":"move"},"selector":{"type":"string","pattern":"^/"},"target":{"type":"string","pattern":"^/"},"index":{"type":"integer","minimum":0}}},
            {"type":"object","additionalProperties":false,"required":["op","selector","target"],"properties":{"op":{"const":"copy"},"selector":{"type":"string","pattern":"^/"},"target":{"type":"string","pattern":"^/"},"index":{"type":"integer","minimum":0}}},
            {"type":"object","additionalProperties":false,"required":["op","first","second"],"properties":{"op":{"const":"swap"},"first":{"type":"string","pattern":"^/"},"second":{"type":"string","pattern":"^/"}}}
        ]
    })
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

    struct Resolver {
        kind: ArtifactKind,
    }

    impl DocumentResolver for Resolver {
        fn resolve(
            &self,
            reference: &DocumentReference,
        ) -> Result<ResolvedDocument, crate::ArtifactError> {
            Ok(ResolvedDocument {
                id: reference.artifact_id.clone(),
                path: "approved.docx".into(),
                kind: self.kind.clone(),
            })
        }
    }

    fn provider() -> OfficeCliToolProvider {
        provider_with_kind(ArtifactKind::Detached)
    }

    fn provider_with_kind(kind: ArtifactKind) -> OfficeCliToolProvider {
        let mut config = OfficeCliConfig::default();
        config.timeout = Duration::from_secs(1);
        let runner = test_runner(config, Arc::new(Backend), OfficeCliStatus::available());
        OfficeCliToolProvider::new(Arc::new(runner), Arc::new(Resolver { kind }))
    }

    #[tokio::test]
    async fn registers_read_and_write_tools_with_deny_by_default_authorization() {
        let provider = provider();
        let registrations = provider.list_tools().await.unwrap();
        assert_eq!(registrations.len(), 11);
        assert!(
            registrations
                .iter()
                .filter(|registration| {
                    registration.definition.policy.effects
                        == std::collections::BTreeSet::from([nineprofs_tools::ToolEffect::Read])
                })
                .all(|registration| {
                    registration.definition.source == ToolSource::OfficeCli
                        && registration.definition.policy.effects
                            == std::collections::BTreeSet::from([nineprofs_tools::ToolEffect::Read])
                })
        );
        let writes = registrations
            .iter()
            .filter(|registration| {
                registration.definition.policy.effects
                    == std::collections::BTreeSet::from([nineprofs_tools::ToolEffect::Write])
            })
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 2);
        assert!(
            writes
                .iter()
                .all(|registration| registration.definition.policy.requires_confirmation)
        );

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

        let write_denied = registry
            .execute(
                ToolInvocation::new(
                    ToolId::new(OFFICE_MUTATE_DETACHED),
                    json!({"artifact_id":"doc","operations":[]} ),
                ),
                &nineprofs_tools::ToolSet::default(),
            )
            .await;
        assert!(matches!(
            write_denied,
            Err(ToolError::ToolNotAuthorized(tool_id)) if tool_id.as_str() == OFFICE_MUTATE_DETACHED
        ));
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
        assert!(
            registrations
                .iter()
                .filter(|registration| registration.definition.policy.effects
                    == std::collections::BTreeSet::from([nineprofs_tools::ToolEffect::Write]))
                .all(|registration| {
                    registration.definition.input_schema["properties"]
                        .get("path")
                        .is_none()
                })
        );

        let raw_error = registry
            .execute(
                ToolInvocation::new(
                    ToolId::new(OFFICE_MUTATE_DETACHED),
                    json!({
                        "artifact_id":"doc",
                        "operations":[{
                            "op":"set",
                            "selector":"/body/p[1]",
                            "properties":{"text":"safe"},
                            "raw":"<w:p/>"
                        }]
                    }),
                ),
                &nineprofs_tools::ToolSet::from_ids([OFFICE_MUTATE_DETACHED]),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            raw_error,
            ToolError::Handler(message) if message.contains("invalid OfficeCLI tool arguments")
        ));
    }

    #[tokio::test]
    async fn explicit_tool_authorization_cannot_override_active_document_authority() {
        let provider = provider_with_kind(ArtifactKind::GenOfficeActive);
        let registry = nineprofs_tools::ToolRegistry::new();
        registry.register_provider(&provider).await.unwrap();
        let error = registry
            .execute(
                ToolInvocation::new(
                    ToolId::new(OFFICE_MUTATE_DETACHED),
                    json!({
                        "artifact_id":"active",
                        "operations":[{
                            "op":"set",
                            "selector":"/body/p[1]",
                            "properties":{"text":"blocked"}
                        }]
                    }),
                ),
                &nineprofs_tools::ToolSet::from_ids([OFFICE_MUTATE_DETACHED]),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ToolError::Handler(message) if message.contains("not writable")
        ));
    }

    #[tokio::test]
    async fn unavailable_provider_contributes_no_tools() {
        let runner = Arc::new(OfficeCliRunner::initialize(OfficeCliConfig::default()).await);
        let provider = OfficeCliToolProvider::new(
            runner,
            Arc::new(Resolver {
                kind: ArtifactKind::Detached,
            }),
        );
        assert!(provider.list_tools().await.unwrap().is_empty());
    }
}
