//! Explicit active-document tools and Core-owned proposal state.
//!
//! This crate depends on both the document bridge and generic tool runtime so
//! the document domain never depends on tool registration or AionRS.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use nineprofs_api_types::ActiveDocumentDto;
use nineprofs_common::{new_id, now_ms};
use nineprofs_documents::{
    ActiveDocumentDescriptor, ActiveDocumentRegistry, DOCUMENT_BRIDGE_CAPABILITY_COMMIT,
    DOCUMENT_BRIDGE_CAPABILITY_INSPECT, DOCX_DOCUMENT_TYPE, DocumentBridgeError, DocumentChange,
    DocumentChangeSet, DocumentChangeTarget, GENOFFICE_ACTIVE_AUTHORITY,
};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_tools::{
    ToolDefinition, ToolEffect, ToolError, ToolHandler, ToolId, ToolInvocation, ToolPolicy,
    ToolProvider, ToolRegistration, ToolResult, ToolSource,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::sync::RwLock;

pub const DOCUMENT_LIST_ACTIVE: &str = "document.list_active";
pub const DOCUMENT_INSPECT_ACTIVE: &str = "document.inspect_active";
pub const DOCUMENT_PROPOSE_ACTIVE_CHANGES: &str = "document.propose_active_changes";
pub const PROPOSAL_CREATED_EVENT: &str = "document.proposalCreated";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentProposalStoreLimits {
    pub max_proposals: usize,
    pub max_changes: usize,
    pub max_payload_bytes: usize,
    pub max_summary_bytes: usize,
}

impl Default for DocumentProposalStoreLimits {
    fn default() -> Self {
        Self {
            max_proposals: 128,
            max_changes: 32,
            max_payload_bytes: 256 * 1024,
            max_summary_bytes: 4096,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProposalStoreError {
    #[error("maximum active-document proposals reached: {0}")]
    MaxProposals(usize),
    #[error("proposal contains too many changes: {actual} exceeds {limit}")]
    MaxChanges { actual: usize, limit: usize },
    #[error("proposal payload is too large: {actual} bytes exceeds {limit}")]
    PayloadTooLarge { actual: usize, limit: usize },
    #[error("proposal summary is too long: {actual} bytes exceeds {limit}")]
    SummaryTooLong { actual: usize, limit: usize },
    #[error("proposal payload could not be serialized: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredDocumentProposal {
    pub proposal_id: String,
    pub change_set: DocumentChangeSet,
    pub document_id: String,
    pub base_version: u64,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProposalFreshness {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProposalAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProposalView {
    pub proposal_id: String,
    pub change_set: DocumentChangeSet,
    pub document_id: String,
    pub base_version: u64,
    pub status: String,
    pub freshness: ProposalFreshness,
    pub availability: ProposalAvailability,
    pub current_version: Option<u64>,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Default)]
pub struct DocumentProposalStore {
    entries: Arc<RwLock<BTreeMap<String, StoredDocumentProposal>>>,
    limits: DocumentProposalStoreLimits,
}

impl DocumentProposalStore {
    pub fn new(limits: DocumentProposalStoreLimits) -> Self {
        Self {
            entries: Arc::new(RwLock::new(BTreeMap::new())),
            limits,
        }
    }

    pub fn limits(&self) -> &DocumentProposalStoreLimits {
        &self.limits
    }

    pub async fn create(
        &self,
        document_id: impl Into<String>,
        base_version: u64,
        changes: Vec<DocumentChange>,
        summary: Option<String>,
    ) -> Result<StoredDocumentProposal, ProposalStoreError> {
        if changes.len() > self.limits.max_changes {
            return Err(ProposalStoreError::MaxChanges {
                actual: changes.len(),
                limit: self.limits.max_changes,
            });
        }
        let payload_bytes = serde_json::to_vec(&changes)
            .map_err(|error| ProposalStoreError::Serialization(error.to_string()))?
            .len();
        if payload_bytes > self.limits.max_payload_bytes {
            return Err(ProposalStoreError::PayloadTooLarge {
                actual: payload_bytes,
                limit: self.limits.max_payload_bytes,
            });
        }
        if let Some(summary) = &summary {
            let summary_bytes = summary.len();
            if summary_bytes > self.limits.max_summary_bytes {
                return Err(ProposalStoreError::SummaryTooLong {
                    actual: summary_bytes,
                    limit: self.limits.max_summary_bytes,
                });
            }
        }

        let mut entries = self.entries.write().await;
        if entries.len() >= self.limits.max_proposals {
            return Err(ProposalStoreError::MaxProposals(self.limits.max_proposals));
        }
        let proposal_id = new_id();
        let document_id = document_id.into();
        let proposal = StoredDocumentProposal {
            proposal_id: proposal_id.clone(),
            change_set: DocumentChangeSet {
                id: proposal_id.clone(),
                status: "proposed".to_owned(),
                target: DocumentChangeTarget {
                    kind: GENOFFICE_ACTIVE_AUTHORITY.to_owned(),
                    document_id: document_id.clone(),
                    write_authority: "genoffice".to_owned(),
                },
                base_version,
                changes,
                approval: None,
            },
            document_id,
            base_version,
            created_at_ms: now_ms() as u64,
            summary,
        };
        entries.insert(proposal_id, proposal.clone());
        Ok(proposal)
    }

    pub async fn get(&self, proposal_id: &str) -> Option<StoredDocumentProposal> {
        self.entries.read().await.get(proposal_id).cloned()
    }

    pub async fn list(&self) -> Vec<StoredDocumentProposal> {
        let mut proposals: Vec<_> = self.entries.read().await.values().cloned().collect();
        proposals.sort_by(|left, right| {
            left.created_at_ms
                .cmp(&right.created_at_ms)
                .then_with(|| left.proposal_id.cmp(&right.proposal_id))
        });
        proposals
    }

    pub async fn list_views(
        &self,
        registry: &ActiveDocumentRegistry,
        document_id: Option<&str>,
    ) -> Vec<DocumentProposalView> {
        let active: BTreeMap<_, _> = registry
            .list()
            .await
            .into_iter()
            .map(|descriptor| (descriptor.document_id.clone(), descriptor))
            .collect();
        self.list()
            .await
            .into_iter()
            .filter(|proposal| document_id.is_none_or(|id| id == proposal.document_id))
            .map(|proposal| proposal.view(active.get(&proposal.document_id)))
            .collect()
    }

    pub async fn get_view(
        &self,
        registry: &ActiveDocumentRegistry,
        proposal_id: &str,
    ) -> Option<DocumentProposalView> {
        let proposal = self.get(proposal_id).await?;
        let active = registry.get(&proposal.document_id).await;
        Some(proposal.view(active.as_ref()))
    }
}

impl StoredDocumentProposal {
    pub fn view(&self, active: Option<&ActiveDocumentDescriptor>) -> DocumentProposalView {
        let (freshness, availability, current_version) = match active {
            None => (
                ProposalFreshness::Unavailable,
                ProposalAvailability::Unavailable,
                None,
            ),
            Some(descriptor) if descriptor.version == self.base_version => (
                ProposalFreshness::Fresh,
                ProposalAvailability::Available,
                Some(descriptor.version),
            ),
            Some(descriptor) => (
                ProposalFreshness::Stale,
                ProposalAvailability::Available,
                Some(descriptor.version),
            ),
        };
        DocumentProposalView {
            proposal_id: self.proposal_id.clone(),
            change_set: self.change_set.clone(),
            document_id: self.document_id.clone(),
            base_version: self.base_version,
            status: self.change_set.status.clone(),
            freshness,
            availability,
            current_version,
            created_at_ms: self.created_at_ms,
            summary: self.summary.clone(),
        }
    }
}

#[derive(Clone)]
pub struct DocumentToolProvider {
    bridge: Arc<nineprofs_documents::DocumentBridgeService>,
    proposals: DocumentProposalStore,
    events: Arc<BroadcastEventBus>,
}

impl DocumentToolProvider {
    pub fn new(
        bridge: Arc<nineprofs_documents::DocumentBridgeService>,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self::with_limits(bridge, events, DocumentProposalStoreLimits::default())
    }

    pub fn with_limits(
        bridge: Arc<nineprofs_documents::DocumentBridgeService>,
        events: Arc<BroadcastEventBus>,
        limits: DocumentProposalStoreLimits,
    ) -> Self {
        Self {
            bridge,
            proposals: DocumentProposalStore::new(limits),
            events,
        }
    }

    pub fn proposal_store(&self) -> DocumentProposalStore {
        self.proposals.clone()
    }

    pub async fn list_proposals(&self, document_id: Option<&str>) -> Vec<DocumentProposalView> {
        self.proposals
            .list_views(&self.bridge.registry(), document_id)
            .await
    }

    pub async fn get_proposal(&self, proposal_id: &str) -> Option<DocumentProposalView> {
        self.proposals
            .get_view(&self.bridge.registry(), proposal_id)
            .await
    }

    async fn execute(
        &self,
        kind: DocumentToolKind,
        arguments: Value,
    ) -> Result<Value, DocumentToolError> {
        match kind {
            DocumentToolKind::ListActive => {
                let _: EmptyInput = parse_input(arguments)?;
                let documents = self
                    .bridge
                    .list()
                    .await
                    .into_iter()
                    .map(active_document_dto)
                    .collect::<Vec<_>>();
                serde_json::to_value(documents)
                    .map_err(|error| DocumentToolError::Serialization(error.to_string()))
            }
            DocumentToolKind::InspectActive => {
                let input: InspectActiveInput = parse_input(arguments)?;
                let descriptor = self.require_document(&input.document_id).await?;
                ensure_document_capability(&descriptor, DOCUMENT_BRIDGE_CAPABILITY_INSPECT)?;
                let inspection = self
                    .bridge
                    .inspect_active_document(&input.document_id)
                    .await?;
                serde_json::to_value(inspection)
                    .map_err(|error| DocumentToolError::Serialization(error.to_string()))
            }
            DocumentToolKind::ProposeActiveChanges => {
                let input: ProposeActiveChangesInput = parse_input(arguments)?;
                let descriptor = self.require_document(&input.document_id).await?;
                ensure_document_capability(&descriptor, DOCUMENT_BRIDGE_CAPABILITY_COMMIT)?;
                if input.base_version != descriptor.version {
                    return Err(DocumentToolError::StaleVersion {
                        requested: input.base_version,
                        current: descriptor.version,
                    });
                }
                let changes = input
                    .changes
                    .into_iter()
                    .map(validate_change)
                    .collect::<Result<Vec<_>, _>>()?;
                let proposal = self
                    .proposals
                    .create(
                        input.document_id,
                        input.base_version,
                        changes,
                        input.summary,
                    )
                    .await?;
                let view = proposal.view(Some(&descriptor));
                let _ = self.events.publish(nineprofs_api_types::EventEnvelope::new(
                    PROPOSAL_CREATED_EVENT,
                    json!({
                        "proposalId": view.proposal_id,
                        "documentId": view.document_id,
                        "baseVersion": view.base_version,
                        "currentVersion": view.current_version,
                        "status": view.status,
                        "freshness": view.freshness,
                        "availability": view.availability,
                    }),
                ));
                serde_json::to_value(view)
                    .map_err(|error| DocumentToolError::Serialization(error.to_string()))
            }
        }
    }

    async fn require_document(
        &self,
        document_id: &str,
    ) -> Result<ActiveDocumentDescriptor, DocumentToolError> {
        self.bridge
            .get(document_id)
            .await
            .ok_or_else(|| DocumentToolError::Unavailable(document_id.to_owned()))
    }
}

#[async_trait]
impl ToolProvider for DocumentToolProvider {
    async fn list_tools(&self) -> Result<Vec<ToolRegistration>, ToolError> {
        Ok(vec![
            registration(
                DOCUMENT_LIST_ACTIVE,
                "List active documents",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {}
                }),
                ToolPolicy::read_only(),
                self.clone(),
                DocumentToolKind::ListActive,
            ),
            registration(
                DOCUMENT_INSPECT_ACTIVE,
                "Inspect the active document through its owning renderer",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["documentId"],
                    "properties": { "documentId": { "type": "string", "minLength": 1 } }
                }),
                ToolPolicy::read_only(),
                self.clone(),
                DocumentToolKind::InspectActive,
            ),
            registration(
                DOCUMENT_PROPOSE_ACTIVE_CHANGES,
                "Propose changes for review without changing the active document",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["documentId", "baseVersion", "changes"],
                    "properties": {
                        "documentId": { "type": "string", "minLength": 1 },
                        "baseVersion": { "type": "integer", "minimum": 0 },
                        "summary": { "type": "string", "maxLength": 4096 },
                        "changes": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 32,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["type", "payload"],
                                "properties": {
                                    "type": { "const": "docs.commandEnvelope" },
                                    "payload": {
                                        "type": "object",
                                        "required": ["commands"],
                                        "properties": { "commands": { "type": "array" } }
                                    }
                                }
                            }
                        }
                    }
                }),
                ToolPolicy::with_effects([ToolEffect::Write]),
                self.clone(),
                DocumentToolKind::ProposeActiveChanges,
            ),
        ])
    }
}

