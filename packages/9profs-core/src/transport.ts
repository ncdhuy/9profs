/**
 * Transport-neutral DTO mapping for the optional 9Profs Core HTTP boundary.
 * Rust remains an implementation detail; callers depend only on these values.
 */
import type {
  AgentBackendDescriptor,
  ActiveDocsAgentRunRequest,
  CreateDocumentAgentConversationRequest,
  CreateDocumentAgentConversationRunRequest,
  AgentRunRequest,
  AgentRunResponse,
  AgentRunStarted,
  AgentTask,
  ActiveDocument,
  AssistantId,
  CoreAssistant,
  CoreSkill,
  CoreSkillCatalog,
  CreateMcpServerInput,
  CreateAssistantInput,
  DocsAgentProfile,
  DocumentAgentConversation,
  McpConnectionTest,
  McpServer,
  McpTool,
  SkillId,
  UpdateAssistantInput,
  UpdateMcpServerInput,
  DocumentProposal,
  ClaimEvidenceLink,
  CaptureResearchPdfEvidenceInput,
  CaptureResearchPdfExtractionInput,
  CaptureResearchSourceSnapshotInput,
  CreateClaimEvidenceLinkInput,
  CreateResearchCaseInput,
  CreateResearchClaimInput,
  CreateResearchEvidenceInput,
  CreateResearchSourceInput,
  ResearchCase,
  ResearchCaseId,
  ResearchClaim,
  ResearchEvidence,
  ResearchPdfExtractionId,
  ResearchPdfExtraction,
  ResearchPdfPage,
  ResearchPdfPageList,
  ResearchPdfPageListOptions,
  ResearchSource,
  ResearchSourceSnapshot,
  ReferencePdfIngestion,
  ResearchSourceSnapshotId,
} from './types'

export interface CoreResponse<T> {
  success: boolean
  data?: T
  message?: string
}

export interface CoreHealth {
  status: 'ok'
  service: '9profs-core'
}

export interface CoreRuntimeInfo {
  service: '9profs-core'
  version: string
  protocol_version: string
  capabilities: string[]
}

export interface CoreRequestInit {
  method?: string
  headers?: Record<string, string>
  body?: string
  /** Binary request body for streamed artifact uploads. */
  rawBody?: Uint8Array
}

export interface CoreTransportOptions {
  readonly sessionSecret?: string
}

export type CoreFetch = (
  input: string,
  init?: CoreRequestInit,
) => Promise<{
  ok: boolean
  json(): Promise<unknown>
}>

