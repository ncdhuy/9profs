import { expectTypeOf, test } from 'vitest'
import type {
  AgentBackend,
  AgentRequest,
  AgentRun,
  AssistantRegistry,
  CoreTransport,
  SkillProvider,
  ToolProvider,
} from '../src'

test('agent and registry contracts stay generic and async', () => {
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
