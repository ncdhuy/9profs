import { expectTypeOf, test } from 'vitest'
import type {
  AgentBackend,
  AgentBackendDescriptor,
  AgentRequest,
  AgentRun,
  AssistantRegistry,
  CoreTransport,
  CreateMcpServerInput,
  McpServer,
  SkillProvider,
  ToolProvider,
} from '../src'

test('agent and registry contracts stay generic and async', () => {
  expectTypeOf<AgentBackendDescriptor>().toMatchTypeOf<{
    id: string
    availability: import('../src').AgentBackendAvailability
  }>()
  expectTypeOf<AgentBackend['run']>().toEqualTypeOf<(request: AgentRequest) => Promise<AgentRun>>()
  expectTypeOf<ToolProvider['listTools']>().returns.toEqualTypeOf<
    Promise<readonly import('../src').ToolDefinition[]>
  >()
  expectTypeOf<SkillProvider['resolveSkill']>().returns.toEqualTypeOf<
    Promise<import('../src').SkillDefinition | undefined>
  >()
  expectTypeOf<AssistantRegistry['resolveAssistant']>().returns.toEqualTypeOf<
    Promise<import('../src').AssistantDefinition | undefined>
  >()
})

test('assistant and skill transport boundary stays async and typed', () => {
  expectTypeOf<CoreTransport['assistants']>().returns.toEqualTypeOf<
    Promise<import('../src').CoreAssistant[]>
  >()
  expectTypeOf<CoreTransport['skills']>().returns.toEqualTypeOf<
    Promise<import('../src').CoreSkillCatalog>
  >()
})

test('MCP boundary stays transport-neutral and redacted', () => {
  expectTypeOf<McpServer>().toMatchTypeOf<{
    id: string
    status: import('../src').McpServerStatus
    transport: import('../src').McpTransport
  }>()
  expectTypeOf<CoreTransport['createMcpServer']>().toEqualTypeOf<
    (input: CreateMcpServerInput) => Promise<McpServer>
  >()
})
