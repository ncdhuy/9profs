export type AgentRunId = string
export type AgentBackendId = string
export type AssistantId = string
export type SkillId = string
export type ToolId = string
export type ResearchCaseId = string
export type ResearchSourceId = string
export type ResearchSourceSnapshotId = string
export type ResearchPdfExtractionId = string
export type ResearchEvidenceId = string
export type ResearchClaimId = string
export type ClaimEvidenceLinkId = string

export interface AgentRequest {
  readonly input: string
  readonly assistantId?: AssistantId
}

export type AgentTaskStatus =
  'queued' | 'starting' | 'running' | 'succeeded' | 'failed' | 'cancelled'

export interface AgentRunRequest {
  readonly assistant_id: AssistantId
  readonly input: string
}

export interface ActiveDocsAgentRunRequest {
  readonly assistantId: AssistantId
  readonly documentId: string
  readonly input: string
}

export type DocsAgentConversationState = 'idle' | 'running' | 'unavailable'

export interface CreateDocumentAgentConversationRequest {
  readonly assistantId: AssistantId
  readonly documentId: string
}

export interface CreateDocumentAgentConversationRunRequest {
  readonly input: string
}

export interface DocumentAgentConversation {
  readonly conversationId: string
  readonly assistantId: AssistantId
  readonly documentId: string
  readonly state: DocsAgentConversationState
  readonly turnCount: number
  readonly createdAtMs: number
  readonly updatedAtMs: number
}

export type DocsAgentReadiness =
  | 'ready'
  | 'assistant_missing'
  | 'assistant_disabled'
  | 'backend_not_configured'
  | 'backend_missing'
  | 'backend_unavailable'
  | 'backend_disabled'
  | 'executor_missing'
  | 'provider_not_configured'
  | 'provider_invalid'
  | 'required_tool_missing'

export type DocsAgentAvailability =
  'not_configured' | 'missing' | 'disabled' | 'unavailable' | 'available'

export interface DocsAgentProfile {
  readonly defaultAssistantId: AssistantId
  readonly readiness: DocsAgentReadiness
  readonly reason?: string
  readonly backendId?: AgentBackendId
  readonly assistantAvailability: DocsAgentAvailability
  readonly backendAvailability: DocsAgentAvailability
  readonly providerReady: boolean
  readonly capabilities: readonly string[]
  readonly supportsActiveDocsRuns: boolean
}

export interface AgentTaskFailure {
  readonly code: string
  readonly message: string
}

export interface AgentTask {
  readonly task_id: string
  readonly run_id: AgentRunId
  readonly backend_id: AgentBackendId
  readonly state: AgentTaskStatus
  readonly created_at_ms: number
  readonly updated_at_ms: number
  readonly started_at_ms: number | null
  readonly completed_at_ms: number | null
  readonly failure: AgentTaskFailure | null
  readonly cancellation_requested: boolean
}

export interface AgentRunStarted {
  readonly run_id: AgentRunId
  readonly task: AgentTask
  readonly context?: AgentRunContext
}

export interface AgentRunResponse {
  readonly run_id: AgentRunId
  readonly tasks: readonly AgentTask[]
  readonly context?: AgentRunContext
}

export type AgentRunContext = {
  readonly kind: 'activeDocs'
  readonly documentId: string
}

export type AgentExecutionOutputEvent =
  | {
      readonly id: string
      readonly name: 'agent.outputStarted'
      readonly occurred_at_ms: number
      readonly payload: {
        readonly run_id: AgentRunId
        readonly task_id: string
        readonly details: Record<string, never>
      }
    }
  | {
      readonly id: string
      readonly name: 'agent.outputDelta'
      readonly occurred_at_ms: number
      readonly payload: {
        readonly run_id: AgentRunId
        readonly task_id: string
        readonly details: { readonly delta: string }
      }
    }
  | {
      readonly id: string
      readonly name: 'agent.outputCompleted'
      readonly occurred_at_ms: number
      readonly payload: {
        readonly run_id: AgentRunId
        readonly task_id: string
        readonly details: { readonly output: string }
      }
    }
  | {
      readonly id: string
      readonly name: 'agent.error'
      readonly occurred_at_ms: number
      readonly payload: {
        readonly run_id: AgentRunId
        readonly task_id: string
        readonly details: { readonly code: string; readonly message: string }
      }
    }
  | {
      readonly id: string
      readonly name: 'agent.toolStarted'
      readonly occurred_at_ms: number
      readonly payload: {
        readonly run_id: AgentRunId
        readonly task_id: string
        readonly details: { readonly tool_call_id: string; readonly tool: string }
      }
    }
  | {
      readonly id: string
      readonly name: 'agent.toolCompleted'
      readonly occurred_at_ms: number
      readonly payload: {
        readonly run_id: AgentRunId
        readonly task_id: string
        readonly details: {
          readonly tool_call_id: string
          readonly tool: string
          readonly is_error: boolean
        }
      }
    }

