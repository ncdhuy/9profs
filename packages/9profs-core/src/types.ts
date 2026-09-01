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
export type CitationVerificationRunId = string
export type CitationReviewRunId = string
export type ManuscriptClaimExtractionRunId = string
export type ManuscriptClaimExtractionItemId = string
export type ManuscriptClaimInventoryRunId = string
export type ManuscriptClaimInventoryItemId = string
export type ManuscriptClaimInventoryCoverageId = string
export type ManuscriptClaimCoverageRunId = string
export type ManuscriptClaimCoverageItemId = string
export type ManuscriptClaimCoverageTargetId = string
export type ManuscriptCitationExpectationRunId = string
export type ManuscriptCitationExpectationItemId = string
export type ManuscriptCrossClaimCandidateRunId = string
export type ManuscriptCrossClaimComparisonWindowId = string
export type ManuscriptCrossClaimCandidateId = string
export type ManuscriptCrossClaimAssessmentRunId = string
export type ManuscriptCrossClaimAssessmentItemId = string
export type ManuscriptResearchReviewRunId = string
export type ManuscriptResearchReviewClaimItemId = string
export type ManuscriptResearchReviewConsistencyItemId = string

export interface ResearchContext {
  readonly language?: string | null
  readonly researchFamilies: readonly string[]
  readonly artifactType?: string | null
  readonly academicLevel?: string | null
  readonly studyDesigns: readonly string[]
  readonly reportingGuidelines: readonly string[]
  readonly organization?: string | null
}

export interface RunManuscriptReviewInput {
  readonly documentId: string
  readonly context: ResearchContext
}

export interface ManuscriptReviewLocator {
  readonly documentId: string
  readonly version: number
  readonly blockId: string
  readonly blockOrdinal: number
  readonly docxIndex?: number | null
  readonly sectionId?: string | null
}

export interface ManuscriptReviewEvidence {
  readonly locator: ManuscriptReviewLocator
  readonly excerpt: string
}

export interface ManuscriptReviewAuthorityPackReference {
  readonly kind: 'authority_pack'
  readonly packId: string
  readonly version: string
  readonly source: Record<string, unknown>
  readonly contentPaths: readonly string[]
}

export interface ManuscriptReviewRegulationReference {
  readonly kind: 'regulation_requirement'
  readonly reference: Record<string, unknown>
}

export type ManuscriptReviewAuthorityReference =
  ManuscriptReviewAuthorityPackReference | ManuscriptReviewRegulationReference

export interface ManuscriptReviewFinding {
  readonly id: string
  readonly sourceFindingIds: readonly string[]
  readonly statement: string
  readonly explanation: string
  readonly manuscriptLocators: readonly ManuscriptReviewLocator[]
  readonly evidence: readonly ManuscriptReviewEvidence[]
  readonly authorityReferences: readonly ManuscriptReviewAuthorityReference[]
  readonly priorityRank: number
}

export interface ManuscriptReviewSummary {
  readonly taskCount: number
  readonly rawFindingCount: number
  readonly rejectedFindingCount: number
  readonly consolidatedFindingCount: number
}

export interface ManuscriptReviewResult {
  readonly documentId: string
  readonly documentVersion: number
  readonly synthesizedFindings: readonly ManuscriptReviewFinding[]
  readonly summary: ManuscriptReviewSummary
}
export type ManuscriptReferenceResolutionRunId = string
export type ManuscriptReferenceResolutionEntryId = string
export type ManuscriptReferenceResolutionCandidateId = string

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

export type ResearchSourceIdentityMethod = 'imported' | 'human_confirmed'

export interface ResearchSourceIdentity {
  readonly provider: string
  readonly externalReference: string
  readonly method: ResearchSourceIdentityMethod
  readonly assertedAtMs: number
}

export interface ResearchSourceIdentityInput {
  readonly provider: string
  readonly externalReference: string
  readonly method: ResearchSourceIdentityMethod
}

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

export type ResearchCitationOccurrenceOrigin =
  | {
      readonly kind: 'manuscript'
      readonly document_id: string
      readonly document_version: string
      readonly locator: ResearchEvidenceLocator | null
    }
  | {
      readonly kind: 'manuscript_snapshot'
      readonly source_snapshot_id: ResearchSourceSnapshotId
      readonly locator: ResearchEvidenceLocator | null
    }
  | { readonly kind: 'imported'; readonly source: string }

export type ResearchClaimEvidenceRelation =
  'supports' | 'contradicts' | 'contextualizes' | 'insufficient'
export type ResearchAssessmentMethod =
  'human' | 'deterministic_checker' | 'agent' | 'external_service'
export type ResearchCitationBindingMethod =
  'human' | 'imported' | 'deterministic_resolver' | 'agent'
export type ResearchCitationTargetResolution =
  'unresolved' | 'source_bound' | 'pdf_extraction_bound'

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
  readonly identity: ResearchSourceIdentity | null
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

export interface CitationOccurrence {
  readonly occurrenceId: string
  readonly researchCaseId: ResearchCaseId
  readonly origin: ResearchCitationOccurrenceOrigin
  readonly renderedText: string
  readonly createdAtMs: number
}

export interface CitationTarget {
  readonly targetId: string
  readonly citationOccurrenceId: string
  readonly ordinal: number
  readonly referenceKey: string
  readonly citedLocator: string | null
  readonly resolution: ResearchCitationTargetResolution
}

