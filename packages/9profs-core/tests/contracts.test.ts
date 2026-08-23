import { expectTypeOf, test } from 'vitest'
import type {
  AgentBackend,
  AgentRequest,
  AgentRun,
  AssistantRegistry,
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