export type AgentRunStatus = 'completed' | 'failed' | 'cancelled'

export interface AgentRun {
  readonly id: AgentRunId
  readonly status: AgentRunStatus
  readonly output?: string
}

/** Implementation boundary for executing one agent request. */
export interface AgentBackend {
  run(request: AgentRequest): Promise<AgentRun>
}

export type AgentBackendSource = 'builtin' | 'custom' | 'extension'
export type AgentBackendKind = 'embedded' | 'cli' | 'remote' | 'extension'
export type AgentBackendAvailability = 'unknown' | 'available' | 'unavailable' | 'disabled'

/** Metadata/catalog boundary. This is not the executable AgentBackend contract. */
export interface AgentBackendDescriptor {
  readonly id: AgentBackendId
  readonly name: string
  readonly description: string
  readonly source: AgentBackendSource
  readonly kind: AgentBackendKind
  readonly capabilities: readonly string[]
  readonly availability: AgentBackendAvailability
  readonly availability_reason: string | null
  readonly enabled: boolean
  readonly sort_order: number
  readonly version: string | null
  readonly created_at_ms: number | null
  readonly updated_at_ms: number | null
}

export type ToolSource = 'builtin' | 'mcp' | 'officecli' | 'research' | 'extension'
export type ToolEffect = 'read' | 'write' | 'execute' | 'external_network'

export interface ToolPolicy {
  readonly effects: readonly ToolEffect[]
  readonly requires_confirmation: boolean
}

export interface ToolDefinition {
  readonly id: ToolId
  readonly name: string
  readonly description: string
  readonly input_schema: unknown
  readonly source: ToolSource
  readonly policy: ToolPolicy
  readonly enabled: boolean
}

export type ActiveDocumentAvailability = 'available' | 'unavailable'
export type DocumentProposalFreshness = 'fresh' | 'stale' | 'unavailable'
export type DocumentProposalStatus =
  'proposed' | 'applying' | 'applied' | 'conflict' | 'failed' | 'rejected'

export interface ActiveDocument {
  readonly documentId: string
  readonly documentType: string
  readonly authority: string
  readonly version: number
  readonly capabilities: readonly string[]
  readonly availability: ActiveDocumentAvailability
}

export interface DocumentProposalChange {
  readonly type: string
  readonly payload?: unknown
}

export interface DocumentProposal {
  readonly proposalId: string
  readonly changeSetId: string
  readonly documentId: string
  readonly authority: string
  readonly baseVersion: number
  readonly status: DocumentProposalStatus
  readonly freshness: DocumentProposalFreshness
  readonly availability: ActiveDocumentAvailability
  readonly currentVersion: number | null
  readonly createdAtMs: number
  readonly summary?: string
  readonly changes: readonly DocumentProposalChange[]
  readonly decision?: unknown
  readonly outcome?: unknown
  readonly failure?: string
  readonly retryable: boolean
}

/** Discovery and resolution boundary for agent tools. MCP is not part of this contract. */
export interface ToolProvider {
  listTools(): Promise<readonly ToolDefinition[]>
  resolveTool(id: ToolId): Promise<ToolDefinition | undefined>
}

export type ResearchSourceKind =
  'reference_pdf' | 'manuscript' | 'dataset' | 'web' | 'regulation' | 'other'
export type ResearchCaptureMethod =
  | 'user_provided'
  | 'uploaded_artifact'
  | 'active_document'
  | 'office_cli'
  | 'web_retrieval'
  | 'external_import'
export type ResearchHashAlgorithm = 'sha256'

export type ResearchPdfExtractionStatus =
  'ready' | 'no_extractable_text' | 'failed' | 'password_required'

export type ResearchSourceOrigin =
  | {
      readonly kind: 'uploaded_artifact'
      readonly artifact_id: string
      readonly revision_id: string | null
    }
  | {
      readonly kind: 'active_document_snapshot'
      readonly document_id: string
      readonly document_version: string
    }
  | {
      readonly kind: 'office_cli_artifact_revision'
      readonly artifact_id: string
      readonly revision_id: string
    }
  | {
      readonly kind: 'web_retrieval'
      readonly url: string
      readonly retrieved_at_ms: number
    }
  | {
      readonly kind: 'external_import'
      readonly provider: string
      readonly external_reference: string
    }