export type ManuscriptCitationFormat = 'word_native' | 'zotero'
export type ManuscriptCitationSyncStatus = 'running' | 'completed' | 'failed'

export interface ManuscriptCitationSyncTargetInput {
  readonly ordinal: number
  readonly referenceKey: string
  readonly citedLocator?: string | null
}

export interface ManuscriptCitationSyncCitationInput {
  readonly format: ManuscriptCitationFormat
  readonly renderedText: string
  readonly blockId: string
  readonly start: number
  readonly end: number
  readonly targets: readonly ManuscriptCitationSyncTargetInput[]
}

export interface SyncManuscriptCitationsInput {
  readonly documentId: string
  readonly documentVersion: number
  readonly citations: readonly ManuscriptCitationSyncCitationInput[]
}

export interface ManuscriptCitationSyncRun {
  readonly syncRunId: string
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly inventoryHash: ResearchContentHash
  readonly status: ManuscriptCitationSyncStatus
  readonly occurrenceCount: number
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly failureCode: string | null
}

export interface ManuscriptCitationSyncOccurrence {
  readonly syncOccurrenceId: string
  readonly syncRunId: string
  readonly ordinal: number
  readonly citationOccurrenceId: string
  readonly documentBlockId: string
  readonly start: number
  readonly end: number
  readonly format: ManuscriptCitationFormat
}

export interface ManuscriptCitationSyncTarget {
  readonly syncTargetId: string
  readonly syncOccurrenceId: string
  readonly documentTargetOrdinal: number
  readonly citationTargetId: string
}

export type ManuscriptReferenceCatalogStatus = 'running' | 'completed' | 'failed'

export interface ManuscriptReferenceWordSource {
  readonly tag: string
  readonly title: string
  readonly author: string
  readonly year: string
}

export interface ManuscriptReferenceZotero {
  readonly itemId: string | null
  readonly uris: readonly string[]
}

export interface ManuscriptReferenceCatalogRun {
  readonly catalogRunId: string
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly citationSyncRunId: string
  readonly documentId: string
  readonly documentVersion: number
  readonly catalogHash: ResearchContentHash
  readonly entryCount: number
  readonly targetMappingCount: number
  readonly status: ManuscriptReferenceCatalogStatus
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly failureCode: string | null
}

export interface ManuscriptReferenceEntry {
  readonly entryId: string
  readonly catalogRunId: string
  readonly ordinal: number
  readonly format: ManuscriptCitationFormat
  readonly referenceKey: string
  readonly descriptorHash: ResearchContentHash
  readonly wordSource: ManuscriptReferenceWordSource | null
  readonly zotero: ManuscriptReferenceZotero | null
  readonly targetCount: number
}

export interface ManuscriptReferenceTargetMapping {
  readonly mappingId: string
  readonly catalogRunId: string
  readonly referenceEntryId: string
  readonly citationOccurrenceId: string
  readonly citationTargetId: string
  readonly documentTargetOrdinal: number
}

export type ManuscriptReferenceResolutionStatus = 'running' | 'completed' | 'failed'
export type ManuscriptReferenceResolutionOutcome =
  | 'resolved_exact'
  | 'already_bound'
  | 'ambiguous_source'
  | 'ambiguous_snapshot_or_extraction'
  | 'candidate_requires_confirmation'
  | 'source_matched_but_not_verification_ready'
  | 'unresolved'
  | 'conflict_with_existing_binding'
  | 'failed'
export type ManuscriptReferenceResolutionMatchKind =
  | 'exact_zotero_item_id'
  | 'exact_zotero_uri'
  | 'reference_key_source_label'
  | 'reference_title_source_label'
  | 'mapping_integrity'

export interface ManuscriptReferenceResolutionRun {
  readonly resolutionRunId: ManuscriptReferenceResolutionRunId
  readonly researchCaseId: ResearchCaseId
  readonly catalogRunId: string
  readonly catalogHash: ResearchContentHash
  readonly sourceStateHash: ResearchContentHash
  readonly resolverPolicyVersion: string
  readonly status: ManuscriptReferenceResolutionStatus
  readonly entryCount: number
  readonly resolvedEntryCount: number
  readonly candidateEntryCount: number
  readonly unresolvedEntryCount: number
  readonly conflictEntryCount: number
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly failureCode: string | null
}

export interface ManuscriptReferenceResolutionEntry {
  readonly resolutionEntryId: ManuscriptReferenceResolutionEntryId
  readonly resolutionRunId: ManuscriptReferenceResolutionRunId
  readonly referenceEntryId: string
  readonly outcome: ManuscriptReferenceResolutionOutcome
  readonly matchKind: ManuscriptReferenceResolutionMatchKind | null
  readonly chosenSourceId: ResearchSourceId | null
  readonly chosenSourceSnapshotId: ResearchSourceSnapshotId | null
  readonly chosenExtractionId: ResearchPdfExtractionId | null
  readonly automaticBindingPermitted: boolean
  readonly candidateCount: number
}

export interface ManuscriptReferenceResolutionCandidate {
  readonly candidateId: ManuscriptReferenceResolutionCandidateId
  readonly resolutionEntryId: ManuscriptReferenceResolutionEntryId
  readonly ordinal: number
  readonly sourceId: ResearchSourceId
  readonly sourceSnapshotId: ResearchSourceSnapshotId | null
  readonly extractionId: ResearchPdfExtractionId | null
  readonly matchKind: ManuscriptReferenceResolutionMatchKind
  readonly automaticBindingPermitted: boolean
}