export interface CoreTransport {
  health(): Promise<CoreHealth>
  runtime(): Promise<CoreRuntimeInfo>
  agents(): Promise<AgentBackendDescriptor[]>
  agent(id: string): Promise<AgentBackendDescriptor>
  documentAgentProfile(): Promise<DocsAgentProfile>
  createDocumentAgentConversation(
    input: CreateDocumentAgentConversationRequest,
  ): Promise<DocumentAgentConversation>
  createDocumentAgentConversationRun(
    conversationId: string,
    input: CreateDocumentAgentConversationRunRequest,
  ): Promise<AgentRunStarted>
  documentAgentConversation(conversationId: string): Promise<DocumentAgentConversation>
  createAgentRun(input: AgentRunRequest): Promise<AgentRunStarted>
  createActiveDocsAgentRun(input: ActiveDocsAgentRunRequest): Promise<AgentRunStarted>
  agentRun(id: string): Promise<AgentRunResponse>
  agentRunTasks(id: string): Promise<AgentTask[]>
  cancelAgentTask(id: string): Promise<AgentTask>
  activeDocuments(): Promise<ActiveDocument[]>
  activeDocument(id: string): Promise<ActiveDocument>
  documentProposals(documentId?: string): Promise<DocumentProposal[]>
  documentProposal(id: string): Promise<DocumentProposal>
  approveDocumentProposal(id: string, note?: string): Promise<DocumentProposal>
  rejectDocumentProposal(id: string, note?: string): Promise<DocumentProposal>
  retryDocumentProposal(id: string): Promise<DocumentProposal>
  assistants(): Promise<CoreAssistant[]>
  assistant(id: AssistantId): Promise<CoreAssistant>
  createAssistant(input: CreateAssistantInput): Promise<CoreAssistant>
  updateAssistant(id: AssistantId, input: UpdateAssistantInput): Promise<CoreAssistant>
  deleteAssistant(id: AssistantId): Promise<void>
  skills(): Promise<CoreSkillCatalog>
  skill(id: SkillId): Promise<CoreSkill>
  scanSkills(): Promise<CoreSkillCatalog>
  mcpServers(): Promise<McpServer[]>
  mcpServer(id: string): Promise<McpServer>
  createMcpServer(input: CreateMcpServerInput): Promise<McpServer>
  updateMcpServer(id: string, input: UpdateMcpServerInput): Promise<McpServer>
  deleteMcpServer(id: string): Promise<void>
  connectMcpServer(id: string): Promise<McpServer>
  disconnectMcpServer(id: string): Promise<McpServer>
  testMcpServer(id: string): Promise<McpConnectionTest>
  mcpTools(id: string): Promise<McpTool[]>
  researchCases(): Promise<ResearchCase[]>
  researchCase(id: string): Promise<ResearchCase>
  createResearchCase(input: CreateResearchCaseInput): Promise<ResearchCase>
  researchSources(researchCaseId?: string): Promise<ResearchSource[]>
  researchSource(id: string): Promise<ResearchSource>
  createResearchSource(input: CreateResearchSourceInput): Promise<ResearchSource>
  researchSnapshots(sourceId?: string): Promise<ResearchSourceSnapshot[]>
  researchSnapshot(id: string): Promise<ResearchSourceSnapshot>
  captureResearchSourceSnapshot(
    input: CaptureResearchSourceSnapshotInput,
  ): Promise<ResearchSourceSnapshot>
  ingestReferencePdf(
    researchCaseId: ResearchCaseId,
    bytes: Uint8Array,
    options?: { readonly filename?: string; readonly label?: string },
  ): Promise<ReferencePdfIngestion>
  recordResearchPdfExtraction(
    snapshotId: ResearchSourceSnapshotId,
    input: CaptureResearchPdfExtractionInput,
  ): Promise<ResearchPdfExtraction>
  /** Fetch one immutable extraction revision by its exact extraction ID. */
  researchPdfExtraction(extractionId: ResearchPdfExtractionId): Promise<ResearchPdfExtraction>
  researchPdfExtractions(snapshotId: ResearchSourceSnapshotId): Promise<ResearchPdfExtraction[]>
  /** Compatibility selector: latest by extractedAtMs DESC, extractionId DESC. */
  latestPdfExtraction(snapshotId: ResearchSourceSnapshotId): Promise<ResearchPdfExtraction>
  researchPdfPages(
    extractionId: ResearchPdfExtractionId,
    options?: ResearchPdfPageListOptions,
  ): Promise<ResearchPdfPageList>
  researchPdfPage(extractionId: ResearchPdfExtractionId, page: number): Promise<ResearchPdfPage>
  captureResearchPdfEvidence(input: CaptureResearchPdfEvidenceInput): Promise<ResearchEvidence>
  researchEvidence(researchCaseId?: string, sourceSnapshotId?: string): Promise<ResearchEvidence[]>
  researchEvidenceById(id: string): Promise<ResearchEvidence>
  createResearchEvidence(input: CreateResearchEvidenceInput): Promise<ResearchEvidence>
  researchClaims(researchCaseId?: string): Promise<ResearchClaim[]>
  researchClaim(id: string): Promise<ResearchClaim>
  createResearchClaim(input: CreateResearchClaimInput): Promise<ResearchClaim>
  claimEvidenceLinks(
    researchCaseId?: string,
    claimId?: string,
    evidenceId?: string,
  ): Promise<ClaimEvidenceLink[]>
  claimEvidenceLink(id: string): Promise<ClaimEvidenceLink>
  createClaimEvidenceLink(input: CreateClaimEvidenceLinkInput): Promise<ClaimEvidenceLink>
  websocketUrl(): string
}