export type ResearchEvidenceLocator =
  | { readonly kind: 'text_range'; readonly start: number; readonly end: number }
  | { readonly kind: 'pdf'; readonly page: number; readonly end_page: number | null }
  | {
      readonly kind: 'pdf_text_range'
      readonly page: number
      /** Unicode scalar/code-point offsets, not UTF-8 bytes or UTF-16 indexes. */
      readonly start: number
      readonly end: number
    }
  | {
      readonly kind: 'manuscript'
      readonly block_id: string
      readonly start: number | null
      readonly end: number | null
    }
  | { readonly kind: 'spreadsheet'; readonly sheet: string; readonly range: string }
  | {
      readonly kind: 'web'
      readonly fragment: string | null
      readonly start: number | null
      readonly end: number | null
    }
  | {
      readonly kind: 'regulation'
      readonly article: string
      readonly section: string | null
      readonly clause: string | null
    }

export type ResearchClaimOrigin =
  | {
      readonly kind: 'manuscript'
      readonly document_id: string
      readonly document_version: string
      readonly locator: ResearchEvidenceLocator | null
    }
  | { readonly kind: 'user' }
  | { readonly kind: 'agent' }
  | { readonly kind: 'imported'; readonly source: string }

export type ResearchClaimEvidenceRelation =
  'supports' | 'contradicts' | 'contextualizes' | 'insufficient'
export type ResearchAssessmentMethod =
  'human' | 'deterministic_checker' | 'agent' | 'external_service'

export interface ResearchContentHash {
  readonly algorithm: ResearchHashAlgorithm
  readonly value: string
}

export interface ResearchCase {
  readonly caseId: ResearchCaseId
  readonly title: string
  readonly createdAtMs: number
  readonly updatedAtMs: number
}

export interface ResearchSource {
  readonly sourceId: ResearchSourceId
  readonly researchCaseId: ResearchCaseId
  readonly kind: ResearchSourceKind
  readonly label: string
  readonly createdAtMs: number
}

export interface ResearchSourceSnapshot {
  readonly snapshotId: ResearchSourceSnapshotId
  readonly sourceId: ResearchSourceId
  readonly contentHash: ResearchContentHash
  readonly capturedAtMs: number
  readonly captureMethod: ResearchCaptureMethod
  readonly origin: ResearchSourceOrigin
  readonly metadata: Readonly<Record<string, string>>
}

export interface ResearchArtifact {
  readonly artifactId: string
  readonly contentHash: ResearchContentHash
  readonly sizeBytes: number
  readonly mediaType: 'application/pdf'
  readonly originalFilename: string
  readonly createdAtMs: number
}

export interface ResearchPdfPageInput {
  readonly page: number
  readonly text: string
}

export interface CaptureResearchPdfExtractionInput {
  readonly extractor: string
  readonly extractorVersion?: string
  readonly pageCount: number
  readonly status: ResearchPdfExtractionStatus
  readonly pages: readonly ResearchPdfPageInput[]
}

export interface ResearchPdfExtraction {
  readonly extractionId: ResearchPdfExtractionId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly artifactId: string
  readonly extractor: string
  readonly extractorVersion: string
  readonly pageCount: number
  readonly extractionHash: ResearchContentHash
  readonly extractedAtMs: number
  readonly status: ResearchPdfExtractionStatus
}

export interface ResearchPdfPage {
  readonly extractionId: ResearchPdfExtractionId
  readonly page: number
  readonly text: string
  readonly textHash: ResearchContentHash
}

export interface ResearchPdfPageList {
  readonly data: readonly ResearchPdfPage[]
  readonly startPage: number
  readonly limit: number
  readonly hasMore: boolean
  readonly nextStartPage: number | null
}

export interface ResearchPdfPageListOptions {
  readonly startPage?: number
  readonly limit?: number
}

export type ResearchRetrievalIndexStatus =
  'not_configured' | 'provisioning' | 'ready' | 'syncing' | 'failed' | 'degraded'

export interface ResearchRetrievalReadiness {
  readonly provider: string
  readonly qualificationTarget: string
  readonly configured: boolean
  readonly status: ResearchRetrievalReadinessStatus
  readonly reachable: boolean
  readonly authorized: boolean
  readonly ready: boolean
}