export interface ManuscriptReferenceWordSourceInput {
  readonly tag: string
  readonly title: string
  readonly author: string
  readonly year: string
}

export interface ManuscriptReferenceZoteroInput {
  readonly itemId?: string | null
  readonly uris: readonly string[]
}

export interface ManuscriptReferenceCatalogTargetInput {
  readonly citationTargetId: string
  readonly ordinal: number
  readonly referenceKey: string
  readonly wordSource?: ManuscriptReferenceWordSourceInput | null
  readonly zotero?: ManuscriptReferenceZoteroInput | null
}

export interface ManuscriptReferenceCatalogCitationInput {
  readonly citationOccurrenceId: string
  readonly blockId: string
  readonly start: number
  readonly end: number
  readonly format: ManuscriptCitationFormat
  readonly targets: readonly ManuscriptReferenceCatalogTargetInput[]
}

export interface SyncManuscriptReferenceCatalogInput {
  readonly documentId: string
  readonly documentVersion: number
  readonly citations: readonly ManuscriptReferenceCatalogCitationInput[]
}

export interface CitationTargetBinding {
  readonly bindingId: string
  readonly researchCaseId: ResearchCaseId
  readonly citationTargetId: string
  readonly sourceId: ResearchSourceId
  readonly sourceSnapshotId: ResearchSourceSnapshotId | null
  readonly extractionId: ResearchPdfExtractionId | null
  readonly method: ResearchCitationBindingMethod
  readonly resolution: ResearchCitationTargetResolution
  readonly pdfVerificationReady: boolean
  readonly createdAtMs: number
}

export interface ClaimCitationLink {
  readonly linkId: string
  readonly researchCaseId: ResearchCaseId
  readonly claimId: ResearchClaimId
  readonly citationOccurrenceId: string
  readonly createdAtMs: number
}

export type ManuscriptClaimExtractionStatus = 'running' | 'completed' | 'failed'
export type ManuscriptClaimExtractionCoverageStatus =
  'associated_with_claim' | 'no_verifiable_claim'

export interface ManuscriptClaimExtractionRun {
  readonly extractionRunId: ManuscriptClaimExtractionRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly citationSyncRunId: string
  readonly documentId: string
  readonly documentVersion: number
  readonly contextHash: ResearchContentHash
  readonly extractorProvider: string
  readonly extractorVersion: string
  readonly extractorModelId: string | null
  readonly extractionContractVersion: string
  readonly status: ManuscriptClaimExtractionStatus
  readonly claimCount: number
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly failureCode: string | null
}

export interface ManuscriptClaimExtractionItem {
  readonly itemId: ManuscriptClaimExtractionItemId
  readonly extractionRunId: ManuscriptClaimExtractionRunId
  readonly researchClaimId: ResearchClaimId
  readonly documentBlockId: string
  readonly sourceStart: number
  readonly sourceEnd: number
  readonly sourceExcerpt: string
  readonly sourceExcerptHash: ResearchContentHash
  readonly ordinal: number
  readonly claimText: string
  readonly citationOccurrenceIds: readonly string[]
  readonly claimCitationLinkIds: readonly string[]
}

export interface ManuscriptClaimExtractionCoverage {
  readonly coverageId: string
  readonly extractionRunId: ManuscriptClaimExtractionRunId
  readonly extractionItemId: ManuscriptClaimExtractionItemId | null
  readonly claimCitationLinkId: string | null
  readonly citationOccurrenceId: string
  readonly status: ManuscriptClaimExtractionCoverageStatus
  readonly reason: string | null
}

export interface ManuscriptClaimExtractionCitationInput {
  readonly citationOccurrenceId: string
  readonly start: number
  readonly end: number
  readonly renderedText: string
}

export interface ManuscriptClaimExtractionBlockInput {
  readonly blockId: string
  readonly text: string
  readonly citations: readonly ManuscriptClaimExtractionCitationInput[]
}

export interface CreateManuscriptClaimExtractionInput {
  readonly documentId: string
  readonly documentVersion: number
  readonly blocks: readonly ManuscriptClaimExtractionBlockInput[]
}

export type ManuscriptClaimInventoryStatus = 'running' | 'completed' | 'failed'
export type ManuscriptClaimInventoryCoverageStatus = 'processed' | 'no_claims' | 'excluded'
export type ManuscriptClaimInventoryBlockKind = 'paragraph' | 'heading' | 'list_item'
export type ClaimReviewKind =
  'external_evidence' | 'manuscript_internal' | 'interpretive' | 'non_evidentiary' | 'uncertain'

export interface ManuscriptClaimInventoryRun {
  readonly inventoryRunId: ManuscriptClaimInventoryRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly documentContextHash: ResearchContentHash
  readonly extractorProvider: string
  readonly extractorVersion: string
  readonly extractorModelId: string | null
  readonly extractionContractVersion: string
  readonly coverageContractVersion: string
  readonly coverageScope: string
  readonly coverageLimitations: readonly string[]
  readonly status: ManuscriptClaimInventoryStatus
  readonly itemCount: number
  readonly coveredBlockCount: number
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly failureCode: string | null
}

export interface ManuscriptClaimInventoryItem {
  readonly itemId: ManuscriptClaimInventoryItemId
  readonly inventoryRunId: ManuscriptClaimInventoryRunId
  readonly ordinal: number
  readonly documentBlockId: string
  readonly blockOrdinal: number
  readonly blockKind: ManuscriptClaimInventoryBlockKind
  readonly sourceStart: number
  readonly sourceEnd: number
  readonly sourceExcerpt: string
  readonly sourceExcerptHash: ResearchContentHash
  readonly claimText: string
  readonly reviewKind: ClaimReviewKind
  readonly overlappingCitationCount: number
}

