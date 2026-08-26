//! Ephemeral active-document registry and dedicated renderer bridge.
//!
//! Rust owns routing and correlation only. GenOffice remains the authority for
//! document state, version checks, inspection, and mutation.

use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use nineprofs_realtime::BroadcastEventBus;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};
use uuid::Uuid;

pub const DOCUMENT_BRIDGE_PROTOCOL_VERSION: &str = "1";
pub const DOCX_DOCUMENT_TYPE: &str = "docx";
pub const GENOFFICE_ACTIVE_AUTHORITY: &str = "genoffice-active";
pub const DOCUMENT_BRIDGE_CAPABILITY_INSPECT: &str = "inspect";
pub const DOCUMENT_BRIDGE_CAPABILITY_COMMIT: &str = "commitApprovedChangeSet";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Connected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDocumentDescriptor {
    pub document_id: String,
    pub document_type: String,
    pub authority: String,
    pub version: u64,
    pub connection_state: ConnectionState,
    pub session_id: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeAuthentication {
    pub session_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RendererMessage {
    Register {
        #[serde(rename = "protocolVersion")]
        protocol_version: String,
        #[serde(rename = "documentId")]
        document_id: String,
        #[serde(rename = "documentType")]
        document_type: String,
        version: u64,
        capabilities: Vec<String>,
        #[serde(default)]
        auth: Option<BridgeAuthentication>,
    },
    VersionChanged {
        #[serde(rename = "documentId")]
        document_id: String,
        version: u64,
    },
    Response {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "documentId")]
        document_id: String,
        response: RendererResponse,
    },
    Unregister {
        #[serde(rename = "documentId")]
        document_id: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoreMessage {
    Registered {
        #[serde(rename = "documentId")]
        document_id: String,
        #[serde(rename = "sessionId")]
        session_id: String,
        version: u64,
    },
    Inspect {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "documentId")]
        document_id: String,
    },
    CommitApprovedChangeSet {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "documentId")]
        document_id: String,
        #[serde(rename = "changeSet")]
        change_set: DocumentChangeSet,
    },
    Error {
        #[serde(rename = "requestId")]
        request_id: Option<String>,
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentBridgeResponse {
    pub request_id: String,
    pub document_id: String,
    pub response: RendererResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RendererResponse {
    Inspection { inspection: DocumentInspection },
    Mutation { result: DocumentMutationResult },
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentAuthority {
    pub kind: String,
    pub document_id: String,
    pub write_authority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInspection {
    pub document_id: String,
    pub authority: DocumentAuthority,
    pub version: u64,
    pub value: DocumentInspectionValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentInspectionValue {
    pub context: Value,
    pub selection: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChangeSet {
    pub id: String,
    pub status: String,
    pub target: DocumentChangeTarget,
    pub base_version: u64,
    pub changes: Vec<DocumentChange>,
    #[serde(default)]
    pub approval: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChangeTarget {
    pub kind: String,
    pub document_id: String,
    pub write_authority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChange {
    #[serde(rename = "type")]
    pub change_type: String,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum DocumentMutationResult {
    Applied {
        #[serde(rename = "changeSetId")]
        change_set_id: String,
        #[serde(rename = "documentId")]
        document_id: String,
        #[serde(rename = "previousVersion")]
        previous_version: u64,
        #[serde(rename = "newVersion")]
        new_version: u64,
        #[serde(rename = "commandCount")]
        command_count: u64,
        #[serde(rename = "changedCount")]
        changed_count: u64,
    },
    Conflict {
        #[serde(rename = "changeSetId")]
        change_set_id: String,
        #[serde(rename = "documentId")]
        document_id: String,
        reason: String,
        #[serde(rename = "baseVersion")]
        base_version: u64,
        #[serde(rename = "currentVersion")]
        current_version: u64,
    },
}

#[derive(Debug, Clone)]
pub struct DocumentRegistration {
    pub protocol_version: String,
    pub document_id: String,
    pub document_type: String,
    pub version: u64,
    pub capabilities: Vec<String>,
}

impl DocumentRegistration {
    fn descriptor(self, session_id: String) -> ActiveDocumentDescriptor {
        ActiveDocumentDescriptor {
            document_id: self.document_id,
            document_type: self.document_type,
            authority: GENOFFICE_ACTIVE_AUTHORITY.to_owned(),
            version: self.version,
            connection_state: ConnectionState::Connected,
            session_id,
            capabilities: self.capabilities,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredSession {
    pub document_id: String,
    pub session_id: String,
    pub version: u64,
}

#[derive(Debug, Clone)]
struct ActiveDocumentEntry {
    descriptor: ActiveDocumentDescriptor,
    sender: mpsc::Sender<CoreMessage>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveDocumentRegistry {
    entries: Arc<RwLock<HashMap<String, ActiveDocumentEntry>>>,
}

impl ActiveDocumentRegistry {
    pub async fn register(
        &self,
        registration: DocumentRegistration,
        sender: mpsc::Sender<CoreMessage>,
    ) -> Result<(RegisteredSession, Option<ActiveDocumentDescriptor>), RegistryError> {
        if registration.protocol_version != DOCUMENT_BRIDGE_PROTOCOL_VERSION {
            return Err(RegistryError::ProtocolVersion(
                registration.protocol_version,
            ));
        }
        let session_id = Uuid::new_v4().to_string();
        let descriptor = registration.descriptor(session_id.clone());
        let mut entries = self.entries.write().await;
        if let Some(existing) = entries.get(&descriptor.document_id) {
            if descriptor.version < existing.descriptor.version {
                return Err(RegistryError::LowerVersion {
                    document_id: descriptor.document_id,
                    current: existing.descriptor.version,
                    received: descriptor.version,
                });
            }
        }
        let previous = entries
            .insert(
                descriptor.document_id.clone(),
                ActiveDocumentEntry {
                    descriptor: descriptor.clone(),
                    sender,
                },
            )
            .map(|entry| entry.descriptor);
        Ok((
            RegisteredSession {
                document_id: descriptor.document_id,
                session_id: descriptor.session_id,
                version: descriptor.version,
            },
            previous,
        ))
    }

    pub async fn get(&self, document_id: &str) -> Option<ActiveDocumentDescriptor> {
        self.entries
            .read()
            .await
            .get(document_id)
            .map(|entry| entry.descriptor.clone())
    }

    pub async fn list(&self) -> Vec<ActiveDocumentDescriptor> {
        let mut descriptors: Vec<_> = self
            .entries
            .read()
            .await
            .values()
            .map(|entry| entry.descriptor.clone())
            .collect();
        descriptors.sort_by(|left, right| left.document_id.cmp(&right.document_id));
        descriptors
    }

    pub async fn sender_for(
        &self,
        document_id: &str,
    ) -> Result<(String, mpsc::Sender<CoreMessage>), RegistryError> {
        self.entries
            .read()
            .await
            .get(document_id)
            .map(|entry| (entry.descriptor.session_id.clone(), entry.sender.clone()))
            .ok_or_else(|| RegistryError::NotActive(document_id.to_owned()))
    }

    pub async fn update_version(
        &self,
        document_id: &str,
        session_id: &str,
        version: u64,
    ) -> Result<VersionUpdate, RegistryError> {
        let mut entries = self.entries.write().await;
        let entry = entries
            .get_mut(document_id)
            .ok_or_else(|| RegistryError::NotActive(document_id.to_owned()))?;
        if entry.descriptor.session_id != session_id {
            return Err(RegistryError::StaleSession(document_id.to_owned()));
        }
        if version < entry.descriptor.version {
            return Ok(VersionUpdate::IgnoredLower {
                current: entry.descriptor.version,
                received: version,
            });
        }
        if version == entry.descriptor.version {
            return Ok(VersionUpdate::Unchanged(version));
        }
        entry.descriptor.version = version;
        Ok(VersionUpdate::Changed(version))
    }

    pub async fn unregister(
        &self,
        document_id: &str,
        session_id: &str,
    ) -> Result<Option<ActiveDocumentDescriptor>, RegistryError> {
        let mut entries = self.entries.write().await;
        let Some(entry) = entries.get(document_id) else {
            return Ok(None);
        };
        if entry.descriptor.session_id != session_id {
            return Err(RegistryError::StaleSession(document_id.to_owned()));
        }
        Ok(entries.remove(document_id).map(|entry| entry.descriptor))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionUpdate {
    Changed(u64),
    Unchanged(u64),
    IgnoredLower { current: u64, received: u64 },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RegistryError {
    #[error("document is not active: {0}")]
    NotActive(String),
    #[error("document session is stale: {0}")]
    StaleSession(String),
    #[error(
        "document version moved backwards for {document_id}: current {current}, received {received}"
    )]
    LowerVersion {
        document_id: String,
        current: u64,
        received: u64,
    },
    #[error("unsupported document bridge protocol version: {0}")]
    ProtocolVersion(String),
}

#[derive(Debug, Clone)]
pub struct DocumentBridgeConfig {
    pub request_timeout: Duration,
    pub channel_capacity: usize,
    pub session_secret: Option<Arc<str>>,
}

impl Default for DocumentBridgeConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(5),
            channel_capacity: 32,
            session_secret: None,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DocumentBridgeError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("document bridge authentication failed")]
    Authentication,
    #[error("document bridge request timed out: {0}")]
    Timeout(String),
    #[error("document bridge disconnected: {0}")]
    Disconnected(String),
    #[error("document bridge returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("approved change set is invalid: {0}")]
    InvalidChangeSet(String),
}

struct PendingRequest {
    document_id: String,
    session_id: String,
    sender: oneshot::Sender<Result<RendererResponse, DocumentBridgeError>>,
}

#[derive(Clone)]
pub struct DocumentBridgeService {
    registry: ActiveDocumentRegistry,
    event_bus: Arc<BroadcastEventBus>,
    config: DocumentBridgeConfig,
    pending: Arc<Mutex<HashMap<String, PendingRequest>>>,
}

impl DocumentBridgeService {
    pub fn new(config: DocumentBridgeConfig, event_bus: Arc<BroadcastEventBus>) -> Self {
        Self {
            registry: ActiveDocumentRegistry::default(),
            event_bus,
            config,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn registry(&self) -> ActiveDocumentRegistry {
        self.registry.clone()
    }

    pub async fn list(&self) -> Vec<ActiveDocumentDescriptor> {
        self.registry.list().await
    }

    pub async fn get(&self, document_id: &str) -> Option<ActiveDocumentDescriptor> {
        self.registry.get(document_id).await
    }

    pub async fn register(
        &self,
        registration: DocumentRegistration,
        sender: mpsc::Sender<CoreMessage>,
    ) -> Result<RegisteredSession, DocumentBridgeError> {
        let document_id = registration.document_id.clone();
        let (session, previous) = self.registry.register(registration, sender).await?;
        if previous.is_some() {
            self.fail_pending_for_document(
                &document_id,
                DocumentBridgeError::Disconnected(document_id.clone()),
            )
            .await;
        }
        self.publish(
            "document.registered",
            serde_json::json!({
                "documentId": session.document_id,
                "version": session.version,
                "sessionId": session.session_id,
            }),
        );
        Ok(session)
    }

    pub async fn send_registered(
        &self,
        session: &RegisteredSession,
    ) -> Result<(), DocumentBridgeError> {
        self.send_to(
            &session.document_id,
            CoreMessage::Registered {
                document_id: session.document_id.clone(),
                session_id: session.session_id.clone(),
                version: session.version,
            },
        )
        .await
    }

    pub async fn version_changed(
        &self,
        document_id: &str,
        session_id: &str,
        version: u64,
    ) -> Result<VersionUpdate, DocumentBridgeError> {
        let update = self
            .registry
            .update_version(document_id, session_id, version)
            .await?;
        if let VersionUpdate::Changed(version) = update {
            self.publish(
                "document.versionChanged",
                serde_json::json!({
                    "documentId": document_id,
                    "version": version,
                }),
            );
        }
        Ok(update)
    }

    pub async fn unregister(
        &self,
        document_id: &str,
        session_id: &str,
    ) -> Result<(), DocumentBridgeError> {
        let descriptor = self.registry.unregister(document_id, session_id).await?;
        if let Some(descriptor) = descriptor {
            self.fail_pending_for_session(
                document_id,
                session_id,
                DocumentBridgeError::Disconnected(document_id.to_owned()),
            )
            .await;
            self.publish(
                "document.unregistered",
                serde_json::json!({
                    "documentId": descriptor.document_id,
                    "version": descriptor.version,
                    "sessionId": descriptor.session_id,
                }),
            );
        }
        Ok(())
    }

    pub async fn inspect_active_document(
        &self,
        document_id: &str,
    ) -> Result<DocumentInspection, DocumentBridgeError> {
        let response = self.request(document_id, CoreMessageKind::Inspect).await?;
        match response {
            RendererResponse::Inspection { inspection }
                if inspection.document_id == document_id
                    && inspection.authority.document_id == document_id =>
            {
                Ok(inspection)
            }
            RendererResponse::Inspection { .. } => Err(DocumentBridgeError::InvalidResponse(
                "inspection document identity mismatch".to_owned(),
            )),
            RendererResponse::Error { code, message } => Err(DocumentBridgeError::InvalidResponse(
                format!("{code}: {message}"),
            )),
            RendererResponse::Mutation { .. } => Err(DocumentBridgeError::InvalidResponse(
                "mutation response received for inspection request".to_owned(),
            )),
        }
    }

    pub async fn commit_approved_change_set(
        &self,
        change_set: DocumentChangeSet,
    ) -> Result<DocumentMutationResult, DocumentBridgeError> {
        if change_set.status != "approved" {
            return Err(DocumentBridgeError::InvalidChangeSet(
                "only approved change sets may cross document bridge".to_owned(),
            ));
        }
        if change_set.target.kind != GENOFFICE_ACTIVE_AUTHORITY
            || change_set.target.write_authority != "genoffice"
        {
            return Err(DocumentBridgeError::InvalidChangeSet(
                "change set target authority is not GenOffice active".to_owned(),
            ));
        }
        let document_id = change_set.target.document_id.clone();
        let response = self
            .request(&document_id, CoreMessageKind::Commit(change_set))
            .await?;
        match response {
            RendererResponse::Mutation { result } => {
                if let DocumentMutationResult::Applied { new_version, .. } = &result {
                    if let Ok((session_id, _)) = self.registry.sender_for(&document_id).await {
                        let _ = self
                            .version_changed(&document_id, &session_id, *new_version)
                            .await;
                    }
                }
                Ok(result)
            }
            RendererResponse::Error { code, message } => Err(DocumentBridgeError::InvalidResponse(
                format!("{code}: {message}"),
            )),
            RendererResponse::Inspection { .. } => Err(DocumentBridgeError::InvalidResponse(
                "inspection response received for mutation request".to_owned(),
            )),
        }
    }

    async fn request(
        &self,
        document_id: &str,
        kind: CoreMessageKind,
    ) -> Result<RendererResponse, DocumentBridgeError> {
        let (session_id, sender) = self.registry.sender_for(document_id).await?;
        let request_id = Uuid::new_v4().to_string();
        let (response_sender, response_receiver) = oneshot::channel();
        self.pending.lock().await.insert(
            request_id.clone(),
            PendingRequest {
                document_id: document_id.to_owned(),
                session_id,
                sender: response_sender,
            },
        );
        let message = match kind {
            CoreMessageKind::Inspect => CoreMessage::Inspect {
                request_id: request_id.clone(),
                document_id: document_id.to_owned(),
            },
            CoreMessageKind::Commit(change_set) => CoreMessage::CommitApprovedChangeSet {
                request_id: request_id.clone(),
                document_id: document_id.to_owned(),
                change_set,
            },
        };
        if sender.send(message).await.is_err() {
            self.pending.lock().await.remove(&request_id);
            return Err(DocumentBridgeError::Disconnected(document_id.to_owned()));
        }
        match tokio::time::timeout(self.config.request_timeout, response_receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(DocumentBridgeError::Disconnected(document_id.to_owned())),
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                Err(DocumentBridgeError::Timeout(request_id))
            }
        }
    }

    pub async fn handle_response(
        &self,
        session_id: &str,
        response: DocumentBridgeResponse,
    ) -> Result<(), DocumentBridgeError> {
        let pending = self.pending.lock().await.remove(&response.request_id);
        let Some(pending) = pending else {
            return Err(DocumentBridgeError::InvalidResponse(
                "unknown or expired request id".to_owned(),
            ));
        };
        if pending.document_id != response.document_id || pending.session_id != session_id {
            let _ = pending
                .sender
                .send(Err(DocumentBridgeError::InvalidResponse(
                    "response document or session does not own request".to_owned(),
                )));
            return Err(DocumentBridgeError::InvalidResponse(
                "response document or session does not own request".to_owned(),
            ));
        }
        let _ = pending.sender.send(Ok(response.response));
        Ok(())
    }

    async fn send_to(
        &self,
        document_id: &str,
        message: CoreMessage,
    ) -> Result<(), DocumentBridgeError> {
        let (_, sender) = self.registry.sender_for(document_id).await?;
        sender
            .send(message)
            .await
            .map_err(|_| DocumentBridgeError::Disconnected(document_id.to_owned()))
    }

    async fn fail_pending_for_document(&self, document_id: &str, error: DocumentBridgeError) {
        let mut pending = self.pending.lock().await;
        let ids: Vec<_> = pending
            .iter()
            .filter(|(_, request)| request.document_id == document_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(request) = pending.remove(&id) {
                let _ = request.sender.send(Err(error.clone()));
            }
        }
    }

    async fn fail_pending_for_session(
        &self,
        document_id: &str,
        session_id: &str,
        error: DocumentBridgeError,
    ) {
        let mut pending = self.pending.lock().await;
        let ids: Vec<_> = pending
            .iter()
            .filter(|(_, request)| {
                request.document_id == document_id && request.session_id == session_id
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(request) = pending.remove(&id) {
                let _ = request.sender.send(Err(error.clone()));
            }
        }
    }

    fn publish(&self, name: &str, payload: Value) {
        let _ = self
            .event_bus
            .publish(nineprofs_api_types::EventEnvelope::new(name, payload));
    }

    fn authenticate(&self, auth: Option<BridgeAuthentication>) -> Result<(), DocumentBridgeError> {
        match &self.config.session_secret {
            Some(secret)
                if auth
                    .as_ref()
                    .and_then(|value| value.session_secret.as_deref())
                    != Some(secret) =>
            {
                Err(DocumentBridgeError::Authentication)
            }
            Some(_) | None => Ok(()),
        }
    }
}

enum CoreMessageKind {
    Inspect,
    Commit(DocumentChangeSet),
}

pub fn websocket_upgrade(ws: WebSocketUpgrade, service: Arc<DocumentBridgeService>) -> Response {
    ws.on_upgrade(move |socket| serve_socket(socket, service))
}

async fn serve_socket(socket: WebSocket, service: Arc<DocumentBridgeService>) {
    let (mut sender, mut receiver) = socket.split();
    let Some(first) = receiver.next().await else {
        return;
    };
    let Ok(Message::Text(first)) = first else {
        return;
    };
    let Ok(message) = serde_json::from_str::<RendererMessage>(&first) else {
        let _ = send_error(
            &mut sender,
            None,
            "malformed_message",
            "invalid register message",
        )
        .await;
        return;
    };
    let RendererMessage::Register {
        protocol_version,
        document_id,
        document_type,
        version,
        capabilities,
        auth,
    } = message
    else {
        let _ = send_error(
            &mut sender,
            None,
            "registration_required",
            "first message must register",
        )
        .await;
        return;
    };
    if let Err(error) = service.authenticate(auth) {
        let _ = send_error(
            &mut sender,
            None,
            "authentication_failed",
            &error.to_string(),
        )
        .await;
        return;
    }
    if document_type != DOCX_DOCUMENT_TYPE {
        let _ = send_error(
            &mut sender,
            None,
            "unsupported_document_type",
            "only docx is supported",
        )
        .await;
        return;
    }
    let (outbound_sender, mut outbound_receiver) = mpsc::channel(service.config.channel_capacity);
    let session = match service
        .register(
            DocumentRegistration {
                protocol_version,
                document_id: document_id.clone(),
                document_type,
                version,
                capabilities,
            },
            outbound_sender,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            let _ = send_error(&mut sender, None, "registration_failed", &error.to_string()).await;
            return;
        }
    };
    if service.send_registered(&session).await.is_err() {
        let _ = service
            .unregister(&session.document_id, &session.session_id)
            .await;
        return;
    }

    loop {
        tokio::select! {
            outbound = outbound_receiver.recv() => match outbound {
                Some(message) => {
                    let Ok(payload) = serde_json::to_string(&message) else { break };
                    if sender.send(Message::Text(payload.into())).await.is_err() { break; }
                }
                None => break,
            },
            inbound = receiver.next() => match inbound {
                Some(Ok(Message::Text(payload))) => {
                    let Ok(message) = serde_json::from_str::<RendererMessage>(&payload) else {
                        let _ = send_error(&mut sender, None, "malformed_message", "invalid document bridge message").await;
                        break;
                    };
                    let result = match message {
                        RendererMessage::VersionChanged { document_id, version } => service.version_changed(&document_id, &session.session_id, version).await.map(|_| ()),
                        RendererMessage::Response { request_id, document_id, response } => service.handle_response(&session.session_id, DocumentBridgeResponse { request_id, document_id, response }).await,
                        RendererMessage::Unregister { document_id } => service.unregister(&document_id, &session.session_id).await.map(|_| ()),
                        RendererMessage::Register { .. } => Err(DocumentBridgeError::InvalidResponse("duplicate register message".to_owned())),
                    };
                    if let Err(error) = result {
                        service.publish("document.bridgeError", serde_json::json!({
                            "documentId": session.document_id,
                            "code": "bridge_message_error",
                            "message": error.to_string(),
                        }));
                        if matches!(error, DocumentBridgeError::Authentication | DocumentBridgeError::Registry(RegistryError::StaleSession(_))) {
                            break;
                        }
                    }
                }
                Some(Ok(Message::Ping(payload))) => {
                    if sender.send(Message::Pong(payload)).await.is_err() { break; }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Binary(_))) | Some(Ok(Message::Pong(_))) => {}
                Some(Err(_)) => break,
            },
        }
    }
    let _ = service
        .unregister(&session.document_id, &session.session_id)
        .await;
}

async fn send_error<S>(
    sender: &mut S,
    request_id: Option<String>,
    code: &str,
    message: &str,
) -> Result<(), S::Error>
where
    S: SinkExt<Message> + Unpin,
{
    let payload = serde_json::to_string(&CoreMessage::Error {
        request_id,
        code: code.to_owned(),
        message: message.to_owned(),
    })
    .expect("core bridge error is serializable");
    sender.send(Message::Text(payload.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

    fn service(timeout: Duration) -> Arc<DocumentBridgeService> {
        Arc::new(DocumentBridgeService::new(
            DocumentBridgeConfig {
                request_timeout: timeout,
                ..Default::default()
            },
            Arc::new(BroadcastEventBus::new(32)),
        ))
    }

    async fn registered(
        service: &DocumentBridgeService,
        document_id: &str,
        version: u64,
    ) -> (RegisteredSession, mpsc::Receiver<CoreMessage>) {
        let (sender, receiver) = mpsc::channel(8);
        let session = service
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: document_id.to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version,
                    capabilities: vec![DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned()],
                },
                sender,
            )
            .await
            .unwrap();
        (session, receiver)
    }

    fn change_set(document_id: &str, base_version: u64) -> DocumentChangeSet {
        DocumentChangeSet {
            id: "change-1".to_owned(),
            status: "approved".to_owned(),
            target: DocumentChangeTarget {
                kind: GENOFFICE_ACTIVE_AUTHORITY.to_owned(),
                document_id: document_id.to_owned(),
                write_authority: "genoffice".to_owned(),
            },
            base_version,
            changes: vec![DocumentChange {
                change_type: "docs.commandEnvelope".to_owned(),
                payload: Some(json!({"commands": [{"replaceAllText": {}}]})),
            }],
            approval: Some(json!({"approvedBy": "test"})),
        }
    }

    #[tokio::test]
    async fn registry_registers_lists_updates_rejects_lower_versions_and_reconnects() {
        let service = service(Duration::from_secs(1));
        let (first, _receiver) = registered(&service, "doc-a", 5).await;
        let (other, _receiver) = registered(&service, "doc-b", 0).await;
        assert_eq!(service.list().await.len(), 2);
        assert_eq!(service.list().await[0].document_id, "doc-a");
        assert_eq!(service.get("doc-a").await.unwrap().version, 5);
        assert_eq!(
            service
                .version_changed("doc-a", &first.session_id, 6)
                .await
                .unwrap(),
            VersionUpdate::Changed(6)
        );

        let (sender, _receiver) = mpsc::channel(8);
        let error = service
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "doc-a".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 4,
                    capabilities: vec![],
                },
                sender,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DocumentBridgeError::Registry(RegistryError::LowerVersion { .. })
        ));

        let (sender, _receiver) = mpsc::channel(8);
        let second = service
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "doc-a".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 6,
                    capabilities: vec![],
                },
                sender,
            )
            .await
            .unwrap();
        assert_ne!(first.session_id, second.session_id);
        assert!(matches!(
            service.version_changed("doc-a", &first.session_id, 7).await,
            Err(DocumentBridgeError::Registry(RegistryError::StaleSession(
                _
            )))
        ));
        service
            .unregister("doc-a", &second.session_id)
            .await
            .unwrap();
        service
            .unregister("doc-b", &other.session_id)
            .await
            .unwrap();
        assert!(service.list().await.is_empty());
    }

    #[tokio::test]
    async fn lifecycle_events_cover_registered_version_changed_and_unregistered() {
        let bus = Arc::new(BroadcastEventBus::new(8));
        let mut events = bus.subscribe();
        let service = Arc::new(DocumentBridgeService::new(
            DocumentBridgeConfig::default(),
            bus,
        ));
        let (session, _receiver) = {
            let (sender, receiver) = mpsc::channel(8);
            let session = service
                .register(
                    DocumentRegistration {
                        protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                        document_id: "doc-events".to_owned(),
                        document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                        version: 0,
                        capabilities: vec![],
                    },
                    sender,
                )
                .await
                .unwrap();
            (session, receiver)
        };
        assert_eq!(events.recv().await.unwrap().name, "document.registered");
        service
            .version_changed("doc-events", &session.session_id, 1)
            .await
            .unwrap();
        assert_eq!(events.recv().await.unwrap().name, "document.versionChanged");
        service
            .unregister("doc-events", &session.session_id)
            .await
            .unwrap();
        assert_eq!(events.recv().await.unwrap().name, "document.unregistered");
    }

    #[tokio::test]
    async fn configured_session_secret_is_checked_only_at_handshake() {
        let service = DocumentBridgeService::new(
            DocumentBridgeConfig {
                session_secret: Some(Arc::from("test-secret")),
                ..Default::default()
            },
            Arc::new(BroadcastEventBus::new(8)),
        );
        assert!(service.authenticate(None).is_err());
        assert!(
            service
                .authenticate(Some(BridgeAuthentication {
                    session_secret: Some("wrong".to_owned()),
                }))
                .is_err()
        );
        assert!(
            service
                .authenticate(Some(BridgeAuthentication {
                    session_secret: Some("test-secret".to_owned()),
                }))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn concurrent_requests_correlate_and_wrong_document_response_fails() {
        let service = service(Duration::from_secs(1));
        let (session, mut receiver) = registered(&service, "doc-a", 5).await;
        let first = {
            let service = service.clone();
            tokio::spawn(async move { service.inspect_active_document("doc-a").await })
        };
        let second = {
            let service = service.clone();
            tokio::spawn(async move { service.inspect_active_document("doc-a").await })
        };
        let mut ids = Vec::new();
        for _ in 0..2 {
            ids.push(match receiver.recv().await.unwrap() {
                CoreMessage::Inspect { request_id, .. } => request_id,
                other => panic!("unexpected message: {other:?}"),
            });
        }
        service
            .handle_response(
                &session.session_id,
                DocumentBridgeResponse {
                    request_id: ids[0].clone(),
                    document_id: "wrong-doc".to_owned(),
                    response: RendererResponse::Inspection {
                        inspection: DocumentInspection {
                            document_id: "wrong-doc".to_owned(),
                            authority: DocumentAuthority {
                                kind: GENOFFICE_ACTIVE_AUTHORITY.to_owned(),
                                document_id: "wrong-doc".to_owned(),
                                write_authority: "genoffice".to_owned(),
                            },
                            version: 5,
                            value: DocumentInspectionValue {
                                context: json!({}),
                                selection: json!({}),
                            },
                        },
                    },
                },
            )
            .await
            .unwrap_err();
        service
            .handle_response(
                &session.session_id,
                DocumentBridgeResponse {
                    request_id: ids[1].clone(),
                    document_id: "doc-a".to_owned(),
                    response: RendererResponse::Inspection {
                        inspection: DocumentInspection {
                            document_id: "doc-a".to_owned(),
                            authority: DocumentAuthority {
                                kind: GENOFFICE_ACTIVE_AUTHORITY.to_owned(),
                                document_id: "doc-a".to_owned(),
                                write_authority: "genoffice".to_owned(),
                            },
                            version: 5,
                            value: DocumentInspectionValue {
                                context: json!({"text": "ok"}),
                                selection: json!({"from": 1}),
                            },
                        },
                    },
                },
            )
            .await
            .unwrap();
        assert!(first.await.unwrap().is_err());
        assert_eq!(second.await.unwrap().unwrap().version, 5);
    }

    #[tokio::test]
    async fn timeout_and_disconnect_cleanup_pending_requests() {
        let service = service(Duration::from_millis(10));
        let (session, _receiver) = registered(&service, "doc-a", 1).await;
        assert!(matches!(
            service.inspect_active_document("doc-a").await,
            Err(DocumentBridgeError::Timeout(_))
        ));
        let (session, _receiver) = registered(&service, "doc-a", 1).await;
        let service_for_request = service.clone();
        let pending =
            tokio::spawn(async move { service_for_request.inspect_active_document("doc-a").await });
        tokio::task::yield_now().await;
        service
            .unregister("doc-a", &session.session_id)
            .await
            .unwrap();
        assert!(matches!(
            pending.await.unwrap(),
            Err(DocumentBridgeError::Disconnected(_))
        ));
    }

    #[tokio::test]
    async fn mutation_domain_preserves_applied_and_conflict_results() {
        let service = service(Duration::from_secs(1));
        let (session, mut receiver) = registered(&service, "doc-a", 5).await;
        let mutation = {
            let service = service.clone();
            tokio::spawn(async move {
                service
                    .commit_approved_change_set(change_set("doc-a", 5))
                    .await
            })
        };
        let request_id = match receiver.recv().await.unwrap() {
            CoreMessage::CommitApprovedChangeSet { request_id, .. } => request_id,
            other => panic!("unexpected message: {other:?}"),
        };
        service
            .handle_response(
                &session.session_id,
                DocumentBridgeResponse {
                    request_id,
                    document_id: "doc-a".to_owned(),
                    response: RendererResponse::Mutation {
                        result: DocumentMutationResult::Applied {
                            change_set_id: "change-1".to_owned(),
                            document_id: "doc-a".to_owned(),
                            previous_version: 5,
                            new_version: 6,
                            command_count: 1,
                            changed_count: 1,
                        },
                    },
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            mutation.await.unwrap().unwrap(),
            DocumentMutationResult::Applied { new_version: 6, .. }
        ));
        assert_eq!(service.get("doc-a").await.unwrap().version, 6);

        let stale = {
            let service = service.clone();
            tokio::spawn(async move {
                service
                    .commit_approved_change_set(change_set("doc-a", 5))
                    .await
            })
        };
        let request_id = match receiver.recv().await.unwrap() {
            CoreMessage::CommitApprovedChangeSet { request_id, .. } => request_id,
            other => panic!("unexpected message: {other:?}"),
        };
        service
            .handle_response(
                &session.session_id,
                DocumentBridgeResponse {
                    request_id,
                    document_id: "doc-a".to_owned(),
                    response: RendererResponse::Mutation {
                        result: DocumentMutationResult::Conflict {
                            change_set_id: "change-1".to_owned(),
                            document_id: "doc-a".to_owned(),
                            reason: "stale-version".to_owned(),
                            base_version: 5,
                            current_version: 6,
                        },
                    },
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            stale.await.unwrap().unwrap(),
            DocumentMutationResult::Conflict {
                current_version: 6,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn local_websocket_bridge_registers_inspects_versions_mutates_and_unregisters() {
        let service = service(Duration::from_secs(1));
        let app = Router::new().route(
            "/ws/documents",
            get({
                let service = service.clone();
                move |upgrade: WebSocketUpgrade| async move { websocket_upgrade(upgrade, service) }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (mut socket, _) = connect_async(format!("ws://{address}/ws/documents"))
            .await
            .unwrap();
        let register_payload = json!({
            "type": "register",
            "protocolVersion": "1",
            "documentId": "doc-a",
            "documentType": "docx",
            "version": 5,
            "capabilities": ["inspect", "commitApprovedChangeSet"]
        });
        let register_result = serde_json::from_value::<RendererMessage>(register_payload.clone());
        assert!(
            register_result.is_ok(),
            "{register_payload}: {register_result:?}"
        );
        socket
            .send(WsMessage::Text(register_payload.to_string().into()))
            .await
            .unwrap();
        let registered = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&registered).unwrap()["type"],
            "registered",
            "{registered}",
        );

        let service_for_inspect = service.clone();
        let inspect =
            tokio::spawn(async move { service_for_inspect.inspect_active_document("doc-a").await });
        let inspect_message = socket.next().await.unwrap().unwrap().into_text().unwrap();
        let inspect_json: Value = serde_json::from_str(&inspect_message).unwrap();
        assert_eq!(inspect_json["type"], "inspect");
        socket.send(WsMessage::Text(serde_json::json!({
            "type": "response",
            "requestId": inspect_json["requestId"],
            "documentId": "doc-a",
            "response": {"kind": "inspection", "inspection": {
                "documentId": "doc-a",
                "authority": {"kind": "genoffice-active", "documentId": "doc-a", "writeAuthority": "genoffice"},
                "version": 5,
                "value": {"context": {"text": "hello"}, "selection": {"from": 1, "to": 1, "empty": true}}
            }}
        }).to_string().into())).await.unwrap();
        assert_eq!(inspect.await.unwrap().unwrap().version, 5);

        socket
            .send(WsMessage::Text(
                serde_json::json!({
                    "type": "versionChanged", "documentId": "doc-a", "version": 6
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if service.get("doc-a").await.unwrap().version == 6 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(service.get("doc-a").await.unwrap().version, 6);
        socket.send(WsMessage::Close(None)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(service.get("doc-a").await.is_none());
    }
}