#[derive(Clone, Copy)]
enum DocumentToolKind {
    ListActive,
    InspectActive,
    ProposeActiveChanges,
}

struct DocumentToolHandler {
    provider: DocumentToolProvider,
    kind: DocumentToolKind,
}

#[async_trait]
impl ToolHandler for DocumentToolHandler {
    async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
        self.provider
            .execute(self.kind, invocation.arguments)
            .await
            .map(ToolResult::new)
            .map_err(|error| ToolError::Handler(error.to_string()))
    }
}

fn registration(
    id: &str,
    description: &str,
    input_schema: Value,
    policy: ToolPolicy,
    provider: DocumentToolProvider,
    kind: DocumentToolKind,
) -> ToolRegistration {
    ToolRegistration {
        definition: ToolDefinition {
            id: ToolId::new(id),
            name: id.to_owned(),
            description: description.to_owned(),
            input_schema,
            source: ToolSource::Builtin,
            policy,
            enabled: true,
        },
        handler: Arc::new(DocumentToolHandler { provider, kind }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectActiveInput {
    #[serde(rename = "documentId")]
    document_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeActiveChangesInput {
    #[serde(rename = "documentId")]
    document_id: String,
    #[serde(rename = "baseVersion")]
    base_version: u64,
    #[serde(default)]
    summary: Option<String>,
    changes: Vec<ProposedChangeInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedChangeInput {
    #[serde(rename = "type")]
    change_type: String,
    payload: Value,
}

#[derive(Debug, Error)]
enum DocumentToolError {
    #[error("invalid tool input: {0}")]
    InvalidInput(String),
    #[error("active document is unavailable: {0}")]
    Unavailable(String),
    #[error("active document is unsupported: {0}")]
    UnsupportedDocument(String),
    #[error("active document does not expose required capability: {0}")]
    MissingCapability(String),
    #[error("active document version is stale: requested {requested}, current {current}")]
    StaleVersion { requested: u64, current: u64 },
    #[error("unsupported active-document change type: {0}")]
    UnsupportedChange(String),
    #[error("invalid active-document change payload: {0}")]
    InvalidChangePayload(String),
    #[error(transparent)]
    Bridge(#[from] DocumentBridgeError),
    #[error(transparent)]
    Store(#[from] ProposalStoreError),
    #[error("tool output could not be serialized: {0}")]
    Serialization(String),
}

fn parse_input<T: serde::de::DeserializeOwned>(arguments: Value) -> Result<T, DocumentToolError> {
    serde_json::from_value(arguments)
        .map_err(|error| DocumentToolError::InvalidInput(error.to_string()))
}

fn active_document_dto(descriptor: ActiveDocumentDescriptor) -> ActiveDocumentDto {
    ActiveDocumentDto {
        document_id: descriptor.document_id,
        document_type: descriptor.document_type,
        authority: descriptor.authority,
        version: descriptor.version,
        capabilities: descriptor.capabilities,
        availability: "available".to_owned(),
    }
}

fn ensure_document_capability(
    descriptor: &ActiveDocumentDescriptor,
    capability: &str,
) -> Result<(), DocumentToolError> {
    if descriptor.document_type != DOCX_DOCUMENT_TYPE
        || descriptor.authority != GENOFFICE_ACTIVE_AUTHORITY
    {
        return Err(DocumentToolError::UnsupportedDocument(
            descriptor.document_id.clone(),
        ));
    }
    if !descriptor
        .capabilities
        .iter()
        .any(|value| value == capability)
    {
        return Err(DocumentToolError::MissingCapability(capability.to_owned()));
    }
    Ok(())
}

fn validate_change(input: ProposedChangeInput) -> Result<DocumentChange, DocumentToolError> {
    if input.change_type != "docs.commandEnvelope" {
        return Err(DocumentToolError::UnsupportedChange(input.change_type));
    }
    let Some(payload) = input.payload.as_object() else {
        return Err(DocumentToolError::InvalidChangePayload(
            "payload must be an object".to_owned(),
        ));
    };
    if !payload.get("commands").is_some_and(Value::is_array) {
        return Err(DocumentToolError::InvalidChangePayload(
            "docs.commandEnvelope payload must contain commands[]".to_owned(),
        ));
    }
    Ok(DocumentChange {
        change_type: input.change_type,
        payload: Some(input.payload),
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nineprofs_documents::{
        CoreMessage, DOCUMENT_BRIDGE_PROTOCOL_VERSION, DocumentBridgeConfig, DocumentRegistration,
        RegisteredSession,
    };
    use serde_json::json;
    use tokio::sync::mpsc;

    use super::*;

    fn bridge() -> Arc<nineprofs_documents::DocumentBridgeService> {
        Arc::new(nineprofs_documents::DocumentBridgeService::new(
            DocumentBridgeConfig {
                request_timeout: Duration::from_millis(100),
                ..Default::default()
            },
            Arc::new(BroadcastEventBus::new(32)),
        ))
    }

    async fn register(
        bridge: &nineprofs_documents::DocumentBridgeService,
        document_id: &str,
        version: u64,
    ) -> (RegisteredSession, mpsc::Receiver<CoreMessage>) {
        let (sender, receiver) = mpsc::channel(8);
        let session = bridge
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: document_id.to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        (session, receiver)
    }

    fn change() -> Value {
        json!({
            "type": "docs.commandEnvelope",
            "payload": { "commands": [{ "replaceAllText": { "find": "old", "replace": "new" } }] }
        })
    }

    #[tokio::test]
    async fn provider_registers_exactly_three_tools_with_safe_list_output() {
        let provider = DocumentToolProvider::new(bridge(), Arc::new(BroadcastEventBus::new(8)));
        let tools = provider.list_tools().await.unwrap();
        let ids: Vec<_> = tools
            .iter()
            .map(|tool| tool.definition.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                DOCUMENT_LIST_ACTIVE,
                DOCUMENT_INSPECT_ACTIVE,
                DOCUMENT_PROPOSE_ACTIVE_CHANGES
            ]
        );
        assert!(
            tools
                .iter()
                .all(|tool| !tool.definition.name.contains("commit"))
        );
        assert_eq!(
            tools
                .iter()
                .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_LIST_ACTIVE))
                .unwrap()
                .definition
                .policy,
            ToolPolicy::read_only()
        );
        assert_eq!(
            tools
                .iter()
                .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_PROPOSE_ACTIVE_CHANGES))
                .unwrap()
                .definition
                .policy
                .effects,
            std::collections::BTreeSet::from([ToolEffect::Write])
        );
    }

    #[tokio::test]
    async fn list_tool_returns_safe_active_metadata_without_session_internals() {
        let bridge = bridge();
        let (_session, _receiver) = register(&bridge, "doc-1", 5).await;
        let provider = DocumentToolProvider::new(bridge, Arc::new(BroadcastEventBus::new(8)));
        let registration = provider
            .list_tools()
            .await
            .unwrap()
            .into_iter()
            .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_LIST_ACTIVE))
            .unwrap();
        let result = registration
            .handler
            .execute(ToolInvocation::new(DOCUMENT_LIST_ACTIVE, json!({})))
            .await
            .unwrap();
        assert_eq!(result.output[0]["documentId"], "doc-1");
        assert_eq!(result.output[0]["authority"], GENOFFICE_ACTIVE_AUTHORITY);
        assert_eq!(result.output[0]["version"], 5);
        assert!(result.output[0].get("sessionId").is_none());
        assert!(result.output[0].get("filePath").is_none());
    }

    #[tokio::test]
    async fn hostile_fields_are_rejected_and_core_generates_proposed_authority() {
        let bridge = bridge();
        let (_session, _receiver) = register(&bridge, "doc-1", 5).await;
        let provider = DocumentToolProvider::new(bridge, Arc::new(BroadcastEventBus::new(8)));
        let registration = provider
            .list_tools()
            .await
            .unwrap()
            .into_iter()
            .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_PROPOSE_ACTIVE_CHANGES))
            .unwrap();

        let hostile = ToolInvocation::new(
            DOCUMENT_PROPOSE_ACTIVE_CHANGES,
            json!({
                "documentId": "doc-1",
                "baseVersion": 5,
                "status": "approved",
                "target": { "writeAuthority": "genoffice" },
                "changes": [change()]
            }),
        );
        assert!(registration.handler.execute(hostile).await.is_err());

        let result = registration
            .handler
            .execute(ToolInvocation::new(
                DOCUMENT_PROPOSE_ACTIVE_CHANGES,
                json!({ "documentId": "doc-1", "baseVersion": 5, "changes": [change()] }),
            ))
            .await
            .unwrap();
        assert_eq!(result.output["status"], "proposed");
        assert_eq!(
            result.output["changeSet"]["target"]["kind"],
            GENOFFICE_ACTIVE_AUTHORITY
        );
        assert_eq!(
            result.output["changeSet"]["target"]["writeAuthority"],
            "genoffice"
        );
        assert!(result.output["changeSet"]["approval"].is_null());
        assert!(!result.output["proposalId"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn inspect_tool_proxies_renderer_version_context_and_selection() {
        let bridge = bridge();
        let (_session, mut receiver) = register(&bridge, "doc-1", 5).await;
        let provider =
            DocumentToolProvider::new(bridge.clone(), Arc::new(BroadcastEventBus::new(8)));
        let registration = provider
            .list_tools()
            .await
            .unwrap()
            .into_iter()
            .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_INSPECT_ACTIVE))
            .unwrap();
        let session_id = bridge.get("doc-1").await.unwrap().session_id;
        let bridge_for_response = bridge.clone();
        let bridge_call = tokio::spawn(async move {
            let CoreMessage::Inspect {
                request_id,
                document_id,
            } = receiver.recv().await.unwrap()
            else {
                panic!("expected inspect request");
            };
            bridge_for_response
                .handle_response(
                    &session_id,
                    nineprofs_documents::DocumentBridgeResponse {
                        request_id,
                        document_id,
                        response: nineprofs_documents::RendererResponse::Inspection {
                            inspection: nineprofs_documents::DocumentInspection {
                                document_id: "doc-1".to_owned(),
                                authority: nineprofs_documents::DocumentAuthority {
                                    kind: GENOFFICE_ACTIVE_AUTHORITY.to_owned(),
                                    document_id: "doc-1".to_owned(),
                                    write_authority: "none".to_owned(),
                                },
                                version: 17,
                                value: nineprofs_documents::DocumentInspectionValue {
                                    context: json!({ "text": "renderer context" }),
                                    selection: json!({ "from": 4, "to": 9 }),
                                },
                            },
                        },
                    },
                )
                .await
                .unwrap();
        });
        let result = registration.handler.execute(ToolInvocation::new(
            DOCUMENT_INSPECT_ACTIVE,
            json!({ "documentId": "doc-1" }),
        ));
        let output = tokio::join!(result, bridge_call).0.unwrap();
        assert_eq!(output.output["version"], 17);
        assert_eq!(
            output.output["value"]["context"]["text"],
            "renderer context"
        );
        assert_eq!(output.output["value"]["selection"]["from"], 4);
    }

    #[tokio::test]
    async fn proposal_event_contains_metadata_without_change_payload() {
        let bridge = bridge();
        let (_session, _receiver) = register(&bridge, "doc-1", 5).await;
        let events = Arc::new(BroadcastEventBus::new(8));
        let mut receiver = events.subscribe();
        let provider = DocumentToolProvider::new(bridge, events);
        let registration = provider
            .list_tools()
            .await
            .unwrap()
            .into_iter()
            .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_PROPOSE_ACTIVE_CHANGES))
            .unwrap();
        registration
            .handler
            .execute(ToolInvocation::new(
                DOCUMENT_PROPOSE_ACTIVE_CHANGES,
                json!({ "documentId": "doc-1", "baseVersion": 5, "changes": [change()] }),
            ))
            .await
            .unwrap();
        let event = receiver.recv().await.unwrap();
        assert_eq!(event.name, PROPOSAL_CREATED_EVENT);
        assert!(event.payload.get("proposalId").is_some());
        assert!(event.payload.get("baseVersion").is_some());
        assert!(event.payload.get("changes").is_none());
        assert!(event.payload.get("commands").is_none());
    }

    #[tokio::test]
    async fn proposal_does_not_send_mutation_and_becomes_stale_after_version_change() {
        let bridge = bridge();
        let (session, mut receiver) = register(&bridge, "doc-1", 5).await;
        let provider =
            DocumentToolProvider::new(bridge.clone(), Arc::new(BroadcastEventBus::new(8)));
        let registration = provider
            .list_tools()
            .await
            .unwrap()
            .into_iter()
            .find(|tool| tool.definition.id == ToolId::new(DOCUMENT_PROPOSE_ACTIVE_CHANGES))
            .unwrap();
        let result = registration
            .handler
            .execute(ToolInvocation::new(
                DOCUMENT_PROPOSE_ACTIVE_CHANGES,
                json!({ "documentId": "doc-1", "baseVersion": 5, "changes": [change()] }),
            ))
            .await
            .unwrap();
        assert!(receiver.try_recv().is_err());
        assert_eq!(result.output["freshness"], "fresh");

        bridge
            .version_changed("doc-1", &session.session_id, 6)
            .await
            .unwrap();
        let proposal_id = result.output["proposalId"].as_str().unwrap();
        let view = provider.get_proposal(proposal_id).await.unwrap();
        assert_eq!(view.freshness, ProposalFreshness::Stale);
        assert_eq!(view.current_version, Some(6));
    }

    #[tokio::test]
    async fn proposal_store_generates_immutable_ids_and_deterministic_order() {
        let store = DocumentProposalStore::default();
        let first = store
            .create(
                "doc-1",
                5,
                vec![DocumentChange {
                    change_type: "docs.commandEnvelope".to_owned(),
                    payload: Some(json!({ "commands": [] })),
                }],
                Some("first".to_owned()),
            )
            .await
            .unwrap();
        let second = store
            .create(
                "doc-1",
                5,
                vec![DocumentChange {
                    change_type: "docs.commandEnvelope".to_owned(),
                    payload: Some(json!({ "commands": [] })),
                }],
                None,
            )
            .await
            .unwrap();
        assert_ne!(first.proposal_id, second.proposal_id);
        assert_eq!(first.proposal_id, first.change_set.id);
        assert_eq!(first.change_set.status, "proposed");
        assert_eq!(first.change_set.target.document_id, "doc-1");
        assert_eq!(first.change_set.target.write_authority, "genoffice");

        let mut copy = store.get(&first.proposal_id).await.unwrap();
        copy.change_set.status = "approved".to_owned();
        assert_eq!(
            store
                .get(&first.proposal_id)
                .await
                .unwrap()
                .change_set
                .status,
            "proposed"
        );

        let listed = store.list().await;
        let mut sorted_ids: Vec<_> = listed
            .iter()
            .map(|proposal| proposal.proposal_id.clone())
            .collect();
        sorted_ids.sort();
        let mut actual_ids: Vec<_> = listed
            .iter()
            .map(|proposal| proposal.proposal_id.clone())
            .collect();
        actual_ids.sort_by_key(|id| {
            listed
                .iter()
                .find(|proposal| &proposal.proposal_id == id)
                .map(|proposal| (proposal.created_at_ms, proposal.proposal_id.clone()))
        });
        assert_eq!(
            actual_ids,
            listed
                .iter()
                .map(|proposal| proposal.proposal_id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(sorted_ids.len(), 2);
    }

    #[tokio::test]
    async fn proposal_store_enforces_all_memory_limits() {
        let limits = DocumentProposalStoreLimits {
            max_proposals: 1,
            max_changes: 1,
            max_payload_bytes: 8,
            max_summary_bytes: 3,
        };
        let store = DocumentProposalStore::new(limits);
        let oversized_change = DocumentChange {
            change_type: "docs.commandEnvelope".to_owned(),
            payload: Some(json!({ "commands": ["large"] })),
        };
        assert!(matches!(
            store
                .create(
                    "doc-1",
                    0,
                    vec![oversized_change.clone(), oversized_change.clone()],
                    None
                )
                .await,
            Err(ProposalStoreError::MaxChanges { .. })
        ));
        assert!(matches!(
            store
                .create("doc-1", 0, vec![oversized_change.clone()], None)
                .await,
            Err(ProposalStoreError::PayloadTooLarge { .. })
        ));
        assert!(matches!(
            DocumentProposalStore::new(DocumentProposalStoreLimits {
                max_payload_bytes: 1024,
                max_summary_bytes: 3,
                ..DocumentProposalStoreLimits::default()
            })
            .create(
                "doc-1",
                0,
                vec![DocumentChange {
                    change_type: "docs.commandEnvelope".to_owned(),
                    payload: Some(json!({ "commands": [] })),
                }],
                Some("long".to_owned())
            )
            .await,
            Err(ProposalStoreError::SummaryTooLong { .. })
        ));

        let store = DocumentProposalStore::new(DocumentProposalStoreLimits {
            max_proposals: 1,
            max_payload_bytes: 1024,
            max_summary_bytes: 1024,
            ..DocumentProposalStoreLimits::default()
        });
        store
            .create(
                "doc-1",
                0,
                vec![DocumentChange {
                    change_type: "docs.commandEnvelope".to_owned(),
                    payload: Some(json!({ "commands": [] })),
                }],
                None,
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .create(
                    "doc-1",
                    0,
                    vec![DocumentChange {
                        change_type: "docs.commandEnvelope".to_owned(),
                        payload: Some(json!({ "commands": [] })),
                    }],
                    None,
                )
                .await,
            Err(ProposalStoreError::MaxProposals(1))
        ));
    }

    #[tokio::test]
    async fn freshness_tracks_disconnect_and_same_document_reconnect() {
        let bridge = bridge();
        let (session, _receiver) = register(&bridge, "doc-1", 5).await;
        let store = DocumentProposalStore::default();
        let proposal = store
            .create(
                "doc-1",
                5,
                vec![DocumentChange {
                    change_type: "docs.commandEnvelope".to_owned(),
                    payload: Some(json!({ "commands": [] })),
                }],
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .get_view(&bridge.registry(), &proposal.proposal_id)
                .await
                .unwrap()
                .freshness,
            ProposalFreshness::Fresh
        );
        bridge
            .unregister("doc-1", &session.session_id)
            .await
            .unwrap();
        assert_eq!(
            store
                .get_view(&bridge.registry(), &proposal.proposal_id)
                .await
                .unwrap()
                .availability,
            ProposalAvailability::Unavailable
        );

        let (_reconnected, _receiver) = register(&bridge, "doc-1", 5).await;
        assert_eq!(
            store
                .get_view(&bridge.registry(), &proposal.proposal_id)
                .await
                .unwrap()
                .freshness,
            ProposalFreshness::Fresh
        );
    }
}