export interface ManuscriptClaimInventoryCoverage {
  readonly coverageId: ManuscriptClaimInventoryCoverageId
  readonly inventoryRunId: ManuscriptClaimInventoryRunId
  readonly documentBlockId: string
  readonly blockOrdinal: number
  readonly blockKind: ManuscriptClaimInventoryBlockKind
  readonly status: ManuscriptClaimInventoryCoverageStatus
  readonly reason: string | null
}

export interface ManuscriptClaimInventoryCitationInput {
  readonly start: number
  readonly end: number
  readonly renderedText: string
}

export interface ManuscriptClaimInventoryBlockInput {
  readonly blockId: string
  readonly blockOrdinal: number
  readonly blockKind: ManuscriptClaimInventoryBlockKind
  readonly text: string
  readonly citations: readonly ManuscriptClaimInventoryCitationInput[]
}

export interface StartManuscriptClaimInventoryInput {
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly blocks: readonly ManuscriptClaimInventoryBlockInput[]
}

export interface CreateManuscriptClaimCoverageInput {
  readonly claimInventoryRunId: ManuscriptClaimInventoryRunId
  readonly citationReviewRunId: CitationReviewRunId
}

export type ManuscriptClaimCoverageRunStatus = 'running' | 'completed' | 'failed'
export type ManuscriptClaimCoverageBridgeStatus =
  | 'exact_claim_bridge'
  | 'no_citation_scoped_claim_match'
  | 'same_span_different_claim'
  | 'multiple_exact_candidates'
  | 'invalid_cross_history'
export type ManuscriptClaimCoverageStructuralCitationState =
  | 'exact_citation_linked'
  | 'citation_observed_in_claim_range'
  | 'citation_observed_in_block'
  | 'no_citation_observed_in_block'
  | 'ambiguous_claim_bridge'

export interface ManuscriptClaimCoverageRun {
  readonly coverageRunId: ManuscriptClaimCoverageRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly claimInventoryRunId: ManuscriptClaimInventoryRunId
  readonly citationReviewRunId: CitationReviewRunId
  readonly analysisContractVersion: string
  readonly coverageContractVersion: string
  readonly coverageScope: string
  readonly coverageLimitations: readonly string[]
  readonly status: ManuscriptClaimCoverageRunStatus
  readonly itemCount: number
  readonly createdAtMs: number
  readonly completedAtMs: number | null
}

export interface ManuscriptClaimCoverageItem {
  readonly coverageItemId: ManuscriptClaimCoverageItemId
  readonly coverageRunId: ManuscriptClaimCoverageRunId
  readonly inventoryItemId: ManuscriptClaimInventoryItemId
  readonly ordinal: number
  readonly bridgeStatus: ManuscriptClaimCoverageBridgeStatus
  readonly structuralCitationState: ManuscriptClaimCoverageStructuralCitationState
  readonly matchedClaimExtractionItemId: ManuscriptClaimExtractionItemId | null
  readonly matchedResearchClaimId: ResearchClaimId | null
  readonly inventoryOverlappingCitationCount: number
  readonly sameBlockCitationCount: number
  readonly claimRangeCitationCount: number
  readonly exactClaimCitationLinkCount: number
  readonly targetCount: number
  readonly supportCount: number
  readonly contradictionCount: number
  readonly contextualizeCount: number
  readonly insufficientCount: number
  readonly unverifiedCount: number
  readonly blockedCount: number
}

export interface CreateManuscriptCitationExpectationInput {
  readonly claimCoverageRunId: ManuscriptClaimCoverageRunId
}

export type CitationExpectation =
  | 'external_evidence_expected'
  | 'external_evidence_context_dependent'
  | 'manuscript_internal_support'
  | 'no_external_citation_expected'
  | 'uncertain'

export type ManuscriptCitationExpectationRunStatus = 'running' | 'completed' | 'failed'
export type CitationExpectationAssessmentStatus = 'assessed' | 'assessment_failed'
export type CoverageAttentionState =
  | 'no_coverage_attention_detected'
  | 'review_suggested'
  | 'expectation_review_needed'
  | 'assessment_unavailable'
export type CoverageAttentionReason =
  | 'expected_external_evidence_no_exact_citation_link'
  | 'ambiguous_claim_citation_bridge'
  | 'citation_verification_blocked'
  | 'citation_verification_incomplete'
  | 'citation_verification_insufficient'
  | 'citation_verification_contextualizes'
  | 'expected_external_evidence_no_supporting_verification'
  | 'contradictory_evidence_observed'
  | 'mixed_evidence_relations'
  | 'expectation_context_dependent'
  | 'expectation_uncertain'
  | 'expectation_assessment_failed'

export interface ManuscriptCitationExpectationRun {
  readonly expectationRunId: ManuscriptCitationExpectationRunId
  readonly researchCaseId: ResearchCaseId
  readonly claimCoverageRunId: ManuscriptClaimCoverageRunId
  readonly providerId: string
  readonly assessorVersion: string
  readonly modelId: string | null
  readonly expectationContractVersion: string
  readonly coverageContractVersion: string
  readonly coverageScope: string
  readonly coverageLimitations: readonly string[]
  readonly status: ManuscriptCitationExpectationRunStatus
  readonly itemCount: number
  readonly failedItemCount: number
  readonly createdAtMs: number
  readonly completedAtMs: number | null
}

