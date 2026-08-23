export type AgentRunId = string
export type AssistantId = string
export type SkillId = string
export type ToolId = string

export interface AgentRequest {
  readonly input: string
  readonly assistantId?: AssistantId
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

export interface ToolDefinition {
  readonly id: ToolId
  readonly description: string
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

/** Registry boundary for assistant definitions and configuration. */
export interface AssistantRegistry {
  listAssistants(): Promise<readonly AssistantDefinition[]>
  resolveAssistant(id: AssistantId): Promise<AssistantDefinition | undefined>
}