export type ResearchRetrievalReadinessStatus =
  'not_configured' | 'configured' | 'unreachable' | 'reachable' | 'unauthorized' | 'ready'

export interface ResearchRetrievalIndex {
  readonly indexId: string
  readonly researchCaseId: ResearchCaseId
  readonly datasetId: string
  readonly status: ResearchRetrievalIndexStatus
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly updatedAtMs: number
}

export interface ResearchExtractionRetrievalIndex {
  readonly indexId: string
  readonly caseIndexId: string
  readonly researchCaseId: ResearchCaseId
  readonly extractionId: ResearchPdfExtractionId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly documentId: string | null
  readonly metadataQualified: boolean
  readonly chunkerVersion: string
  readonly status: ResearchRetrievalIndexStatus
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly updatedAtMs: number
}

export type ResearchRetrievalScope =
  | { readonly kind: 'case' }
  | { readonly kind: 'sources'; readonly sourceIds: readonly ResearchSourceId[] }
  | {
      readonly kind: 'extractions'
      readonly extractionIds: readonly ResearchPdfExtractionId[]
    }

export interface ResearchRetrievalIndexState {
  readonly readiness: ResearchRetrievalReadiness
  readonly caseIndex: ResearchRetrievalIndex | null
  readonly extractionIndexes: readonly ResearchExtractionRetrievalIndex[]
}

export interface ResearchRetrievalCandidate {
  readonly retrievalChunkId: string
  readonly researchSourceId: ResearchSourceId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly extractionId: ResearchPdfExtractionId
  readonly page: number
  readonly start: number
  readonly end: number
  readonly verbatimExcerpt: string
  readonly retrievalScore: number
  readonly provider: string
  readonly rank: number
}

export interface RetrieveResearchInput {
  readonly query: string
  readonly topK?: number
  readonly scope?: ResearchRetrievalScope
}

export interface ReferencePdfIngestion {
  readonly artifact: ResearchArtifact
  readonly source: ResearchSource
  readonly snapshot: ResearchSourceSnapshot
}

export interface CaptureResearchPdfEvidenceInput {
  readonly researchCaseId: ResearchCaseId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly extractionId: ResearchPdfExtractionId
  readonly page: number
  readonly start: number
  readonly end: number
}

export interface ResearchEvidence {
  readonly evidenceId: ResearchEvidenceId
  readonly researchCaseId: ResearchCaseId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly verbatimExcerpt: string
  readonly normalizedText: string | null
  readonly locator: ResearchEvidenceLocator
  readonly excerptHash: ResearchContentHash
  readonly capturedAtMs: number
  readonly captureMethod: ResearchCaptureMethod
  readonly pdfExtractionId: string | null
}

export interface ResearchClaim {
  readonly claimId: ResearchClaimId
  readonly researchCaseId: ResearchCaseId
  readonly text: string
  readonly origin: ResearchClaimOrigin
  readonly createdAtMs: number
}

export interface ClaimEvidenceLink {
  readonly linkId: ClaimEvidenceLinkId
  readonly researchCaseId: ResearchCaseId
  readonly claimId: ResearchClaimId
  readonly evidenceId: ResearchEvidenceId
  readonly relation: ResearchClaimEvidenceRelation
  readonly rationale: string | null
  readonly assessmentMethod: ResearchAssessmentMethod
  readonly assessmentMetadata: Readonly<Record<string, string>>
  readonly createdAtMs: number
}

export interface CreateResearchCaseInput {
  readonly title: string
}

export interface CreateResearchSourceInput {
  readonly researchCaseId: ResearchCaseId
  readonly kind: ResearchSourceKind
  readonly label: string
}

export interface CaptureResearchSourceSnapshotInput {
  readonly sourceId: ResearchSourceId
  readonly content: string
  readonly captureMethod: ResearchCaptureMethod
  readonly origin: ResearchSourceOrigin
  readonly metadata?: Readonly<Record<string, string>>
}

export interface CreateResearchEvidenceInput {
  readonly researchCaseId: ResearchCaseId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly verbatimExcerpt: string
  readonly normalizedText?: string | null
  readonly locator: ResearchEvidenceLocator
  readonly captureMethod: ResearchCaptureMethod
}

export interface CreateResearchClaimInput {
  readonly researchCaseId: ResearchCaseId
  readonly text: string
  readonly origin: ResearchClaimOrigin
}