export interface ManuscriptCitationExpectationItem {
  readonly expectationItemId: ManuscriptCitationExpectationItemId
  readonly expectationRunId: ManuscriptCitationExpectationRunId
  readonly coverageItemId: ManuscriptClaimCoverageItemId
  readonly inventoryItemId: ManuscriptClaimInventoryItemId
  readonly ordinal: number
  readonly claimText: string
  readonly sourceExcerpt: string
  readonly reviewKind: ClaimReviewKind
  readonly blockKind: ManuscriptClaimInventoryBlockKind
  readonly assessmentStatus: CitationExpectationAssessmentStatus
  readonly expectation: CitationExpectation | null
  readonly attention: CoverageAttentionState
  readonly attentionReasons: readonly CoverageAttentionReason[]
  readonly rationale: string | null
  readonly failureCode: string | null
}

export type ManuscriptCrossClaimCandidateRunStatus = 'running' | 'completed' | 'failed'
export type ManuscriptCrossClaimComparisonWindowStatus = 'pending' | 'processed' | 'failed'
export type ManuscriptCrossClaimCandidateKind =
  | 'potential_direct_conflict'
  | 'potential_quantitative_mismatch'
  | 'potential_direction_mismatch'
  | 'potential_modality_mismatch'
  | 'potential_causal_strength_mismatch'
  | 'potential_scope_mismatch'
  | 'potential_temporal_mismatch'
  | 'potential_definition_mismatch'
  | 'potential_duplicate_or_restatement'
  | 'other_consistency_candidate'

export interface CreateManuscriptCrossClaimCandidatesInput {
  readonly claimInventoryRunId: ManuscriptClaimInventoryRunId
}

export interface ManuscriptCrossClaimCandidateRun {
  readonly candidateRunId: ManuscriptCrossClaimCandidateRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly claimInventoryRunId: ManuscriptClaimInventoryRunId
  readonly providerId: string
  readonly modelId: string | null
  readonly discoveryImplementationVersion: string
  readonly discoveryContractVersion: string
  readonly claimCount: number
  readonly batchCount: number
  readonly expectedWindowCount: number
  readonly processedWindowCount: number
  readonly candidatePairCount: number
  readonly status: ManuscriptCrossClaimCandidateRunStatus
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly completedAtMs: number | null
}

export interface ManuscriptCrossClaimComparisonWindow {
  readonly windowId: ManuscriptCrossClaimComparisonWindowId
  readonly candidateRunId: ManuscriptCrossClaimCandidateRunId
  readonly leftBatchOrdinal: number
  readonly rightBatchOrdinal: number
  readonly sameBatch: boolean
  readonly status: ManuscriptCrossClaimComparisonWindowStatus
  readonly candidateCount: number
  readonly failureCode: string | null
}

export interface ManuscriptCrossClaimCandidate {
  readonly candidateId: ManuscriptCrossClaimCandidateId
  readonly candidateRunId: ManuscriptCrossClaimCandidateRunId
  readonly comparisonWindowId: ManuscriptCrossClaimComparisonWindowId
  readonly leftInventoryItemId: ManuscriptClaimInventoryItemId
  readonly rightInventoryItemId: ManuscriptClaimInventoryItemId
  readonly leftOrdinal: number
  readonly rightOrdinal: number
  readonly candidateKinds: readonly ManuscriptCrossClaimCandidateKind[]
  readonly rationale: string
}

export type ManuscriptCrossClaimAssessmentRunStatus = 'running' | 'completed' | 'failed'
export type CrossClaimAssessmentStatus = 'assessed' | 'assessment_failed'
export type CrossClaimConsistencyRelation =
  | 'conflict'
  | 'compatible'
  | 'qualification_or_refinement'
  | 'equivalent_or_restatement'
  | 'not_meaningfully_comparable'
  | 'insufficient_context'
export type CrossClaimDifferenceDimension =
  | 'proposition'
  | 'quantitative'
  | 'direction'
  | 'modality_or_certainty'
  | 'causal_strength'
  | 'scope_or_population'
  | 'temporal'
  | 'definition'
  | 'other'
export type CrossClaimConsistencyAttentionState =
  | 'no_internal_consistency_attention_detected'
  | 'review_suggested'
  | 'context_review_needed'
  | 'assessment_unavailable'
export type CrossClaimConsistencyAttentionReason =
  | 'assessed_internal_conflict'
  | 'quantitative_conflict_observed'
  | 'direction_conflict_observed'
  | 'modality_conflict_observed'
  | 'causal_strength_conflict_observed'
  | 'scope_conflict_observed'
  | 'temporal_conflict_observed'
  | 'definition_conflict_observed'
  | 'propositional_conflict_observed'
  | 'consistency_context_insufficient'
  | 'consistency_assessment_failed'

export interface CreateManuscriptCrossClaimAssessmentInput {
  readonly candidateRunId: ManuscriptCrossClaimCandidateRunId
}