export function createCoreTransport(
  baseUrl: string,
  fetcher: CoreFetch,
  options: CoreTransportOptions = {},
): CoreTransport {
  const normalizedBaseUrl = baseUrl.replace(/\/+$/, '')

  async function get<T>(path: string): Promise<T> {
    const response = await fetcher(`${normalizedBaseUrl}${path}`)
    if (!response.ok) throw new Error(`9Profs Core request failed: ${path}`)

    const body = (await response.json()) as CoreResponse<T>
    if (!body.success || body.data === undefined)
      throw new Error(`9Profs Core response failed: ${path}`)
    return body.data
  }

  async function request<T>(path: string, method: string, value?: unknown): Promise<T> {
    const response = await fetcher(`${normalizedBaseUrl}${path}`, {
      method,
      headers: value === undefined ? undefined : { 'content-type': 'application/json' },
      body: value === undefined ? undefined : JSON.stringify(value),
    })
    if (!response.ok) throw new Error(`9Profs Core request failed: ${path}`)

    const body = (await response.json()) as CoreResponse<T>
    if (!body.success || body.data === undefined)
      throw new Error(`9Profs Core response failed: ${path}`)
    return body.data
  }

  async function trustedRequest<T>(path: string, method: string, value?: unknown): Promise<T> {
    const headers: Record<string, string> = {}
    if (value !== undefined) headers['content-type'] = 'application/json'
    if (options.sessionSecret !== undefined) {
      headers['x-nineprofs-session-secret'] = options.sessionSecret
    }
    const response = await fetcher(`${normalizedBaseUrl}${path}`, {
      method,
      headers: Object.keys(headers).length === 0 ? undefined : headers,
      body: value === undefined ? undefined : JSON.stringify(value),
    })
    if (!response.ok) throw new Error(`9Profs Core request failed: ${path}`)

    const body = (await response.json()) as CoreResponse<T>
    if (!body.success || body.data === undefined)
      throw new Error(`9Profs Core response failed: ${path}`)
    return body.data
  }

  async function trustedBinaryRequest<T>(
    path: string,
    bytes: Uint8Array,
    headers: Record<string, string>,
  ): Promise<T> {
    const response = await fetcher(`${normalizedBaseUrl}${path}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/pdf',
        ...headers,
        ...(options.sessionSecret === undefined
          ? {}
          : { 'x-nineprofs-session-secret': options.sessionSecret }),
      },
      rawBody: bytes,
    })
    if (!response.ok) throw new Error(`9Profs Core request failed: ${path}`)

    const body = (await response.json()) as CoreResponse<T>
    if (!body.success || body.data === undefined)
      throw new Error(`9Profs Core response failed: ${path}`)
    return body.data
  }

  function queryPath(path: string, values: Array<[string, string | undefined]>): string {
    const query = new URLSearchParams()
    for (const [key, value] of values) if (value !== undefined) query.set(key, value)
    const encoded = query.toString()
    return encoded.length === 0 ? path : `${path}?${encoded}`
  }

  return {
    health: () => get<CoreHealth>('/api/health'),
    runtime: () => get<CoreRuntimeInfo>('/api/runtime'),
    agents: () => get<AgentBackendDescriptor[]>('/api/agents'),
    agent: (id) => get<AgentBackendDescriptor>(`/api/agents/${encodeURIComponent(id)}`),
    documentAgentProfile: () => get<DocsAgentProfile>('/api/document-agent-profile'),
    createDocumentAgentConversation: (input) =>
      trustedRequest<DocumentAgentConversation>('/api/document-agent-conversations', 'POST', {
        assistant_id: input.assistantId,
        document_id: input.documentId,
      }),
    createDocumentAgentConversationRun: (conversationId, input) =>
      trustedRequest<AgentRunStarted>(
        `/api/document-agent-conversations/${encodeURIComponent(conversationId)}/runs`,
        'POST',
        input,
      ),
    documentAgentConversation: (conversationId) =>
      get<DocumentAgentConversation>(
        `/api/document-agent-conversations/${encodeURIComponent(conversationId)}`,
      ),
    createAgentRun: (input) => request<AgentRunStarted>('/api/agent-runs', 'POST', input),
    createActiveDocsAgentRun: (input) =>
      trustedRequest<AgentRunStarted>('/api/document-agent-runs', 'POST', {
        assistant_id: input.assistantId,
        document_id: input.documentId,
        input: input.input,
      }),
    agentRun: (id) => get<AgentRunResponse>(`/api/agent-runs/${encodeURIComponent(id)}`),
    agentRunTasks: (id) => get<AgentTask[]>(`/api/agent-runs/${encodeURIComponent(id)}/tasks`),
    cancelAgentTask: (id) =>
      request<AgentTask>(`/api/agent-tasks/${encodeURIComponent(id)}/cancel`, 'POST'),
    activeDocuments: () => get<ActiveDocument[]>('/api/documents'),
    activeDocument: (id) => get<ActiveDocument>(`/api/documents/${encodeURIComponent(id)}`),
    documentProposals: (documentId) =>
      get<DocumentProposal[]>(
        documentId === undefined
          ? '/api/document-proposals'
          : `/api/document-proposals?documentId=${encodeURIComponent(documentId)}`,
      ),
    documentProposal: (id) =>
      get<DocumentProposal>(`/api/document-proposals/${encodeURIComponent(id)}`),
    approveDocumentProposal: (id, note) =>
      trustedRequest<DocumentProposal>(
        `/api/document-proposals/${encodeURIComponent(id)}/approve`,
        'POST',
        note === undefined ? undefined : { note },
      ),
    rejectDocumentProposal: (id, note) =>
      trustedRequest<DocumentProposal>(
        `/api/document-proposals/${encodeURIComponent(id)}/reject`,
        'POST',
        note === undefined ? undefined : { note },
      ),
    retryDocumentProposal: (id) =>
      trustedRequest<DocumentProposal>(
        `/api/document-proposals/${encodeURIComponent(id)}/retry`,
        'POST',
      ),
    assistants: () => get<CoreAssistant[]>('/api/assistants'),
    assistant: (id) => get<CoreAssistant>(`/api/assistants/${encodeURIComponent(id)}`),
    createAssistant: (input) => request<CoreAssistant>('/api/assistants', 'POST', input),
    updateAssistant: (id, input) =>
      request<CoreAssistant>(`/api/assistants/${encodeURIComponent(id)}`, 'PUT', input),
    deleteAssistant: async (id) => {
      await request<unknown>(`/api/assistants/${encodeURIComponent(id)}`, 'DELETE')
    },
    skills: () => get<CoreSkillCatalog>('/api/skills'),
    skill: (id) => get<CoreSkill>(`/api/skills/${encodeURIComponent(id)}`),
    scanSkills: () => request<CoreSkillCatalog>('/api/skills/scan', 'POST'),
    mcpServers: () => get<McpServer[]>('/api/mcp/servers'),
    mcpServer: (id) => get<McpServer>(`/api/mcp/servers/${encodeURIComponent(id)}`),
    createMcpServer: (input) => request<McpServer>('/api/mcp/servers', 'POST', input),
    updateMcpServer: (id, input) =>
      request<McpServer>(`/api/mcp/servers/${encodeURIComponent(id)}`, 'PUT', input),
    deleteMcpServer: async (id) => {
      await request<unknown>(`/api/mcp/servers/${encodeURIComponent(id)}`, 'DELETE')
    },
    connectMcpServer: (id) =>
      request<McpServer>(`/api/mcp/servers/${encodeURIComponent(id)}/connect`, 'POST'),
    disconnectMcpServer: (id) =>
      request<McpServer>(`/api/mcp/servers/${encodeURIComponent(id)}/disconnect`, 'POST'),
    testMcpServer: (id) =>
      request<McpConnectionTest>(`/api/mcp/servers/${encodeURIComponent(id)}/test`, 'POST'),
    mcpTools: (id) => get<McpTool[]>(`/api/mcp/servers/${encodeURIComponent(id)}/tools`),
    researchCases: () => get<ResearchCase[]>('/api/research/cases'),
    researchCase: (id) => get<ResearchCase>(`/api/research/cases/${encodeURIComponent(id)}`),
    createResearchCase: (input) =>
      trustedRequest<ResearchCase>('/api/research/cases', 'POST', input),
    researchSources: (researchCaseId) =>
      get<ResearchSource[]>(
        queryPath('/api/research/sources', [['researchCaseId', researchCaseId]]),
      ),
    researchSource: (id) => get<ResearchSource>(`/api/research/sources/${encodeURIComponent(id)}`),
    createResearchSource: (input) =>
      trustedRequest<ResearchSource>('/api/research/sources', 'POST', input),
    researchSnapshots: (sourceId) =>
      get<ResearchSourceSnapshot[]>(queryPath('/api/research/snapshots', [['sourceId', sourceId]])),
    researchSnapshot: (id) =>
      get<ResearchSourceSnapshot>(`/api/research/snapshots/${encodeURIComponent(id)}`),
    captureResearchSourceSnapshot: (input) =>
      trustedRequest<ResearchSourceSnapshot>('/api/research/snapshots', 'POST', input),
    ingestReferencePdf: (researchCaseId, bytes, options = {}) =>
      trustedBinaryRequest<ReferencePdfIngestion>(
        `/api/research/cases/${encodeURIComponent(researchCaseId)}/reference-pdfs`,
        bytes,
        {
          ...(options.filename === undefined
            ? {}
            : { 'x-nineprofs-original-filename': options.filename }),
          ...(options.label === undefined ? {} : { 'x-nineprofs-source-label': options.label }),
        },
      ),
    recordResearchPdfExtraction: (snapshotId, input) =>
      trustedRequest<ResearchPdfExtraction>(
        `/api/research/snapshots/${encodeURIComponent(snapshotId)}/pdf-extraction`,
        'POST',
        input,
      ),
    researchPdfExtraction: (extractionId) =>
      get<ResearchPdfExtraction>(
        `/api/research/pdf-extractions/${encodeURIComponent(extractionId)}`,
      ),
    researchPdfExtractions: (snapshotId) =>
      get<ResearchPdfExtraction[]>(
        `/api/research/source-snapshots/${encodeURIComponent(snapshotId)}/pdf-extractions`,
      ),
    latestPdfExtraction: (snapshotId) =>
      get<ResearchPdfExtraction>(
        `/api/research/snapshots/${encodeURIComponent(snapshotId)}/pdf-extraction`,
      ),
    researchPdfPages: (extractionId, options) =>
      get<ResearchPdfPageList>(
        queryPath(`/api/research/pdf-extractions/${encodeURIComponent(extractionId)}/pages`, [
          ['startPage', options?.startPage === undefined ? undefined : String(options.startPage)],
          ['limit', options?.limit === undefined ? undefined : String(options.limit)],
        ]),
      ),
    researchPdfPage: (extractionId, page) =>
      get<ResearchPdfPage>(
        `/api/research/pdf-extractions/${encodeURIComponent(extractionId)}/pages/${page}`,
      ),
    captureResearchPdfEvidence: (input) =>
      trustedRequest<ResearchEvidence>('/api/research/pdf-evidence', 'POST', input),
    researchEvidence: (researchCaseId, sourceSnapshotId) =>
      get<ResearchEvidence[]>(
        queryPath('/api/research/evidence', [
          ['researchCaseId', researchCaseId],
          ['sourceSnapshotId', sourceSnapshotId],
        ]),
      ),
    researchEvidenceById: (id) =>
      get<ResearchEvidence>(`/api/research/evidence/${encodeURIComponent(id)}`),
    createResearchEvidence: (input) =>
      trustedRequest<ResearchEvidence>('/api/research/evidence', 'POST', input),
    researchClaims: (researchCaseId) =>
      get<ResearchClaim[]>(queryPath('/api/research/claims', [['researchCaseId', researchCaseId]])),
    researchClaim: (id) => get<ResearchClaim>(`/api/research/claims/${encodeURIComponent(id)}`),
    createResearchClaim: (input) =>
      trustedRequest<ResearchClaim>('/api/research/claims', 'POST', input),
    claimEvidenceLinks: (researchCaseId, claimId, evidenceId) =>
      get<ClaimEvidenceLink[]>(
        queryPath('/api/research/claim-evidence', [
          ['researchCaseId', researchCaseId],
          ['claimId', claimId],
          ['evidenceId', evidenceId],
        ]),
      ),
    claimEvidenceLink: (id) =>
      get<ClaimEvidenceLink>(`/api/research/claim-evidence/${encodeURIComponent(id)}`),
    createClaimEvidenceLink: (input) =>
      trustedRequest<ClaimEvidenceLink>('/api/research/claim-evidence', 'POST', input),
    websocketUrl: () => normalizedBaseUrl.replace(/^http/, 'ws') + '/ws',
  }
}