export interface CreateClaimEvidenceLinkInput {
  readonly researchCaseId: ResearchCaseId
  readonly claimId: ResearchClaimId
  readonly evidenceId: ResearchEvidenceId
  readonly relation: ResearchClaimEvidenceRelation
  readonly rationale?: string | null
  readonly assessmentMethod: ResearchAssessmentMethod
  readonly assessmentMetadata?: Readonly<Record<string, string>>
}

export interface SkillDefinition {
  readonly id: SkillId
  readonly description: string
}

/** Discovery and resolution boundary for skills. Filesystem loading is not part of this contract. */
export interface SkillProvider {
  listSkills(): Promise<readonly SkillDefinition[]>
  resolveSkill(id: SkillId): Promise<SkillDefinition | undefined>
}

export interface AssistantDefinition {
  readonly id: AssistantId
  readonly description: string
}

export type AssistantSource = 'builtin' | 'custom'

export interface CoreAssistant {
  readonly id: AssistantId
  readonly name: string
  readonly description: string
  readonly avatar: string | null
  readonly source: AssistantSource
  readonly rules: string
  readonly enabled: boolean
  readonly skill_ids: readonly SkillId[]
  readonly backend_agent_id: string | null
  readonly created_at_ms: number | null
  readonly updated_at_ms: number | null
}

export interface CreateAssistantInput {
  readonly id?: string
  readonly name: string
  readonly description: string
  readonly avatar?: string | null
  readonly rules?: string
  readonly enabled?: boolean
  readonly skill_ids?: readonly SkillId[]
  readonly backend_agent_id?: string | null
}

export interface UpdateAssistantInput {
  readonly name?: string
  readonly description?: string
  readonly avatar?: string | null
  readonly rules?: string
  readonly enabled?: boolean
  readonly skill_ids?: readonly SkillId[]
  readonly backend_agent_id?: string | null
}

export type SkillSource = 'builtin' | 'custom' | 'extension'

export interface CoreSkill {
  readonly id: SkillId
  readonly name: string
  readonly description: string
  readonly source: SkillSource
  readonly location: string
  readonly content?: string
}

export interface CoreSkillIssue {
  readonly root: string
  readonly path?: string
  readonly message: string
}

export interface CoreSkillCatalog {
  readonly skills: readonly CoreSkill[]
  readonly issues: readonly CoreSkillIssue[]
}

export type McpServerStatus = 'disconnected' | 'connecting' | 'connected' | 'error'

export type McpTransportInput =
  | {
      readonly type: 'stdio'
      readonly command: string
      readonly args?: readonly string[]
      readonly env?: Readonly<Record<string, string>>
    }
  | {
      readonly type: 'sse'
      readonly url: string
      readonly headers?: Readonly<Record<string, string>>
    }
  | {
      readonly type: 'streamable-http'
      readonly url: string
      readonly headers?: Readonly<Record<string, string>>
    }

export type McpTransport =
  | {
      readonly type: 'stdio'
      readonly command: string
      readonly args: readonly string[]
      readonly env_keys: readonly string[]
    }
  | {
      readonly type: 'sse'
      readonly url: string
      readonly header_names: readonly string[]
    }
  | {
      readonly type: 'streamable-http'
      readonly url: string
      readonly header_names: readonly string[]
    }

export interface McpTool {
  readonly id: ToolId
  readonly name: string
  readonly display_name: string
  readonly description: string
  readonly input_schema: unknown
}

export interface McpServer {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly enabled: boolean
  readonly startup_timeout_ms: number
  readonly transport: McpTransport
  readonly status: McpServerStatus
  readonly last_connected: number | null
  readonly error: string | null
  readonly supports_resources: boolean
  readonly tools: readonly McpTool[]
  readonly created_at_ms: number
  readonly updated_at_ms: number
}

export interface CreateMcpServerInput {
  readonly id?: string
  readonly name: string
  readonly description?: string
  readonly enabled?: boolean
  readonly startup_timeout_ms?: number
  readonly transport: McpTransportInput
}

export interface UpdateMcpServerInput {
  readonly name?: string
  readonly description?: string
  readonly enabled?: boolean
  readonly startup_timeout_ms?: number
  readonly transport?: McpTransportInput
}

export interface McpConnectionTest {
  readonly success: boolean
  readonly tool_count: number
  readonly supports_resources: boolean
  readonly error: string | null
}

/** Registry boundary for assistant definitions and configuration. */
export interface AssistantRegistry {
  listAssistants(): Promise<readonly AssistantDefinition[]>
  resolveAssistant(id: AssistantId): Promise<AssistantDefinition | undefined>
}