export interface ManuscriptCrossClaimAssessmentRun {
  readonly assessmentRunId: ManuscriptCrossClaimAssessmentRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly candidateRunId: ManuscriptCrossClaimCandidateRunId
  readonly claimInventoryRunId: ManuscriptClaimInventoryRunId
  readonly providerId: string
  readonly modelId: string | null
  readonly assessorImplementationVersion: string
  readonly assessmentContractVersion: string
  readonly candidateCount: number
  readonly assessedCount: number
  readonly failedItemCount: number
  readonly conflictCount: number
  readonly compatibleCount: number
  readonly qualificationCount: number
  readonly equivalentCount: number
  readonly notComparableCount: number
  readonly insufficientContextCount: number
  readonly failedAssessmentCount: number
  readonly status: ManuscriptCrossClaimAssessmentRunStatus
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly completedAtMs: number | null
}

export interface ManuscriptCrossClaimAssessmentItem {
  readonly assessmentItemId: ManuscriptCrossClaimAssessmentItemId
  readonly assessmentRunId: ManuscriptCrossClaimAssessmentRunId
  readonly candidateId: ManuscriptCrossClaimCandidateId
  readonly leftInventoryItemId: ManuscriptClaimInventoryItemId
  readonly rightInventoryItemId: ManuscriptClaimInventoryItemId
  readonly leftOrdinal: number
  readonly rightOrdinal: number
  readonly assessmentStatus: CrossClaimAssessmentStatus
  readonly relation: CrossClaimConsistencyRelation | null
  readonly dimensions: readonly CrossClaimDifferenceDimension[]
  readonly rationale: string | null
  readonly failureCode: string | null
  readonly attention: CrossClaimConsistencyAttentionState
  readonly attentionReasons: readonly CrossClaimConsistencyAttentionReason[]
}

export interface ManuscriptClaimCoverageTarget {
  readonly coverageTargetId: ManuscriptClaimCoverageTargetId
  readonly coverageItemId: ManuscriptClaimCoverageItemId
  readonly claimCitationLinkId: string
  readonly citationOccurrenceId: string
  readonly citationTargetId: string
  readonly citationReviewItemId: string
  readonly bindingId: string | null
  readonly sourceId: ResearchSourceId | null
  readonly sourceSnapshotId: ResearchSourceSnapshotId | null
  readonly extractionId: ResearchPdfExtractionId | null
  readonly verificationRunId: CitationVerificationRunId | null
  readonly reviewStatus: CitationReviewItemStatus
  readonly failureCode: string | null
  readonly verificationStatus: CitationVerificationStatus | null
  readonly verificationFailureCode: string | null
  readonly relation: ResearchClaimEvidenceRelation | null
  readonly rationale: string | null
  readonly evidenceCount: number
  readonly evidence: readonly CitationReviewEvidence[]
}

export interface StartManuscriptClaimCoverageInput extends CreateManuscriptClaimCoverageInput {}

export type CitationVerificationStatus = 'running' | 'completed' | 'failed'

export interface CitationVerificationCandidate {
  readonly verificationRunId: CitationVerificationRunId
  readonly retrievalChunkId: string
  readonly researchSourceId: ResearchSourceId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly extractionId: ResearchPdfExtractionId
  readonly page: number
  readonly start: number
  readonly end: number
  readonly excerptHash: string
  readonly rank: number
  readonly retrievalScore: number
}

export interface CitationVerificationResult {
  readonly verificationRunId: CitationVerificationRunId
  readonly overallRelation: ResearchClaimEvidenceRelation
  readonly rationale: string
  readonly assessorProvider: string
  readonly assessorVersion: string
  readonly assessorModelId: string | null
  readonly assessmentContractVersion: string
  readonly completedAtMs: number
}

export interface CitationVerificationEvidence {
  readonly verificationRunId: CitationVerificationRunId
  readonly retrievalChunkId: string
  readonly evidenceId: ResearchEvidenceId
  readonly claimEvidenceLinkId: ClaimEvidenceLinkId
  readonly relation: ResearchClaimEvidenceRelation
}

export interface CitationVerificationRun {
  readonly runId: CitationVerificationRunId
  readonly researchCaseId: ResearchCaseId
  readonly claimCitationLinkId: string
  readonly citationTargetBindingId: string
  readonly claimId: ResearchClaimId
  readonly citationOccurrenceId: string
  readonly citationTargetId: string
  readonly sourceId: ResearchSourceId
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly extractionId: ResearchPdfExtractionId
  readonly status: CitationVerificationStatus
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly result: CitationVerificationResult | null
  readonly candidates: readonly CitationVerificationCandidate[]
  readonly evidence: readonly CitationVerificationEvidence[]
}

export type CitationReviewRunStatus = 'running' | 'completed' | 'failed'
export type CitationReviewItemStatus =
  | 'unresolved_reference'
  | 'ambiguous_reference'
  | 'reference_requires_confirmation'
  | 'source_matched_not_verification_ready'
  | 'binding_conflict'
  | 'ready_for_verification'
  | 'verification_running'
  | 'verification_completed'
  | 'verification_failed'
  | 'resolution_failed'

export interface CitationReviewRun {
  readonly reviewRunId: CitationReviewRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly citationSyncRunId: string | null
  readonly referenceCatalogRunId: string | null
  readonly referenceResolutionRunId: string | null
  readonly claimExtractionRunId: string | null
  readonly status: CitationReviewRunStatus
  readonly failureStage: string | null
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly completedAtMs: number | null
}

export interface StartManuscriptCitationReviewInput {
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly citations: readonly CitationReviewCitationInput[]
  readonly blocks: readonly CitationReviewBlockInput[]
}

export interface CitationReviewTargetInput {
  readonly ordinal: number
  readonly referenceKey: string
  readonly citedLocator?: string | null
  readonly wordSource?: ManuscriptReferenceWordSourceInput | null
  readonly zotero?: ManuscriptReferenceZoteroInput | null
}

