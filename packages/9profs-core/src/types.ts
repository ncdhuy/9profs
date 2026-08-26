export type AgentRunId = string
export type AgentBackendId = string
export type AssistantId = string
export type SkillId = string
export type ToolId = string

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
