/**
 * Transport-neutral DTO mapping for the optional 9Profs Core HTTP boundary.
 * Rust remains an implementation detail; callers depend only on these values.
 */
import type {
  AgentBackendDescriptor,
  AgentRunRequest,
  AgentRunResponse,
  AgentRunStarted,
  AgentTask,
  AssistantId,
  CoreAssistant,
  CoreSkill,
  CoreSkillCatalog,
  CreateMcpServerInput,
  CreateAssistantInput,
  McpConnectionTest,
  McpServer,
  McpTool,
  SkillId,
  UpdateAssistantInput,
  UpdateMcpServerInput,
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
  createAgentRun(input: AgentRunRequest): Promise<AgentRunStarted>
  agentRun(id: string): Promise<AgentRunResponse>
  agentRunTasks(id: string): Promise<AgentTask[]>
  cancelAgentTask(id: string): Promise<AgentTask>
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
  websocketUrl(): string
}

export function createCoreTransport(baseUrl: string, fetcher: CoreFetch): CoreTransport {
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

  return {
    health: () => get<CoreHealth>('/api/health'),
    runtime: () => get<CoreRuntimeInfo>('/api/runtime'),
    agents: () => get<AgentBackendDescriptor[]>('/api/agents'),
    agent: (id) => get<AgentBackendDescriptor>(`/api/agents/${encodeURIComponent(id)}`),
    createAgentRun: (input) => request<AgentRunStarted>('/api/agent-runs', 'POST', input),
    agentRun: (id) => get<AgentRunResponse>(`/api/agent-runs/${encodeURIComponent(id)}`),
    agentRunTasks: (id) => get<AgentTask[]>(`/api/agent-runs/${encodeURIComponent(id)}/tasks`),
    cancelAgentTask: (id) =>
      request<AgentTask>(`/api/agent-tasks/${encodeURIComponent(id)}/cancel`, 'POST'),
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
    websocketUrl: () => normalizedBaseUrl.replace(/^http/, 'ws') + '/ws',
  }
}