export interface CitationReviewCitationInput {
  readonly format: ManuscriptCitationFormat
  readonly renderedText: string
  readonly blockId: string
  readonly start: number
  readonly end: number
  readonly targets: readonly CitationReviewTargetInput[]
}

export interface CitationReviewBlockCitationInput {
  readonly start: number
  readonly end: number
  readonly renderedText: string
}

export interface CitationReviewBlockInput {
  readonly blockId: string
  readonly text: string
  readonly citations: readonly CitationReviewBlockCitationInput[]
}

export interface CitationReviewCandidate {
  readonly candidateId: string
  readonly resolutionEntryId: string
  readonly ordinal: number
  readonly sourceId: ResearchSourceId
  readonly sourceLabel: string | null
  readonly sourceSnapshotId: ResearchSourceSnapshotId | null
  readonly extractionId: ResearchPdfExtractionId | null
  readonly matchKind: ManuscriptReferenceResolutionMatchKind | null
  readonly automaticBindingPermitted: boolean
}

export interface CitationReviewVerification {
  readonly verificationRunId: CitationVerificationRunId
  readonly status: CitationVerificationStatus
  readonly failureCode: string | null
  readonly relation: ResearchClaimEvidenceRelation | null
  readonly rationale: string | null
  readonly assessorProvider: string | null
  readonly assessorVersion: string | null
  readonly assessorModelId: string | null
  readonly assessmentContractVersion?: string | null
  readonly completedAtMs: number | null
}

export interface CitationReviewEvidence {
  readonly evidenceId: ResearchEvidenceId
  readonly relation: ResearchClaimEvidenceRelation
  readonly sourceSnapshotId: ResearchSourceSnapshotId
  readonly extractionId: ResearchPdfExtractionId | null
  readonly locator: ResearchEvidenceLocator
  readonly verbatimExcerpt: string
}

export interface CitationReviewItem {
  readonly itemId: string
  readonly reviewRunId: CitationReviewRunId
  readonly ordinal: number
  readonly claimId: ResearchClaimId
  readonly claimCitationLinkId: string
  readonly citationOccurrenceId: string
  readonly citationTargetId: string
  readonly referenceEntryId: string | null
  readonly resolutionEntryId: string | null
  readonly resolutionOutcome: ManuscriptReferenceResolutionOutcome | null
  readonly documentBlockId: string
  readonly start: number
  readonly end: number
  readonly renderedText: string
  readonly referenceKey: string
  readonly citedLocator: string | null
  readonly claimText: string
  readonly sourceExcerpt: string | null
  readonly bindingId: string | null
  readonly bindingMethod: ResearchCitationBindingMethod | null
  readonly sourceId: ResearchSourceId | null
  readonly sourceSnapshotId: ResearchSourceSnapshotId | null
  readonly extractionId: ResearchPdfExtractionId | null
  readonly status: CitationReviewItemStatus
  readonly failureCode: string | null
  readonly candidates: readonly CitationReviewCandidate[]
  readonly verification: CitationReviewVerification | null
  readonly evidence: readonly CitationReviewEvidence[]
}

export interface StartManuscriptResearchReviewInput {
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly citationReviewObservations: {
    readonly citations: readonly CitationReviewCitationInput[]
    readonly citationBlocks: readonly CitationReviewBlockInput[]
  }
  readonly claimInventoryObservations: {
    readonly wholeManuscriptBlocks: readonly ManuscriptClaimInventoryBlockInput[]
  }
}

export type ManuscriptResearchReviewRunStatus = 'running' | 'completed' | 'failed'

export interface ManuscriptResearchReviewSummary {
  readonly totalInventoryClaims: number
  readonly coverageReviewSuggestedCount: number
  readonly expectationReviewNeededCount: number
  readonly assessmentUnavailableCount: number
  readonly claimsWithSupportCount: number
  readonly claimsWithContradictionCount: number
  readonly claimsWithBlockedVerificationCount: number
  readonly claimsWithUnverifiedVerificationCount: number
  readonly consistencyAssessedCount: number
  readonly consistencyConflictCount: number
  readonly consistencyCompatibleCount: number
  readonly consistencyQualificationCount: number
  readonly consistencyEquivalentCount: number
  readonly consistencyNotComparableCount: number
  readonly consistencyInsufficientContextCount: number
  readonly consistencyAssessmentFailureCount: number
  readonly coverageContractVersion: string
  readonly coverageScope: string
  readonly coverageLimitations: readonly string[]
  readonly candidateClaimCount: number
  readonly candidateBatchCount: number
  readonly candidateExpectedWindowCount: number
  readonly candidateProcessedWindowCount: number
  readonly candidatePairCount: number
}

export interface ManuscriptResearchReviewRun {
  readonly reviewRunId: ManuscriptResearchReviewRunId
  readonly researchCaseId: ResearchCaseId
  readonly manuscriptSourceId: ResearchSourceId
  readonly documentId: string
  readonly documentVersion: number
  readonly inputHashAlgorithm: string
  readonly inputHash: string
  readonly executionIdentityHashAlgorithm: string | null
  readonly executionIdentityHash: string | null
  readonly citationReviewRunId: CitationReviewRunId | null
  readonly claimInventoryRunId: ManuscriptClaimInventoryRunId | null
  readonly claimCoverageRunId: ManuscriptClaimCoverageRunId | null
  readonly citationExpectationRunId: ManuscriptCitationExpectationRunId | null
  readonly crossClaimCandidateRunId: ManuscriptCrossClaimCandidateRunId | null
  readonly crossClaimAssessmentRunId: ManuscriptCrossClaimAssessmentRunId | null
  readonly reviewContractVersion: string
  readonly status: ManuscriptResearchReviewRunStatus
  readonly failureStage: string | null
  readonly failureCode: string | null
  readonly createdAtMs: number
  readonly completedAtMs: number | null
  readonly summary: ManuscriptResearchReviewSummary | null
}

export interface ManuscriptResearchReviewClaimTarget {
  readonly coverageTargetId: string
  readonly claimCitationLinkId: string
  readonly citationOccurrenceId: string
  readonly citationTargetId: string
  readonly citationReviewItemId: string
  readonly bindingId: string | null
  readonly sourceId: string | null
  readonly sourceSnapshotId: string | null
  readonly extractionId: string | null
  readonly verificationRunId: string | null
  readonly reviewStatus: CitationReviewItemStatus
  readonly failureCode: string | null
  readonly verificationStatus: CitationVerificationStatus | null
  readonly verificationFailureCode: string | null
  readonly relation: ResearchClaimEvidenceRelation | null
  readonly rationale: string | null
  readonly evidenceCount: number
  readonly evidence: readonly CitationReviewEvidence[]
  readonly citationReviewItem: CitationReviewItem
}

export interface ManuscriptResearchReviewClaimItem {
  readonly wholeReviewRunId: ManuscriptResearchReviewRunId
  readonly inventoryItemId: ManuscriptClaimInventoryItemId
  readonly ordinal: number
  readonly documentBlockId: string
  readonly blockOrdinal: number
  readonly blockKind: ManuscriptClaimInventoryBlockKind
  readonly sourceStart: number
  readonly sourceEnd: number
  readonly sourceExcerpt: string
  readonly claimText: string
  readonly claimReviewKind: ClaimReviewKind
  readonly bridgeStatus: ManuscriptClaimCoverageBridgeStatus
  readonly structuralCitationState: ManuscriptClaimCoverageStructuralCitationState
  readonly sameBlockCitationCount: number
  readonly exactClaimCitationLinkCount: number
  readonly targetCount: number
  readonly assessmentStatus: CitationExpectationAssessmentStatus
  readonly expectation: CitationExpectation | null
  readonly expectationRationale: string | null
  readonly attentionState: CoverageAttentionState
  readonly attentionReasons: readonly CoverageAttentionReason[]
  readonly supportCount: number
  readonly contradictionCount: number
  readonly contextualizeCount: number
  readonly insufficientCount: number
  readonly blockedCount: number
  readonly unverifiedCount: number
  readonly targets: readonly ManuscriptResearchReviewClaimTarget[]
}

export interface ManuscriptResearchReviewConsistencyClaim {
  readonly inventoryItemId: ManuscriptClaimInventoryItemId
  readonly ordinal: number
  readonly documentBlockId: string
  readonly blockOrdinal: number
  readonly blockKind: ManuscriptClaimInventoryBlockKind
  readonly sourceStart: number
  readonly sourceEnd: number
  readonly sourceExcerpt: string
  readonly claimText: string
  readonly claimReviewKind: ClaimReviewKind
}

export interface ManuscriptResearchReviewConsistencyItem {
  readonly wholeReviewRunId: ManuscriptResearchReviewRunId
  readonly assessmentItemId: ManuscriptCrossClaimAssessmentItemId
  readonly candidateId: ManuscriptCrossClaimCandidateId
  readonly left: ManuscriptResearchReviewConsistencyClaim
  readonly right: ManuscriptResearchReviewConsistencyClaim
  readonly assessmentStatus: CrossClaimAssessmentStatus
  readonly relation: CrossClaimConsistencyRelation | null
  readonly dimensions: readonly CrossClaimDifferenceDimension[]
  readonly rationale: string | null
  readonly failureCode: string | null
  readonly attentionState: CrossClaimConsistencyAttentionState
  readonly attentionReasons: readonly CrossClaimConsistencyAttentionReason[]
}

export interface CreateCitationVerificationInput {
  readonly claimCitationLinkId: string
  readonly citationTargetBindingId: string
}

export interface CreateResearchCaseInput {
  readonly title: string
}

export interface CreateResearchSourceInput {
  readonly researchCaseId: ResearchCaseId
  readonly kind: ResearchSourceKind
  readonly label: string
  readonly identity?: ResearchSourceIdentityInput | null
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

export interface CreateCitationOccurrenceInput {
  readonly researchCaseId: ResearchCaseId
  readonly origin: ResearchCitationOccurrenceOrigin
  readonly renderedText: string
}

export interface CreateCitationTargetInput {
  readonly citationOccurrenceId: string
  readonly ordinal: number
  readonly referenceKey: string
  readonly citedLocator?: string | null
}

export interface CreateCitationTargetBindingInput {
  readonly researchCaseId: ResearchCaseId
  readonly citationTargetId: string
  readonly sourceId: ResearchSourceId
  readonly sourceSnapshotId?: ResearchSourceSnapshotId | null
  readonly extractionId?: ResearchPdfExtractionId | null
  readonly method: ResearchCitationBindingMethod
}

export interface CreateClaimCitationLinkInput {
  readonly researchCaseId: ResearchCaseId
  readonly claimId: ResearchClaimId
  readonly citationOccurrenceId: string
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
